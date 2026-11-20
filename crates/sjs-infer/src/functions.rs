//! Function signatures (SPEC.md §3.2, §4.4): a parameter is the union of what is
//! passed for it at every call of the function by name, with its default; the return
//! is the union of what the body returns, `undefined` when it returns nothing and
//! `never` when it only throws. A call is typed by what it calls, and judged: a count
//! of arguments the function does not take (E136), a callee that is no function or a
//! `new` of what is no class (E139), a body that returns a value on some paths only
//! (E111).

use crate::expressions::{element_span_of, nodes, punct, Facts, Typer};
use crate::mutation::arguments;
use sjs_bind::{Bindings, DeclKind};
use sjs_diag::{Code, SourceFile, Span};
use sjs_syntax::{Element, Keyword, NodeKind, NodeRef, Punct, SyntaxTree, TokenKind};
use sjs_types::{Function, Param, Type, TypeId};
use std::collections::BTreeMap;

/// One call site of a parameter: where the argument is, and its type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Site {
    /// The file of the call.
    pub path: String,
    /// The argument.
    pub span: Span,
    /// Its type.
    pub ty: TypeId,
}

/// An argument passed to an imported function: for the module that declares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Passed {
    /// The declaration of the import called.
    pub callee: usize,
    /// The place of the argument.
    pub position: usize,
    /// The call site.
    pub site: Site,
}

/// A parameter of a function, as it is written.
pub(crate) struct Parameter<'t> {
    /// Its declaration, when it is a plain name.
    pub declaration: Option<usize>,
    pub default: Option<Element<'t>>,
    pub rest: bool,
}

/// A function declared by name: `function f() {}`, `const f = () => …`.
pub(crate) struct Declared<'t> {
    /// The declaration of its name.
    pub name: usize,
    pub parameters: Vec<Parameter<'t>>,
    /// The block of its body, or the expression an arrow gives.
    pub body: Element<'t>,
    pub asynchronous: bool,
    pub generator: bool,
}

/// A call of a name.
pub(crate) struct Call<'t> {
    pub callee: usize,
    pub passed: Vec<Element<'t>>,
}

fn is_function(node: NodeRef<'_>) -> bool {
    matches!(
        node.kind(),
        NodeKind::FunctionDecl
            | NodeKind::FunctionExpr
            | NodeKind::ArrowFunction
            | NodeKind::ClassMethod
            | NodeKind::ClassAccessor
    )
}

/// The `return`s of a body that are its own: not those of a function inside it.
/// Each with its value, if it has one.
fn returns_of(body: NodeRef<'_>) -> Vec<(NodeRef<'_>, Option<Element<'_>>)> {
    let mut found = Vec::new();
    let mut pending = vec![body];
    while let Some(node) = pending.pop() {
        if node.kind() == NodeKind::ReturnStmt {
            let value = node
                .children()
                .nth(1)
                .filter(|value| punct(Some(value)).is_none());
            found.push((node, value));
        }
        pending.extend(nodes(node).filter(|inner| !is_function(*inner)));
    }
    found.sort_by_key(|(node, _)| node.span().byte_start);
    found
}

