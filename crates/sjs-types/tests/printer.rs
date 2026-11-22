//! STORIES.md US-081: the readable notation for types.

mod common;

use common::{description, intern, Desc};
use sjs_testkit::prng::Rng;
use sjs_testkit::prop::{check, Gen};
use sjs_types::{
    Builtin, ClassId, Function, Integer, Literal, Number, Param, Printer, Property, Type, TypeId,
    Types, VarId,
};
use std::collections::BTreeMap;
use std::fmt::Write as _;

fn class(id: ClassId) -> String {
    ["Config", "ParseError", "Node"]
        .get(id.0 as usize)
        .map_or(format!("Class{}", id.0), |name| (*name).to_owned())
}

fn property(key: &str, ty: TypeId, literal: bool) -> Property {
    Property {
        key: key.to_owned(),
        ty,
        literal,
    }
}

fn function(
    types: &mut Types,
    vars: &[u32],
    params: &[(TypeId, bool)],
    rest: Option<TypeId>,
    returns: TypeId,
) -> TypeId {
    types.intern(Type::Function(Function {
        type_params: vars.iter().map(|&var| VarId(var)).collect(),
        params: params
            .iter()
            .map(|&(ty, optional)| Param { ty, optional })
            .collect(),
        rest,
        returns,
    }))
}

fn number(types: &mut Types, value: f64) -> TypeId {
    types.literal(Literal::Number(Number::new(value)))
}

fn text(types: &mut Types, value: &str) -> TypeId {
    types.literal(Literal::String(value.to_owned()))
}

#[test]
fn the_examples_of_the_spec_print_exactly() {
    let mut t = Types::new();
    let loud = t.intern(Type::Object(vec![property("loud", TypeId::BOOLEAN, false)]));
    let greet = function(
        &mut t,
        &[],
        &[(TypeId::STRING, false), (loud, false)],
        None,
        TypeId::STRING,
    );
    let (yes, no) = (
        t.literal(Literal::Boolean(true)),
        t.literal(Literal::Boolean(false)),
    );
    let (config, error) = (
        t.intern(Type::Class(ClassId(0), Vec::new())),
        t.intern(Type::Class(ClassId(1), Vec::new())),
    );
    let ok = t.intern(Type::Object(vec![
        property("ok", yes, true),
        property("value", config, false),
    ]));
    let failed = t.intern(Type::Object(vec![
        property("ok", no, true),
        property("error", error, false),
    ]));
    let result = t.union([failed, ok]);
    let parse = function(&mut t, &[], &[(TypeId::STRING, false)], None, result);
    let sum = function(
        &mut t,
        &[],
        &[(TypeId::NUMBER, false)],
        Some(TypeId::NUMBER),
        TypeId::NUMBER,
    );
    let printer = Printer::new(&t, &class);
    assert_eq!(
        printer.signature("greet", &["name", "opts"], greet),
        "greet(name: string, opts: { loud: boolean }): string"
    );
    assert_eq!(
        printer.signature("parse", &["text"], parse),
        "parse(text: string): { ok: true, value: Config } | { ok: false, error: ParseError }"
    );
    assert_eq!(
        printer.signature("limit", &[], TypeId::INT32),
        "limit: int32"
    );
    assert_eq!(
        printer.signature("sum", &["first", "xs"], sum),
        "sum(first: number, ...xs: number[]): number"
    );
    assert_eq!(
        printer.signature("sum", &["first"], sum),
        "sum(first: number, ...rest: number[]): number"
    );
    assert_eq!(
        printer.signature("result", &["unused"], result),
        format!("result: {}", printer.print(result))
    );
    assert_eq!(
        printer.signature("greet", &["name"], greet),
        "greet(name: string, b: { loud: boolean }): string"
    );
    assert_eq!(
        printer.signature("greet", &["a", "b", "c"], greet),
        "greet(a: string, b: { loud: boolean }): string"
    );
}

