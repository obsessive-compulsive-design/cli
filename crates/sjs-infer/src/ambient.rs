//! The classes of the ambient library beyond the typed arrays (SPEC.md §6.1):
//! `RegExp`, `Date`, `Error` and its family, and the constructors of `Intl`, which
//! are read from `ambient/intl.toml`. Each is told to [`Facts`] as a class is, with
//! the members §6.1 keeps; a member it removes is E133 where it is read.

use crate::expressions::{ClassFacts, Facts};
use crate::typed_arrays::{ambient_class, taking_any};
use sjs_types::{ClassId, Type, TypeId, Types};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, OnceLock};

/// The classes this module tells of, after the typed arrays and the buffers, in
/// the order of their ids; the constructors of `Intl` come after them.
pub(crate) const NAMED: [&str; 10] = [
    "RegExp",
    "Date",
    "Error",
    "AggregateError",
    "EvalError",
    "RangeError",
    "ReferenceError",
    "SyntaxError",
    "TypeError",
    "URIError",
];

/// The getters of a `Date` that read it as UTC, which are kept.
const UTC_GETTERS: [&str; 8] = [
    "getUTCFullYear",
    "getUTCMonth",
    "getUTCDate",
    "getUTCDay",
    "getUTCHours",
    "getUTCMinutes",
    "getUTCSeconds",
    "getUTCMilliseconds",
];

/// The members of a `Date` that are removed: the setters, and the getters that read
/// it in the local time zone.
const REMOVED_OF_DATE: [&str; 27] = [
    "getFullYear",
    "getMonth",
    "getDate",
    "getDay",
    "getHours",
    "getMinutes",
    "getSeconds",
    "getMilliseconds",
    "getTimezoneOffset",
    "getYear",
    "setFullYear",
    "setMonth",
    "setDate",
    "setHours",
    "setMinutes",
    "setSeconds",
    "setMilliseconds",
    "setTime",
    "setYear",
    "setUTCFullYear",
    "setUTCMonth",
    "setUTCDate",
    "setUTCHours",
    "setUTCMinutes",
    "setUTCSeconds",
    "setUTCMilliseconds",
    "toGMTString",
];

/// The flags of a `RegExp` it tells of itself.
const FLAGS_OF_REGEXP: [&str; 8] = [
    "global",
    "ignoreCase",
    "multiline",
    "dotAll",
    "unicode",
    "unicodeSets",
    "sticky",
    "hasIndices",
];

/// One table of `ambient/intl.toml`: its name, and its members with what each is.
pub(crate) type IntlTable = (String, Vec<(String, String)>);

/// Reads the tables of a file written as `ambient/intl.toml` is: `[Name]`, then
/// `member = "what it is"` lines; `#` starts a comment.
pub(crate) fn intl_tables(text: &str) -> Vec<IntlTable> {
    let mut tables: Vec<IntlTable> = Vec::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or_default().trim();
        let name = line
            .strip_prefix('[')
            .and_then(|name| name.strip_suffix(']'));
        if let Some(name) = name {
            tables.push((name.to_owned(), Vec::new()));
        }
        let member = line.split_once('=').zip(tables.last_mut());
        if let Some(((key, what), (_, members))) = member {
            let what = what.trim().trim_matches('"');
            members.push((key.trim().to_owned(), what.to_owned()));
        }
    }
    tables
}

/// The tables the compiler carries: `ambient/intl.toml`.
pub(crate) fn intl() -> &'static [IntlTable] {
    static TABLES: OnceLock<Vec<IntlTable>> = OnceLock::new();
    TABLES.get_or_init(|| intl_tables(include_str!("../../../ambient/intl.toml")))
}

/// The type a word of `ambient/intl.toml` names; `unknown` for any other.
pub(crate) fn named_type(types: &mut Types, word: &str) -> TypeId {
    let list = |types: &mut Types, element: TypeId| {
        types.intern(Type::Array {
            element,
            mutated: false,
        })
    };
    match word {
        "number" => TypeId::NUMBER,
        "string" => TypeId::STRING,
        "string[]" => list(types, TypeId::STRING),
        "unknown[]" => list(types, TypeId::UNKNOWN),
        "string | undefined" => types.union([TypeId::STRING, TypeId::UNDEFINED]),
        _ => TypeId::UNKNOWN,
    }
}

