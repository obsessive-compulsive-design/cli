//! The readable notation for types (SPEC.md §13.7, §10.6): output only, never read
//! back. Printing is total, depends on the type alone, and tells any two types apart
//! as long as the classes are given names that differ.

use crate::{Builtin, ClassId, Function, Literal, Property, Type, TypeId, Types, VarId};

/// Prints the types of a [`Types`], naming classes as it is told to.
pub struct Printer<'a> {
    types: &'a Types,
    class: &'a dyn Fn(ClassId) -> String,
}

/// A string as JavaScript writes one, in double quotes.
fn quoted(text: &str) -> String {
    let escaped = text.chars().map(|c| match c {
        '"' => "\\\"".to_owned(),
        '\\' => "\\\\".to_owned(),
        '\n' => "\\n".to_owned(),
        '\r' => "\\r".to_owned(),
        '\t' => "\\t".to_owned(),
        c if c.is_control() => format!("\\u{{{:x}}}", u32::from(c)),
        c => c.to_string(),
    });
    format!("\"{}\"", escaped.collect::<String>())
}

fn is_identifier(key: &str) -> bool {
    let start = |c: char| c.is_ascii_alphabetic() || c == '_' || c == '$';
    let mut chars = key.chars();
    chars.next().is_some_and(start) && chars.all(|c| start(c) || c.is_ascii_digit())
}

/// The name of a type variable: `T`, `U`, … `Z`, then `T7`, `T8`, ….
fn variable(var: VarId) -> String {
    let letters = ["T", "U", "V", "W", "X", "Y", "Z"];
    let letter = usize::try_from(var.0).ok().and_then(|at| letters.get(at));
    letter.map_or(format!("T{}", var.0), |letter| (*letter).to_owned())
}

/// The name of the parameter at `at` when none is given: `a`, `b`, … `z`, then `p26`,
/// `p27`, ….
fn parameter(at: usize) -> String {
    let letter = u8::try_from(at).ok().filter(|at| *at < 26);
    letter.map_or(format!("p{at}"), |at| char::from(b'a' + at).to_string())
}

impl<'a> Printer<'a> {
    /// A printer of `types` that names each class by `class`.
    #[must_use]
    pub fn new(types: &'a Types, class: &'a dyn Fn(ClassId) -> String) -> Self {
        Self { types, class }
    }

    /// The type in the notation: `string | undefined`, `{ ok: true, value: Config }`,
    /// `readonly number[]`, `<T>(a: T): T`.
    #[must_use]
    pub fn print(&self, id: TypeId) -> String {
        let list = |ids: &[TypeId]| -> String {
            let printed: Vec<String> = ids.iter().map(|&id| self.print(id)).collect();
            printed.join(", ")
        };
        match self.types.get(id) {
            Type::Number => "number".to_owned(),
            Type::Int32 => "int32".to_owned(),
            Type::BigInt => "bigint".to_owned(),
            Type::String => "string".to_owned(),
            Type::Boolean => "boolean".to_owned(),
            Type::Null => "null".to_owned(),
            Type::Undefined => "undefined".to_owned(),
            Type::Unknown => "unknown".to_owned(),
            Type::Never => "never".to_owned(),
            Type::Literal(Literal::Number(value)) => value.text(),
            Type::Literal(Literal::BigInt(value)) => format!("{}n", value.decimal()),
            Type::Literal(Literal::String(value)) => quoted(value),
            Type::Literal(Literal::Boolean(value)) => value.to_string(),
            Type::Object(properties) if properties.is_empty() => "{}".to_owned(),
            Type::Object(properties) => {
                let printed: Vec<String> = properties.iter().map(|p| self.property(p)).collect();
                format!("{{ {} }}", printed.join(", "))
            }
            Type::Dictionary(value) => format!("{{ [key: string]: {} }}", self.print(*value)),
            Type::Array { element, mutated } => {
                let readonly = if *mutated { "" } else { "readonly " };
                format!("{readonly}{}[]", self.element(*element))
            }
            Type::Tuple(elements) => format!("[{}]", list(elements)),
            Type::Function(function) => self.function(function, &[]),
            Type::Class(class, arguments) if arguments.is_empty() => (self.class)(*class),
            Type::Class(class, arguments) => {
                format!("{}<{}>", (self.class)(*class), list(arguments))
            }
            Type::Var(var) => variable(*var),
            Type::Promise(value) => format!("Promise<{}>", self.print(*value)),
            Type::Iterator { item, asynchronous } => {
                let name = if *asynchronous {
                    "AsyncIterator"
                } else {
                    "Iterator"
                };
                format!("{name}<{}>", self.print(*item))
            }
            Type::Generator {
                yields,
                returns,
                asynchronous,
            } => {
                let name = if *asynchronous {
                    "AsyncGenerator"
                } else {
                    "Generator"
                };
                format!("{name}<{}>", list(&[*yields, *returns]))
            }
            Type::Builtin(builtin, arguments) => {
                let name = match builtin {
                    Builtin::Map => "Map",
                    Builtin::Set => "Set",
                    Builtin::WeakMap => "WeakMap",
                };
                format!("{name}<{}>", list(arguments))
            }
            Type::Union(members) => {
                let printed: Vec<String> = members.iter().map(|&m| self.member(m)).collect();
                printed.join(" | ")
            }
        }
    }