/// The primitives and literals.
fn simple(t: &mut Types) -> Vec<TypeId> {
    let mut lines: Vec<TypeId> = vec![
        TypeId::NUMBER,
        TypeId::INT32,
        TypeId::BIGINT,
        TypeId::STRING,
        TypeId::BOOLEAN,
        TypeId::UNDEFINED,
        TypeId::NULL,
        TypeId::UNKNOWN,
        TypeId::NEVER,
    ];
    for value in [
        0.0,
        -0.0,
        1.0,
        -1.5,
        1e3,
        123_456_789.25,
        1e21,
        1.5e22,
        1e-6,
        1e-7,
        2.5e-9,
        -1e300,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        999_999_999_999_999_900_000.0,
    ] {
        lines.push(number(t, value));
    }
    for value in ["0", "10", "-7", "123456789012345678901234567890"] {
        lines.push(t.literal(Literal::BigInt(Integer::parse(value).expect("an integer"))));
    }
    for value in [
        "",
        "a",
        "it's",
        "say \"hi\"",
        "back\\slash",
        "line\nbreak\r\ttab",
        "bell\u{7}",
        "naïve ☃",
    ] {
        lines.push(text(t, value));
    }
    lines
}

/// One of every other kind, in the forms that need care.
// A list of types, one to a line of the snapshot: long, and nothing but the list.
#[allow(clippy::too_many_lines)]
fn composite(t: &mut Types) -> Vec<TypeId> {
    let (n, s) = (TypeId::NUMBER, TypeId::STRING);
    let (yes, no) = (
        t.literal(Literal::Boolean(true)),
        t.literal(Literal::Boolean(false)),
    );
    let (a, one) = (text(t, "a"), number(t, 1.0));
    let literals = t.union([a, one]);
    let or_undefined = t.union([s, TypeId::UNDEFINED]);
    let mixed = t.union([a, n]);
    let node = t.intern(Type::Class(ClassId(2), Vec::new()));
    let boxed = t.intern(Type::Class(ClassId(2), vec![n, s]));
    assert_eq!(
        Printer::new(t, &class).print(boxed),
        format!("{}<number, string>", class(ClassId(2)))
    );
    let (var_t, var_u) = (t.intern(Type::Var(VarId(0))), t.intern(Type::Var(VarId(1))));
    let numbers = t.intern(Type::Array {
        element: n,
        mutated: true,
    });
    let frozen = t.intern(Type::Array {
        element: n,
        mutated: false,
    });
    let callback = function(t, &[], &[(n, false)], None, s);
    let callback_or = t.union([callback, TypeId::UNDEFINED]);
    let returns_union = function(t, &[], &[], None, or_undefined);
    vec![
        yes,
        no,
        t.intern(Type::Object(Vec::new())),
        t.intern(Type::Object(vec![property("a", n, false)])),
        t.intern(Type::Object(vec![
            property("kind", a, true),
            property("size", one, true),
            property("next", or_undefined, false),
        ])),
        t.intern(Type::Object(vec![
            property("both", literals, true),
            property("held", literals, false),
        ])),
        t.intern(Type::Object(vec![
            property("kept", n, true),
            property("loose", a, false),
            property("part", mixed, true),
        ])),
        t.intern(Type::Object(vec![
            property("not-an-identifier", n, false),
            property("1st", n, false),
            property("", n, false),
            property("$ok_1", n, false),
            property("with \"quotes\"", n, false),
            property("é", n, false),
        ])),
        t.intern(Type::Dictionary(n)),
        t.intern(Type::Dictionary(or_undefined)),
        numbers,
        frozen,
        t.intern(Type::Array {
            element: or_undefined,
            mutated: true,
        }),
        t.intern(Type::Array {
            element: or_undefined,
            mutated: false,
        }),
        t.intern(Type::Array {
            element: callback,
            mutated: true,
        }),
        t.intern(Type::Array {
            element: numbers,
            mutated: true,
        }),
        t.intern(Type::Array {
            element: numbers,
            mutated: false,
        }),
        t.intern(Type::Array {
            element: frozen,
            mutated: true,
        }),
        t.intern(Type::Array {
            element: frozen,
            mutated: false,
        }),
        t.intern(Type::Tuple(Vec::new())),
        t.intern(Type::Tuple(vec![n, s])),
        t.intern(Type::Tuple(vec![or_undefined, callback, numbers])),
        function(t, &[], &[], None, TypeId::UNDEFINED),
        callback,
        function(t, &[0], &[(var_t, false)], None, var_t),
        function(
            t,
            &[0, 1],
            &[(var_t, false), (var_u, true)],
            Some(var_u),
            var_t,
        ),
        function(t, &[], &[(n, true)], None, n),
        function(t, &[], &[], Some(n), n),
        function(t, &[], &[(s, false)], Some(or_undefined), n),
        function(t, &[], &[], Some(callback), n),
        function(t, &[], &[], Some(frozen), n),
        function(t, &[], &[(callback, false)], None, callback),
        returns_union,
        callback_or,
        t.union([returns_union, callback, n]),
        function(t, &[7, 6], &[], None, n),
        function(t, &[], &vec![(n, false); 28], None, n),
        node,
        t.intern(Type::Class(ClassId(9), Vec::new())),
        var_t,
        var_u,
        t.intern(Type::Var(VarId(6))),
        t.intern(Type::Var(VarId(7))),
        t.intern(Type::Promise(n)),
        t.intern(Type::Promise(or_undefined)),
        t.intern(Type::Promise(callback)),
        t.intern(Type::Iterator {
            item: n,
            asynchronous: false,
        }),
        t.intern(Type::Iterator {
            item: s,
            asynchronous: true,
        }),
        t.intern(Type::Generator {
            yields: n,
            returns: s,
            asynchronous: false,
        }),
        t.intern(Type::Generator {
            yields: n,
            returns: TypeId::UNDEFINED,
            asynchronous: true,
        }),
        t.intern(Type::Builtin(Builtin::Map, vec![s, n])),
        t.intern(Type::Builtin(Builtin::Set, vec![or_undefined])),
        t.intern(Type::Builtin(Builtin::WeakMap, vec![node, callback])),
        or_undefined,
        t.union([
            n,
            s,
            TypeId::BOOLEAN,
            TypeId::NULL,
            TypeId::UNDEFINED,
            a,
            one,
            node,
            yes,
        ]),
        t.union([TypeId::INT32, TypeId::BIGINT, numbers, frozen]),
    ]
}