/// What is known of one class, as it is told.
struct Told {
    fields: BTreeMap<String, TypeId>,
    methods: BTreeSet<String>,
    known: ClassFacts,
}

impl Told {
    fn field(&mut self, name: &str, ty: TypeId) {
        self.known.declared.insert(name.to_owned());
        self.fields.insert(name.to_owned(), ty);
    }

    fn method(&mut self, types: &mut Types, name: &str, returns: TypeId) {
        self.known.declared.insert(name.to_owned());
        let signature = taking_any(types, returns);
        self.known.signatures.insert(name.to_owned(), signature);
        self.methods.insert(name.to_owned());
    }
}

/// Tells `facts` the classes of this module.
pub(crate) fn add_named_classes(facts: &mut Facts, types: &mut Types) {
    let error = ambient_class("Error");
    let constructors = intl().iter().filter(|(name, _)| name != "Intl");
    let constructors = constructors.map(|(name, _)| format!("Intl.{name}"));
    let names = NAMED.iter().map(|name| (*name).to_owned());
    for name in names.chain(constructors) {
        let class = ambient_class(&name).unwrap_or(ClassId(u32::MAX));
        let instance = types.intern(Type::Class(class, Vec::new()));
        let mut told = Told {
            fields: BTreeMap::new(),
            methods: BTreeSet::new(),
            known: ClassFacts {
                constructor: Some(taking_any(types, instance)),
                ..ClassFacts::default()
            },
        };
        match name.as_str() {
            "RegExp" => {
                for field in ["source", "flags"] {
                    told.field(field, TypeId::STRING);
                }
                for flag in FLAGS_OF_REGEXP {
                    told.field(flag, TypeId::BOOLEAN);
                }
                told.field("lastIndex", TypeId::INT32);
                told.known.unwritten.insert("lastIndex".to_owned());
                told.known.removed.insert("compile".to_owned());
                // What `exec` gives is told by the literal: see `regex.rs`.
                told.known.declared.insert("exec".to_owned());
                told.methods.insert("exec".to_owned());
                told.method(types, "test", TypeId::BOOLEAN);
                told.method(types, "toString", TypeId::STRING);
            }
            "Date" => {
                for getter in UTC_GETTERS.into_iter().chain(["getTime", "valueOf"]) {
                    told.method(types, getter, TypeId::NUMBER);
                }
                told.method(types, "toISOString", TypeId::STRING);
                let removed = REMOVED_OF_DATE.iter().map(|name| (*name).to_owned());
                told.known.removed.extend(removed);
            }
            name if name.starts_with("Intl.") => {
                let table = intl()
                    .iter()
                    .find(|(table, _)| name.ends_with(table.as_str()));
                for (member, what) in table.into_iter().flat_map(|(_, members)| members) {
                    let bound = what.strip_prefix("bound ");
                    let field = what.strip_prefix("field ");
                    let returns = named_type(types, bound.or(field).unwrap_or(what));
                    match (bound, field) {
                        (Some(_), _) => told.field(member, taking_any(types, returns)),
                        (None, Some(_)) => told.field(member, returns),
                        (None, None) => told.method(types, member, returns),
                    }
                }
            }
            _ => {
                for field in ["message", "name"] {
                    told.field(field, TypeId::STRING);
                }
                let stack = types.union([TypeId::STRING, TypeId::UNDEFINED]);
                told.field("stack", stack);
                told.field("cause", TypeId::UNKNOWN);
                told.method(types, "toString", TypeId::STRING);
                if name == "AggregateError" {
                    told.field("errors", named_type(types, "unknown[]"));
                }
                // Every error but `Error` itself is one.
                let base = error.filter(|error| *error != class);
                facts.superclass.extend(base.map(|base| (class, base)));
                told.known.base_arguments = Some(Vec::new());
            }
        }
        facts.class_names.insert(class, name);
        facts.fields.insert(class, Arc::new(told.fields));
        facts.methods.insert(class, Arc::new(told.methods));
        facts.classes.insert(class, Arc::new(told.known));
    }
}