/// The block directly inside a clause.
fn block_of(part: NodeRef<'_>) -> Option<NodeRef<'_>> {
    nodes(part).find(|inner| inner.kind() == NodeKind::Block)
}

/// Whether a block never runs off its end: its last statement returns or throws, on
/// every path.
fn ends(block: NodeRef<'_>) -> bool {
    // The last statement that runs: a function declared after it is hoisted.
    let statements = nodes(block).filter(|statement| statement.kind() != NodeKind::FunctionDecl);
    let Some(last) = statements.last() else {
        return false;
    };
    match last.kind() {
        NodeKind::SwitchStmt => {
            // Every case, with a `default` among them.
            let clauses: Vec<NodeRef<'_>> = nodes(last)
                .filter(|part| part.kind() == NodeKind::CaseClause)
                .collect();
            let is_default = |clause: &NodeRef<'_>| {
                clause
                    .tokens()
                    .iter()
                    .any(|token| token.kind == TokenKind::Keyword(Keyword::Default))
            };
            let bodies = clauses.iter().filter_map(|clause| block_of(*clause));
            clauses.iter().any(is_default) && bodies.into_iter().all(ends)
        }
        NodeKind::ReturnStmt | NodeKind::ThrowStmt => true,
        NodeKind::Block => ends(last),
        NodeKind::IfStmt => {
            let has_else = last.children().any(|part| {
                matches!(&part, Element::Token(token) if token.token().kind == TokenKind::Keyword(Keyword::Else))
            });
            // The branches: the blocks, and an `else if`.
            let mut branches = nodes(last)
                .filter(|part| matches!(part.kind(), NodeKind::Block | NodeKind::IfStmt));
            has_else
                && branches.all(|branch| {
                    if branch.kind() == NodeKind::Block {
                        ends(branch)
                    } else {
                        ends_statement(branch)
                    }
                })
        }
        NodeKind::TryStmt => {
            let finally = nodes(last).find(|part| part.kind() == NodeKind::FinallyClause);
            let catch = nodes(last).find(|part| part.kind() == NodeKind::CatchClause);
            let tried = nodes(last).find(|part| part.kind() == NodeKind::Block);
            let handled = catch.and_then(block_of).is_none_or(ends);
            finally.and_then(block_of).is_some_and(ends) || (tried.is_some_and(ends) && handled)
        }
        _ => false,
    }
}

/// [`ends`] for one statement that is no block: an `else if`.
fn ends_statement(statement: NodeRef<'_>) -> bool {
    let mut branches =
        nodes(statement).filter(|part| matches!(part.kind(), NodeKind::Block | NodeKind::IfStmt));
    let has_else = statement.children().any(|part| {
        matches!(&part, Element::Token(token) if token.token().kind == TokenKind::Keyword(Keyword::Else))
    });
    has_else
        && branches.all(|branch| {
            if branch.kind() == NodeKind::Block {
                ends(branch)
            } else {
                ends_statement(branch)
            }
        })
}

/// The parameters of a function, as they are written.
fn parameters_of<'t>(
    function: NodeRef<'t>,
    declared_at: &BTreeMap<usize, usize>,
) -> Vec<Parameter<'t>> {
    let declares = |element: Option<&Element<'t>>| match element {
        Some(Element::Token(name)) => declared_at.get(&name.token().span.byte_start).copied(),
        _ => None,
    };
    let list = nodes(function).next();
    let listed = list.into_iter().flat_map(nodes);
    listed
        .map(|parameter| {
            let inside: Vec<Element<'t>> = parameter.children().collect();
            let rest = parameter.kind() == NodeKind::RestParam;
            let name = if rest { inside.last() } else { inside.first() };
            let value = nodes(parameter).find(|inner| inner.kind() == NodeKind::Initializer);
            Parameter {
                declaration: declares(name),
                default: value.and_then(|value| value.children().last()),
                rest,
            }
        })
        .collect()
}