#[test]
fn every_kind_of_type_has_its_notation() {
    let mut t = Types::new();
    let mut lines = simple(&mut t);
    lines.extend(composite(&mut t));
    let printer = Printer::new(&t, &class);
    let mut out = String::new();
    for id in lines {
        writeln!(out, "{}", printer.print(id)).expect("a string");
        assert_eq!(printer.print(id), printer.print(id), "printing is stable");
    }
    sjs_testkit::snapshot!("notation", &out);
}

/// A few types to intern together.
struct Several;

impl Gen for Several {
    type Value = Vec<Desc>;

    fn generate(&self, rng: &mut Rng) -> Self::Value {
        (0..4).map(|_| description(rng, 0)).collect()
    }

    fn shrink(&self, _: &Self::Value) -> Vec<Self::Value> {
        Vec::new()
    }
}

#[test]
fn printing_is_injective_on_the_interned_set() {
    check(&Several, |several| {
        let mut types = Types::new();
        for desc in several {
            intern(&mut types, desc);
        }
        let printer = Printer::new(&types, &class);
        let mut seen: BTreeMap<String, TypeId> = BTreeMap::new();
        for id in types.ids() {
            let text = printer.print(id);
            assert!(!text.is_empty(), "printing is total");
            if let Some(other) = seen.insert(text.clone(), id) {
                return Err(format!(
                    "{:?} and {:?} both print `{text}`",
                    types.get(other),
                    types.get(id)
                ));
            }
        }
        assert_eq!(seen.len(), types.count());
        Ok(())
    });
}
