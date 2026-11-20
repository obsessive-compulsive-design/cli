//! Module purity and evaluation order (SPEC.md §5.1, §7). A module is pure when its top
//! level only declares: nothing runs when it is evaluated but the making of functions,
//! classes and literals. The order modules are evaluated in is the one of ES modules.

use crate::graph::{Content, Module, ModuleGraph};
use crate::scopes::{child, is_static};
use sjs_diag::Span;
use sjs_syntax::{Element, NodeKind, NodeRef};
use std::collections::{BTreeSet, VecDeque};
use std::path::Path;

fn nodes(node: NodeRef<'_>) -> impl Iterator<Item = NodeRef<'_>> {
    node.children().filter_map(|child| match child {
        Element::Node(inner) => Some(inner),
        Element::Token(_) => None,
    })
}

/// The first thing in the expression or declaration `node` that runs code when the
/// module is evaluated. A literal, a name, an arrow and a class make a value and run
/// nothing; so do an object or array literal and an operator over such. Anything else
/// may run code: a call, `new`, `await`, an assignment, a member read (an accessor), a
/// spread or a pattern (an iterator), a template that fills in values (a template
/// that fills in none is a token, like any literal).
fn effect(node: NodeRef<'_>) -> Option<Span> {
    let inside = || nodes(node).find_map(effect);
    match node.kind() {
        NodeKind::ArrowFunction | NodeKind::FunctionExpr | NodeKind::FunctionDecl => None,
        NodeKind::Property if child(node, NodeKind::Params).is_some() => None,
        NodeKind::ClassDecl | NodeKind::ClassExpr => {
            // Of a class, its static blocks and the values of its static fields run.
            let members = child(node, NodeKind::ClassBody).into_iter().flat_map(nodes);
            members
                .filter(|member| match member.kind() {
                    NodeKind::StaticBlock => true,
                    NodeKind::ClassField => is_static(*member),
                    _ => false,
                })
                .find_map(|member| match member.kind() {
                    NodeKind::StaticBlock => Some(member.span()),
                    _ => nodes(member).find_map(effect),
                })
        }
        NodeKind::ConstDecl
        | NodeKind::LetDecl
        | NodeKind::Initializer
        | NodeKind::ObjectLit
        | NodeKind::ArrayLit
        | NodeKind::Property
        | NodeKind::ParenExpr
        | NodeKind::UnaryExpr
        | NodeKind::BinaryExpr
        | NodeKind::CondExpr => inside(),
        _ => Some(node.span()),
    }
}

impl Module {
    /// Where the module first does something when it is evaluated, as the text reads;
    /// `None` for a pure module, whose top level is imports, exports and declarations
    /// that run nothing. A JSON module is pure, and so is a file that is no text.
    #[must_use]
    pub fn impure(&self) -> Option<Span> {
        let Content::Script { tree, .. } = &self.content else {
            return None;
        };
        nodes(tree.root()).find_map(|item| match item.kind() {
            NodeKind::ImportDecl => None,
            NodeKind::ExportDecl => nodes(item)
                .filter(|inner| inner.kind() != NodeKind::NamedExports)
                .find_map(effect),
            _ => effect(item),
        })
    }
}

impl ModuleGraph {
    /// Every module once, in the order ES modules are evaluated: from each root in the
    /// order given, what a module imports before the module, in the order of its
    /// imports (`export … from` is one). `import(…)` runs later and orders nothing: a
    /// module only it reaches comes after the roots' modules, in the order found, with
    /// what that module imports before it.
    #[must_use]
    pub fn evaluation_order(&self) -> Vec<&Module> {
        let mut order = Vec::new();
        let mut seen: BTreeSet<&Path> = BTreeSet::new();
        let mut entries: VecDeque<&Module> = self
            .roots
            .iter()
            .filter_map(|root| self.get(root))
            .collect();
        while let Some(entry) = entries.pop_front() {
            // Each module being evaluated, with the import it has come to.
            let mut work = vec![(entry, 0)];
            if !seen.insert(entry.path.as_path()) {
                work.clear();
            }
            while let Some((module, next)) = work.pop() {
                let Some(import) = module.imports.get(next) else {
                    order.push(module);
                    continue;
                };
                work.push((module, next + 1));
                let Some(target) = import.target.as_deref().and_then(|path| self.get(path)) else {
                    continue;
                };
                if import.dynamic {
                    entries.push_back(target);
                } else if seen.insert(target.path.as_path()) {
                    work.push((target, 0));
                }
            }
        }
        order
    }
}