/// The functions a file declares by name and the calls of names in it.
pub(crate) fn declared_in<'t>(
    source: &SourceFile,
    tree: &'t SyntaxTree,
    bindings: &Bindings,
) -> (Vec<Declared<'t>>, Vec<Call<'t>>) {
    let at = |starts: Vec<usize>| -> BTreeMap<usize, usize> {
        starts
            .into_iter()
            .enumerate()
            .map(|(at, start)| (start, at))
            .collect()
    };
    let declared_at = at(bindings
        .declarations
        .iter()
        .map(|d| d.span.byte_start)
        .collect());
    let referenced_at = at(bindings
        .references
        .iter()
        .map(|r| r.span.byte_start)
        .collect());
    let declares = |element: Option<&Element<'t>>| match element {
        Some(Element::Token(name)) => declared_at.get(&name.token().span.byte_start).copied(),
        _ => None,
    };
    let word = |node: NodeRef<'t>, text: &str| {
        let first = node.tokens().first().map(|token| token.span);
        first.and_then(|span| source.slice(&span)) == Some(text)
    };
    let parameters = |function: NodeRef<'t>| parameters_of(function, &declared_at);
    let mut functions = Vec::new();
    let mut calls = Vec::new();
    let mut pending = vec![tree.root()];
    while let Some(node) = pending.pop() {
        pending.extend(nodes(node));
        let children: Vec<Element<'t>> = node.children().collect();
        match node.kind() {
            NodeKind::FunctionDecl => {
                let name = children.iter().find_map(|child| declares(Some(child)));
                let body = children.last().copied();
                let generator = children
                    .iter()
                    .any(|child| punct(Some(child)) == Some(Punct::Star));
                functions.extend(name.zip(body).map(|(name, body)| Declared {
                    name,
                    parameters: parameters(node),
                    body,
                    asynchronous: word(node, "async"),
                    generator,
                }));
            }
            NodeKind::ConstDecl | NodeKind::LetDecl => {
                let value = nodes(node).find(|part| part.kind() == NodeKind::Initializer);
                let arrow = value.and_then(|value| {
                    nodes(value).find(|part| part.kind() == NodeKind::ArrowFunction)
                });
                let name = declares(children.get(1));
                let body = arrow.and_then(|arrow| arrow.children().last());
                functions.extend(
                    name.zip(arrow)
                        .zip(body)
                        .map(|((name, arrow), body)| Declared {
                            name,
                            parameters: parameters(arrow),
                            body,
                            asynchronous: word(arrow, "async"),
                            generator: false,
                        }),
                );
            }
            NodeKind::CallExpr => {
                let callee = match children.first() {
                    Some(Element::Token(name)) => referenced_at.get(&name.token().span.byte_start),
                    _ => None,
                };
                let callee = callee.and_then(|&at| bindings.references[at].declaration);
                calls.extend(callee.map(|callee| Call {
                    callee,
                    passed: arguments(&children),
                }));
            }
            _ => {}
        }
    }
    (functions, calls)
}

fn is_spread(element: &Element<'_>) -> bool {
    matches!(element, Element::Node(node) if node.kind() == NodeKind::SpreadElement)
}

