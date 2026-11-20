//! A generator of types for the property tests: a type as a description, which is
//! interned as it is written.
#![allow(dead_code)]

use sjs_testkit::prng::Rng;
use sjs_types::{
    Builtin, ClassId, Function, Integer, Literal, Number, Param, Property, Type, TypeId, Types,
    VarId,
};

/// A type as written, before any normalization.
#[derive(Debug, Clone)]
pub enum Desc {
    Primitive(usize),
    Number(f64),
    BigInt(&'static str),
    Text(&'static str),
    Truth(bool),
    Object(Vec<(&'static str, Desc, bool)>),
    Dictionary(Box<Desc>),
    Array(Box<Desc>, bool),
    Tuple(Vec<Desc>),
    Function(Vec<(Desc, bool)>, Option<Box<Desc>>, Box<Desc>),
    Class(u32),
    Var(u32),
    Promise(Box<Desc>),
    Iterator(Box<Desc>, bool),
    Generator(Box<Desc>, Box<Desc>, bool),
    Builtin(usize, Vec<Desc>),
    Union(Vec<Desc>),
}

pub const PRIMITIVES: [(&str, TypeId); 9] = [
    ("number", TypeId::NUMBER),
    ("int32", TypeId::INT32),
    ("bigint", TypeId::BIGINT),
    ("string", TypeId::STRING),
    ("boolean", TypeId::BOOLEAN),
    ("undefined", TypeId::UNDEFINED),
    ("null", TypeId::NULL),
    ("unknown", TypeId::UNKNOWN),
    ("never", TypeId::NEVER),
];
pub const BUILTINS: [Builtin; 3] = [Builtin::Map, Builtin::Set, Builtin::WeakMap];
pub const NUMBERS: [f64; 6] = [0.0, -0.0, 1.0, 1.5, -1.0, 2_147_483_648.0];

pub fn description(rng: &mut Rng, depth: u32) -> Desc {
    let boxed = |rng: &mut Rng| Box::new(description(rng, depth + 1));
    let several = |rng: &mut Rng, most: usize| -> Vec<Desc> {
        (0..rng.index(most + 1))
            .map(|_| description(rng, depth + 1))
            .collect()
    };
    // Past the depth, only what holds no type.
    let kinds = if depth >= 3 { 7 } else { 17 };
    match rng.index(kinds) {
        0 | 1 => Desc::Primitive(rng.index(PRIMITIVES.len())),
        2 => Desc::Number(NUMBERS[rng.index(NUMBERS.len())]),
        3 => Desc::BigInt(["0", "1", "-1", "10"][rng.index(4)]),
        4 => Desc::Text(["a", "b", ""][rng.index(3)]),
        5 => Desc::Truth(rng.chance(1, 2)),
        6 => [
            Desc::Class([0u32, 1][rng.index(2)]),
            Desc::Var([0u32, 1][rng.index(2)]),
        ][rng.index(2)]
        .clone(),
        7 => {
            let count = rng.index(4);
            let keys = ["a", "b", "c"];
            Desc::Object(
                (0..count)
                    .map(|_| {
                        (
                            keys[rng.index(3)],
                            description(rng, depth + 1),
                            rng.chance(1, 3),
                        )
                    })
                    .collect(),
            )
        }
        8 => Desc::Dictionary(boxed(rng)),
        9 => Desc::Array(boxed(rng), rng.chance(1, 2)),
        10 => Desc::Tuple(several(rng, 3)),
        11 => {
            let params = several(rng, 2);
            let params = params.into_iter().map(|p| (p, rng.chance(1, 3))).collect();
            let rest = rng.chance(1, 3).then(|| boxed(rng));
            Desc::Function(params, rest, boxed(rng))
        }
        12 => Desc::Promise(boxed(rng)),
        13 => Desc::Iterator(boxed(rng), rng.chance(1, 2)),
        14 => Desc::Generator(boxed(rng), boxed(rng), rng.chance(1, 2)),
        15 => Desc::Builtin(rng.index(BUILTINS.len()), several(rng, 2)),
        _ => Desc::Union(several(rng, 4)),
    }
}

/// Interns a description as it is written.
pub fn intern(types: &mut Types, desc: &Desc) -> TypeId {
    let mut inner = |desc: &Desc| intern(types, desc);
    let ty = match desc {
        Desc::Primitive(at) => return PRIMITIVES[*at].1,
        Desc::Number(value) => Type::Literal(Literal::Number(Number::new(*value))),
        Desc::BigInt(text) => {
            Type::Literal(Literal::BigInt(Integer::parse(text).expect("an integer")))
        }
        Desc::Text(text) => Type::Literal(Literal::String((*text).to_owned())),
        Desc::Truth(value) => Type::Literal(Literal::Boolean(*value)),
        Desc::Object(properties) => Type::Object(
            properties
                .iter()
                .map(|(key, value, literal)| Property {
                    key: (*key).to_owned(),
                    ty: inner(value),
                    literal: *literal,
                })
                .collect(),
        ),
        Desc::Dictionary(value) => Type::Dictionary(inner(value)),
        Desc::Array(element, mutated) => Type::Array {
            element: inner(element),
            mutated: *mutated,
        },
        Desc::Tuple(elements) => Type::Tuple(elements.iter().map(inner).collect()),
        Desc::Function(params, rest, returns) => Type::Function(Function {
            type_params: Vec::new(),
            params: params
                .iter()
                .map(|(p, optional)| Param {
                    ty: inner(p),
                    optional: *optional,
                })
                .collect(),
            rest: rest.as_ref().map(|rest| inner(rest)),
            returns: inner(returns),
        }),
        Desc::Class(id) => Type::Class(ClassId(*id)),
        Desc::Var(id) => Type::Var(VarId(*id)),
        Desc::Promise(value) => Type::Promise(inner(value)),
        Desc::Iterator(item, asynchronous) => Type::Iterator {
            item: inner(item),
            asynchronous: *asynchronous,
        },
        Desc::Generator(yields, returns, asynchronous) => Type::Generator {
            yields: inner(yields),
            returns: inner(returns),
            asynchronous: *asynchronous,
        },
        Desc::Builtin(builtin, arguments) => {
            Type::Builtin(BUILTINS[*builtin], arguments.iter().map(inner).collect())
        }
        Desc::Union(members) => Type::Union(members.iter().map(inner).collect()),
    };
    types.intern(ty)
}