    /// The signature of a binding named `name`: `greet(name: string): string` for a
    /// function, its parameters named by `params` as far as they go, and
    /// `name: type` for anything else.
    #[must_use]
    pub fn signature(&self, name: &str, params: &[&str], id: TypeId) -> String {
        match self.types.get(id) {
            Type::Function(function) => format!("{name}{}", self.function(function, params)),
            _ => format!("{name}: {}", self.print(id)),
        }
    }

    /// A member of a union: a function goes in parentheses, for its return type would
    /// take the members after it.
    fn member(&self, id: TypeId) -> String {
        match self.types.get(id) {
            Type::Function(_) => format!("({})", self.print(id)),
            _ => self.print(id),
        }
    }

    /// The element type before `[]`: a union and a function go in parentheses, and so
    /// does an array that says `readonly`.
    fn element(&self, id: TypeId) -> String {
        let bare = !matches!(
            self.types.get(id),
            Type::Union(_) | Type::Function(_) | Type::Array { mutated: false, .. }
        );
        if bare {
            self.print(id)
        } else {
            format!("({})", self.print(id))
        }
    }

    /// `key: type`. A key that is no identifier is quoted. A key whose literal type is
    /// retained reads as its literal, which is the usual case and has no mark; the
    /// two unusual ones are marked: `literal key` retains a type that is no literal,
    /// `mutable key` holds a literal type it does not retain.
    fn property(&self, property: &Property) -> String {
        let is_literal = |id: &TypeId| matches!(self.types.get(*id), Type::Literal(_));
        let literal = match self.types.get(property.ty) {
            Type::Union(members) => members.iter().all(is_literal),
            _ => is_literal(&property.ty),
        };
        let mark = match (property.literal, literal) {
            (true, false) => "literal ",
            (false, true) => "mutable ",
            _ => "",
        };
        let key = if is_identifier(&property.key) {
            property.key.clone()
        } else {
            quoted(&property.key)
        };
        format!("{mark}{key}: {}", self.print(property.ty))
    }

    /// `<T>(a: T, b?: U, ...rest: V[]): R`, the parameters named by `names` as far as
    /// they go and by their place after that.
    fn function(&self, function: &Function, names: &[&str]) -> String {
        let name = |at: usize| {
            names
                .get(at)
                .map_or(parameter(at), |name| (*name).to_owned())
        };
        let mut params: Vec<String> = Vec::new();
        for (at, param) in function.params.iter().enumerate() {
            let optional = if param.optional { "?" } else { "" };
            params.push(format!("{}{optional}: {}", name(at), self.print(param.ty)));
        }
        if let Some(rest) = function.rest {
            let at = function.params.len();
            let given = names
                .get(at)
                .map_or("rest".to_owned(), |name| (*name).to_owned());
            params.push(format!("...{given}: {}[]", self.element(rest)));
        }
        let variables: Vec<String> = function.type_params.iter().map(|v| variable(*v)).collect();
        let generic = if variables.is_empty() {
            String::new()
        } else {
            format!("<{}>", variables.join(", "))
        };
        format!(
            "{generic}({}): {}",
            params.join(", "),
            self.print(function.returns)
        )
    }
}