impl Typer<'_> {
    /// What a function's body returns, when it can be told: the union of its `return`
    /// values without their literals, `undefined` for a body that returns none, `never`
    /// for one that only throws; a promise of that for an `async` function.
    pub(crate) fn returned(&mut self, function: &Declared<'_>) -> Option<TypeId> {
        let ty = match function.body {
            Element::Node(block) if block.kind() == NodeKind::Block => {
                let mut types = Vec::new();
                let returns = returns_of(block);
                for (_, value) in &returns {
                    let ty = match value {
                        Some(value) => self.type_of(*value)?,
                        None => TypeId::UNDEFINED,
                    };
                    types.push(self.widened(ty));
                }
                // Off the end of the body it returns nothing.
                if !ends(block) {
                    types.push(TypeId::UNDEFINED);
                }
                self.types.union(types)
            }
            expression => {
                let ty = self.type_of(expression)?;
                self.widened(ty)
            }
        };
        let ty = if function.asynchronous {
            self.types.intern(Type::Promise(ty))
        } else {
            ty
        };
        (!function.generator).then_some(ty)
    }

    /// The sites of each parameter of `function` in this file, with what `known` has
    /// of sites elsewhere: the type each parameter has from them and from its default.
    /// The type is of the calls that have one, `None` when none has (for a rest
    /// parameter it is the type of its elements); the flag says whether every call
    /// has one.
    pub(crate) fn parameter_types(
        &mut self,
        function: &Declared<'_>,
        calls: &[Call<'_>],
        known: &Facts,
    ) -> Vec<(usize, Option<TypeId>, bool, Vec<Site>)> {
        let mut typed = Vec::new();
        for (place, parameter) in function.parameters.iter().enumerate() {
            let Some(declaration) = parameter.declaration else {
                continue;
            };
            let mut sites: Vec<Site> = known
                .external_sites
                .get(&declaration)
                .cloned()
                .unwrap_or_default();
            let mut all_known = true;
            let own = calls.iter().filter(|call| call.callee == function.name);
            for call in own {
                // With a spread, which argument is which is not known.
                let spreads = call.passed.iter().any(is_spread);
                let taken: Vec<&Element<'_>> = if parameter.rest {
                    call.passed.iter().skip(place).collect()
                } else {
                    call.passed.get(place).into_iter().collect()
                };
                all_known &= !spreads;
                for argument in taken.into_iter().filter(|_| !spreads) {
                    let ty = self.type_of(*argument).map(|ty| self.loosened(ty));
                    all_known &= ty.is_some();
                    sites.extend(ty.map(|ty| Site {
                        path: self.source.path().to_owned(),
                        span: element_span_of(argument),
                        ty,
                    }));
                }
            }
            // In the order of the text, whatever order the calls were found in.
            sites.sort_by(|a, b| (&a.path, a.span.byte_start).cmp(&(&b.path, b.span.byte_start)));
            let default = parameter
                .default
                .map(|default| self.type_of(default).map(|ty| self.loosened(ty)));
            all_known &= default != Some(None);
            let mut given: Vec<TypeId> = sites.iter().map(|site| site.ty).collect();
            given.extend(default.flatten());
            let called = calls.iter().any(|call| call.callee == function.name)
                || known.external_sites.contains_key(&declaration);
            // A rest nothing is passed for holds nothing; a parameter nothing is
            // passed for has no type.
            let empty_rest = parameter.rest && called;
            let ty = (!given.is_empty() || empty_rest).then(|| self.types.union(given));
            typed.push((declaration, ty, all_known, sites));
        }
        typed
    }

    /// The type of a function whose parameters and return are known, and how many
    /// arguments it takes: at least those without a default, at most all, or any
    /// number with a rest.
    pub(crate) fn signature(
        &mut self,
        function: &Declared<'_>,
        known: &Facts,
        calls: &[Call<'_>],
    ) -> (Option<TypeId>, (usize, Option<usize>)) {
        let plain = || {
            function
                .parameters
                .iter()
                .filter(|parameter| !parameter.rest)
        };
        let required = plain()
            .take_while(|parameter| parameter.default.is_none())
            .count();
        let has_rest = function.parameters.iter().any(|parameter| parameter.rest);
        let arity = (required, (!has_rest).then(|| plain().count()));
        let returns = known.returns.get(&function.name).copied();
        let of = |parameter: &Parameter<'_>| {
            parameter
                .declaration
                .and_then(|declaration| known.declarations.get(&declaration).copied())
        };
        let mut params = Vec::new();
        for (place, parameter) in plain().enumerate() {
            let Some(ty) = of(parameter) else {
                return (None, arity);
            };
            // Optional: it has a default, and some call leaves it out.
            let omitted = calls
                .iter()
                .any(|call| call.callee == function.name && call.passed.len() <= place);
            params.push(Param {
                ty,
                optional: parameter.default.is_some() && omitted,
            });
        }
        let rest = function.parameters.iter().find(|parameter| parameter.rest);
        let rest_element = rest.map(|rest| {
            let declaration = rest.declaration;
            declaration.and_then(|declaration| known.rest.get(&declaration).copied())
        });
        let ty = returns
            .filter(|_| rest_element != Some(None))
            .map(|returns| {
                self.types.intern(Type::Function(Function {
                    type_params: Vec::new(),
                    params,
                    rest: rest_element.flatten(),
                    returns,
                }))
            });
        (ty, arity)
    }

    /// The type of a call, and what is wrong with it: a callee that is no function
    /// (E139), a count of arguments it does not take (E136).
    pub(crate) fn call(&mut self, node: NodeRef<'_>, children: &[Element<'_>]) -> Option<TypeId> {
        let first = self.first_time(node);
        let passed = arguments(children);
        // A call begins with what is called.
        let callee = children[0];
        let name = self
            .source
            .slice(&element_span_of(&callee))
            .unwrap_or_default()
            .to_owned();
        let declaration = match callee {
            Element::Token(name) => self.references.get(&name.token().span.byte_start).copied(),
            Element::Node(_) => None,
        };
        let declaration = declaration.and_then(|at| self.bindings.references[at].declaration);
        let callee_type = self.type_of(callee);
        let members = callee_type
            .map(|ty| self.members_of(ty))
            .unwrap_or_default();
        let functions: Vec<Function> = members
            .iter()
            .filter_map(|&member| match self.types.get(member) {
                Type::Function(function) => Some(function.clone()),
                _ => None,
            })
            .collect();
        if let Some(ty) = callee_type.filter(|_| functions.len() < members.len()) {
            let message = format!(
                "`{name}` is called, and is a `{}`: only a function is called (SPEC.md §4.4)",
                self.print_type(ty)
            );
            self.report(first, Code::E139, element_span_of(&callee), message);
            return None;
        }
        // How many arguments it takes: as it is declared, or as its type says.
        let declared =
            declaration.and_then(|declaration| self.facts.arity.get(&declaration).copied());
        let by_type = functions
            .first()
            .filter(|_| functions.len() == 1)
            .map(|function| {
                let required = function
                    .params
                    .iter()
                    .take_while(|param| !param.optional)
                    .count();
                (
                    required,
                    function.rest.is_none().then_some(function.params.len()),
                )
            });
        let spread = passed.iter().any(is_spread);
        if let Some((least, most)) = declared.or(by_type).filter(|_| !spread) {
            let count = passed.len();
            if count < least || most.is_some_and(|most| count > most) {
                let takes = match most {
                    Some(most) if most == least => format!("{least}"),
                    Some(most) => format!("{least} to {most}"),
                    None => format!("{least} or more"),
                };
                let message = format!(
                    "`{name}` takes {takes} argument(s) and is passed {count} (SPEC.md §4.4)"
                );
                self.report(first, Code::E136, node.span(), message);
            }
        }
        let returned =
            declaration.and_then(|declaration| self.facts.returns.get(&declaration).copied());
        returned.or_else(|| {
            let returns: Vec<TypeId> = functions.iter().map(|function| function.returns).collect();
            (!returns.is_empty()).then(|| self.types.union(returns))
        })
    }

    /// E139 for `new` on what is no class: a function, or a value that has a type.
    pub(crate) fn construction(&mut self, node: NodeRef<'_>, children: &[Element<'_>]) {
        let first = self.first_time(node);
        // What is made comes after `new`.
        let class = children.get(1).copied().unwrap_or(children[0]);
        let name = self
            .source
            .slice(&element_span_of(&class))
            .unwrap_or_default()
            .to_owned();
        let declaration = match class {
            Element::Token(name) => self.references.get(&name.token().span.byte_start).copied(),
            Element::Node(_) => None,
        };
        let declaration = declaration.and_then(|at| self.bindings.references[at].declaration);
        let kind = declaration.map(|declaration| self.bindings.declarations[declaration].kind);
        // A function is no class; nor is a value that has a type, for a class as a
        // value has none yet.
        let is_function = kind == Some(DeclKind::Function);
        if is_function || self.type_of(class).is_some() {
            let message = format!("`new` on `{name}`, which is no class (SPEC.md §4.4)");
            self.report(first, Code::E139, element_span_of(&class), message);
        }
    }

    /// E111 for a function that returns a value on some paths and nothing on others:
    /// at the `return` without a value, or at the first one with a value when the
    /// body may run off its end.
    pub(crate) fn mixed_returns(&mut self, function: NodeRef<'_>) {
        let body = nodes(function)
            .last()
            .filter(|body| body.kind() == NodeKind::Block);
        let Some(body) = body.filter(|_| is_function(function)) else {
            return;
        };
        let first = self.first_time(body);
        let returns = returns_of(body);
        let valued = returns.iter().find(|(_, value)| value.is_some());
        let bare = returns.iter().find(|(_, value)| value.is_none());
        let at = match (valued, bare) {
            (Some(_), Some((bare, _))) => Some(bare.span()),
            (Some((valued, _)), None) if !ends(body) => Some(valued.span()),
            _ => None,
        };
        if let Some(at) = at {
            let message = "this function returns a value on some paths and nothing on others: return a value on every path, or on none (SPEC.md §4.3)";
            self.report(first, Code::E111, at, message.to_owned());
        }
    }
}
