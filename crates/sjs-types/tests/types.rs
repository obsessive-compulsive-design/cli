//! STORIES.md US-080: interned types and the normal form of a union.

use sjs_testkit::prng::Rng;
use sjs_types::{
    Builtin, ClassId, Function, Integer, Literal, Number, Param, Property, Type, TypeId, Types,
    VarId,
};
use std::cmp::Ordering;

fn number(types: &mut Types, value: f64) -> TypeId {
    types.literal(Literal::Number(Number::new(value)))
}

fn string(types: &mut Types, value: &str) -> TypeId {
    types.literal(Literal::String(value.to_owned()))
}

fn boolean(types: &mut Types, value: bool) -> TypeId {
    types.literal(Literal::Boolean(value))
}

fn bigint(types: &mut Types, value: &str) -> TypeId {
    types.literal(Literal::BigInt(Integer::parse(value).expect("an integer")))
}

fn property(key: &str, ty: TypeId, literal: bool) -> Property {
    Property {
        key: key.to_owned(),
        ty,
        literal,
    }
}

fn function(params: &[(TypeId, bool)], rest: Option<TypeId>, returns: TypeId) -> Type {
    Type::Function(Function {
        type_params: Vec::new(),
        params: params
            .iter()
            .map(|&(ty, optional)| Param { ty, optional })
            .collect(),
        rest,
        returns,
    })
}

fn members(types: &Types, id: TypeId) -> Vec<TypeId> {
    match types.get(id) {
        Type::Union(members) => members.clone(),
        _ => vec![id],
    }
}

#[test]
fn the_primitives_are_interned_at_their_ids() {
    let types = Types::default();
    let primitives = [
        (TypeId::NUMBER, Type::Number),
        (TypeId::INT32, Type::Int32),
        (TypeId::BIGINT, Type::BigInt),
        (TypeId::STRING, Type::String),
        (TypeId::BOOLEAN, Type::Boolean),
        (TypeId::UNDEFINED, Type::Undefined),
        (TypeId::NULL, Type::Null),
        (TypeId::UNKNOWN, Type::Unknown),
        (TypeId::NEVER, Type::Never),
    ];
    assert_eq!(types.count(), primitives.len());
    let mut again = Types::new();
    for (id, ty) in primitives {
        assert_eq!(types.get(id), &ty);
        assert_eq!(again.intern(ty), id);
    }
    assert_eq!(again.count(), types.count(), "nothing new was interned");
}

#[test]
fn a_union_is_a_normalized_set() {
    let mut t = Types::new();
    let a = string(&mut t, "a");
    let class = t.intern(Type::Class(ClassId(1)));
    for (x, y) in [
        (a, class),
        (TypeId::NUMBER, TypeId::STRING),
        (TypeId::NULL, a),
    ] {
        assert_eq!(
            t.union([x, y]),
            t.union([y, x]),
            "norm(a | b) == norm(b | a)"
        );
        assert_ne!(t.union([x, y]), x);
    }
    assert_eq!(
        t.union([a, TypeId::STRING]),
        TypeId::STRING,
        "\"a\" | string"
    );
    assert_eq!(t.union([TypeId::STRING, a]), TypeId::STRING);
    assert_eq!(t.union([TypeId::INT32, TypeId::NUMBER]), TypeId::NUMBER);
    assert_eq!(t.union([TypeId::NUMBER, TypeId::INT32]), TypeId::NUMBER);
    for ty in [
        a,
        class,
        TypeId::STRING,
        TypeId::UNDEFINED,
        TypeId::NEVER,
        TypeId::UNKNOWN,
    ] {
        assert_eq!(t.union([TypeId::NEVER, ty]), ty, "never | T");
        assert_eq!(t.union([ty, TypeId::NEVER]), ty);
        assert_eq!(t.union([ty, ty]), ty, "T | T");
        assert_eq!(t.union([ty]), ty, "one member is itself");
        assert_eq!(
            t.union([TypeId::UNKNOWN, ty]),
            TypeId::UNKNOWN,
            "unknown | T"
        );
        assert_eq!(t.union([ty, TypeId::UNKNOWN]), TypeId::UNKNOWN);
    }
    assert_eq!(t.union([]), TypeId::NEVER, "no member is no value");
    assert_eq!(t.union([TypeId::NEVER, TypeId::NEVER]), TypeId::NEVER);
    let both = t.union([a, class]);
    assert_eq!(members(&t, both).len(), 2);
    assert_eq!(
        t.union([both, a, class, both, TypeId::NEVER]),
        both,
        "flat, each once"
    );
    let nested = t.union([both, TypeId::NULL]);
    assert_eq!(members(&t, nested), [a, class, TypeId::NULL]);
    assert_eq!(t.intern(Type::Union(vec![TypeId::NULL, class, a])), nested);
    assert_eq!(
        t.intern(Type::Union(vec![a, a])),
        a,
        "a union is interned normalized"
    );
    assert_eq!(t.intern(Type::Union(Vec::new())), TypeId::NEVER);
}

#[test]
fn a_literal_is_absorbed_by_its_primitive_and_by_nothing_else() {
    let mut t = Types::new();
    let (one, half, zero, minus_zero) = (
        number(&mut t, 1.0),
        number(&mut t, 1.5),
        number(&mut t, 0.0),
        number(&mut t, -0.0),
    );
    let big = number(&mut t, 2_147_483_648.0);
    let (yes, no) = (boolean(&mut t, true), boolean(&mut t, false));
    let (a, n1) = (string(&mut t, "a"), bigint(&mut t, "1"));
    assert_eq!(t.union([one, TypeId::INT32]), TypeId::INT32);
    assert_eq!(t.union([zero, TypeId::INT32]), TypeId::INT32);
    assert_eq!(
        t.union([one, TypeId::NUMBER]),
        TypeId::NUMBER,
        "an int32 literal, by number too"
    );
    assert_eq!(t.union([half, TypeId::NUMBER]), TypeId::NUMBER);
    assert_eq!(
        t.union([one, half, TypeId::INT32, TypeId::NUMBER]),
        TypeId::NUMBER
    );
    for other in [half, minus_zero, big] {
        let kept = t.union([other, TypeId::INT32]);
        assert_eq!(
            members(&t, kept),
            [TypeId::INT32, other],
            "no int32, so int32 keeps it apart"
        );
    }
    assert_eq!(t.union([n1, TypeId::BIGINT]), TypeId::BIGINT);
    assert_eq!(t.union([yes, TypeId::BOOLEAN]), TypeId::BOOLEAN);
    assert_eq!(
        t.union([yes, no]),
        TypeId::BOOLEAN,
        "true | false is boolean"
    );
    assert_eq!(t.union([no, yes, a]), t.union([a, TypeId::BOOLEAN]));
    let pair = t.union([yes, a]);
    assert_eq!(members(&t, pair), [a, yes], "one of the two is itself");
    for (literal, primitive) in [
        (one, TypeId::STRING),
        (a, TypeId::NUMBER),
        (n1, TypeId::NUMBER),
        (a, TypeId::BOOLEAN),
        (yes, TypeId::STRING),
        (one, TypeId::BIGINT),
    ] {
        let kept = t.union([literal, primitive]);
        assert_eq!(
            members(&t, kept).len(),
            2,
            "another primitive absorbs nothing"
        );
    }
    let nullish = t.union([TypeId::NULL, TypeId::UNDEFINED]);
    assert_eq!(
        members(&t, nullish),
        [TypeId::NULL, TypeId::UNDEFINED],
        "null is not undefined"
    );
    assert_ne!(t.union([TypeId::NULL, a]), t.union([TypeId::UNDEFINED, a]));
    assert_eq!(
        t.union([TypeId::INT32]),
        TypeId::INT32,
        "int32 alone stays int32"
    );
}

#[test]
fn a_union_of_a_thousand_literals_is_one_type_however_it_is_built() {
    let mut t = Types::new();
    let mut literals: Vec<TypeId> = (0..1_000)
        .map(|n| string(&mut t, &format!("k{n:04}")))
        .collect();
    let sorted = literals.clone();
    let union = t.union(literals.clone());
    assert_eq!(members(&t, union), sorted, "in the order of their values");
    let mut rng = Rng::from_seed(80);
    for _ in 0..5 {
        for at in (1..literals.len()).rev() {
            literals.swap(at, rng.index(at + 1));
        }
        assert_eq!(t.union(literals.clone()), union);
    }
    let halves = [
        t.union(literals[..500].to_vec()),
        t.union(literals[500..].to_vec()),
    ];
    assert_eq!(t.union(halves), union);
    assert_eq!(t.union([union, TypeId::STRING]), TypeId::STRING);
}

#[test]
fn one_id_for_one_type_and_another_for_any_other() {
    let mut t = Types::new();
    let (n, s) = (TypeId::NUMBER, TypeId::STRING);
    let shape = t.intern(Type::Object(vec![
        property("a", n, false),
        property("b", s, true),
    ]));
    let other_order = t.intern(Type::Object(vec![
        property("b", s, true),
        property("a", n, false),
    ]));
    assert_eq!(
        shape, other_order,
        "key order does not change a shape's identity"
    );
    let twice = t.intern(Type::Object(vec![
        property("b", n, false),
        property("a", n, false),
        property("b", s, true),
    ]));
    assert_eq!(twice, shape, "of a key given twice the last stays");
    assert_ne!(
        shape,
        t.intern(Type::Object(vec![
            property("a", n, false),
            property("b", s, false)
        ])),
        "the literal flag"
    );
    assert_ne!(shape, t.intern(Type::Object(vec![property("a", n, false)])));
    assert_eq!(
        t.intern(Type::Object(Vec::new())),
        t.intern(Type::Object(Vec::new()))
    );
    let array = t.intern(Type::Array {
        element: n,
        mutated: false,
    });
    let tuple = t.intern(Type::Tuple(vec![n]));
    assert_ne!(array, tuple, "a tuple is no array");
    assert_ne!(
        array,
        t.intern(Type::Array {
            element: n,
            mutated: true
        }),
        "a mutated array is another"
    );
    assert_ne!(tuple, t.intern(Type::Tuple(vec![n, n])));
    assert_ne!(
        t.intern(Type::Tuple(vec![n, s])),
        t.intern(Type::Tuple(vec![s, n]))
    );
    let or_undefined = t.union([n, TypeId::UNDEFINED]);
    let optional = t.intern(function(&[(n, true)], None, n));
    let required = t.intern(function(&[(or_undefined, false)], None, n));
    assert_ne!(
        optional, required,
        "an optional parameter is not a `T | undefined` one"
    );
    assert_ne!(optional, t.intern(function(&[(n, false)], None, n)));
    assert_ne!(
        optional,
        t.intern(function(&[(n, true)], Some(n), n)),
        "a rest parameter"
    );
    assert_ne!(optional, t.intern(function(&[(n, true)], None, s)));
    assert_eq!(optional, t.intern(function(&[(n, true)], None, n)));
    assert_ne!(
        t.intern(Type::Class(ClassId(0))),
        t.intern(Type::Class(ClassId(1))),
        "classes are nominal"
    );
    assert_ne!(
        t.intern(Type::Var(VarId(0))),
        t.intern(Type::Class(ClassId(0)))
    );
    assert_ne!(t.intern(Type::Promise(n)), t.intern(Type::Dictionary(n)));
    assert_ne!(
        t.intern(Type::Iterator {
            item: n,
            asynchronous: false
        }),
        t.intern(Type::Iterator {
            item: n,
            asynchronous: true
        })
    );
    assert_ne!(
        t.intern(Type::Builtin(Builtin::Map, vec![s, n])),
        t.intern(Type::Builtin(Builtin::WeakMap, vec![s, n]))
    );
    let before = t.count();
    assert_eq!(t.intern(Type::Promise(n)), t.intern(Type::Promise(n)));
    assert_eq!(t.count(), before, "interning again makes nothing");
    assert_eq!(
        t.get(shape),
        &Type::Object(vec![property("b", s, true), property("a", n, false)]),
        "the keys that retain a literal first"
    );
}

#[test]
fn the_keys_of_a_shape_and_the_two_booleans_have_their_order() {
    let mut t = Types::new();
    let n = TypeId::NUMBER;
    let keys = ["d", "a", "c", "b"].map(|key| property(key, n, key > "b"));
    let sorted = t.intern(Type::Object(keys.to_vec()));
    let order: Vec<String> = match t.get(sorted) {
        Type::Object(properties) => properties.iter().map(|p| p.key.clone()).collect(),
        _ => Vec::new(),
    };
    assert_eq!(order, ["c", "d", "a", "b"], "each group by key");
    let (yes, no) = (boolean(&mut t, true), boolean(&mut t, false));
    assert_eq!(t.compare(yes, no), Ordering::Less, "`true` before `false`");
    let a = string(&mut t, "a");
    assert_eq!(
        t.compare(a, yes),
        Ordering::Less,
        "after the other literals"
    );
    let of = |t: &mut Types, ty| {
        t.intern(Type::Object(vec![
            property("ok", ty, true),
            property("z", TypeId::NUMBER, false),
        ]))
    };
    let (ok, failed) = (of(&mut t, yes), of(&mut t, no));
    let other = t.intern(Type::Object(vec![
        property("ok", no, true),
        property("a", n, false),
    ]));
    let union = t.union([other, failed, ok]);
    assert_eq!(
        members(&t, union),
        [ok, other, failed],
        "shapes key by key: the type of a key before the next key"
    );
}

/// One type of every kind, in the canonical order.
fn in_order(t: &mut Types) -> Vec<TypeId> {
    let (n, s) = (TypeId::NUMBER, TypeId::STRING);
    let union = t.union([n, s]);
    let types = vec![
        Type::Number,
        Type::Int32,
        Type::BigInt,
        Type::String,
        Type::Boolean,
        Type::Literal(Literal::Number(Number::new(-1.5))),
        Type::Literal(Literal::Number(Number::new(-0.0))),
        Type::Literal(Literal::Number(Number::new(0.0))),
        Type::Literal(Literal::Number(Number::new(2.0))),
        Type::Literal(Literal::Number(Number::new(10.0))),
        Type::Literal(Literal::BigInt(Integer::parse("-10").expect("an integer"))),
        Type::Literal(Literal::BigInt(Integer::parse("-9").expect("an integer"))),
        Type::Literal(Literal::BigInt(Integer::parse("0").expect("an integer"))),
        Type::Literal(Literal::BigInt(Integer::parse("9").expect("an integer"))),
        Type::Literal(Literal::BigInt(Integer::parse("10").expect("an integer"))),
        Type::Literal(Literal::BigInt(Integer::parse("11").expect("an integer"))),
        Type::Literal(Literal::String(String::new())),
        Type::Literal(Literal::String("a".to_owned())),
        Type::Literal(Literal::String("b".to_owned())),
        Type::Literal(Literal::Boolean(false)),
        Type::Object(Vec::new()),
        Type::Object(vec![property("a", n, false)]),
        Type::Object(vec![property("a", s, false)]),
        Type::Object(vec![property("a", n, true)]),
        Type::Object(vec![property("b", n, false)]),
        Type::Object(vec![property("a", n, false), property("b", n, false)]),
        Type::Object(vec![property("a", n, false), property("b", s, false)]),
        Type::Dictionary(n),
        Type::Dictionary(s),
        Type::Array {
            element: n,
            mutated: false,
        },
        Type::Array {
            element: s,
            mutated: false,
        },
        Type::Array {
            element: union,
            mutated: false,
        },
        Type::Array {
            element: n,
            mutated: true,
        },
    ];
    let mut ids: Vec<TypeId> = types.into_iter().map(|ty| t.intern(ty)).collect();
    ids.extend(larger_in_order(t));
    ids
}

/// The rest of [`in_order`]: from the tuples on.
fn larger_in_order(t: &mut Types) -> Vec<TypeId> {
    let (n, s) = (TypeId::NUMBER, TypeId::STRING);
    let generic = |type_params: Vec<VarId>, params: Vec<Param>, rest, returns| {
        Type::Function(Function {
            type_params,
            params,
            rest,
            returns,
        })
    };
    let p = |ty, optional| Param { ty, optional };
    let types = vec![
        Type::Tuple(Vec::new()),
        Type::Tuple(vec![n]),
        Type::Tuple(vec![s]),
        Type::Tuple(vec![n, n]),
        Type::Tuple(vec![n, s]),
        generic(vec![], vec![], None, n),
        generic(vec![], vec![], None, s),
        generic(vec![], vec![], Some(n), n),
        generic(vec![], vec![p(n, false)], None, n),
        generic(vec![], vec![p(n, false)], None, s),
        generic(vec![], vec![p(s, false)], None, n),
        generic(vec![], vec![p(n, true)], None, n),
        generic(vec![], vec![p(n, false), p(n, false)], None, n),
        generic(vec![VarId(0)], vec![], None, n),
        generic(vec![VarId(1)], vec![], None, n),
        generic(vec![VarId(0), VarId(1)], vec![], None, n),
        Type::Class(ClassId(0)),
        Type::Class(ClassId(1)),
        Type::Var(VarId(0)),
        Type::Var(VarId(1)),
        Type::Promise(n),
        Type::Promise(s),
        Type::Iterator {
            item: n,
            asynchronous: false,
        },
        Type::Iterator {
            item: s,
            asynchronous: false,
        },
        Type::Iterator {
            item: n,
            asynchronous: true,
        },
        Type::Generator {
            yields: n,
            returns: n,
            asynchronous: false,
        },
        Type::Generator {
            yields: n,
            returns: s,
            asynchronous: false,
        },
        Type::Generator {
            yields: s,
            returns: n,
            asynchronous: false,
        },
        Type::Generator {
            yields: n,
            returns: n,
            asynchronous: true,
        },
        Type::Builtin(Builtin::Map, vec![n, n]),
        Type::Builtin(Builtin::Map, vec![n, s]),
        Type::Builtin(Builtin::Set, vec![n]),
        Type::Builtin(Builtin::Set, vec![n, n]),
        Type::Builtin(Builtin::WeakMap, vec![n, n]),
        Type::Null,
        Type::Undefined,
    ];
    types.into_iter().map(|ty| t.intern(ty)).collect()
}

#[test]
fn the_canonical_order_is_of_the_types_and_not_of_their_interning() {
    let mut t = Types::new();
    let ordered = in_order(&mut t);
    for (at, &a) in ordered.iter().enumerate() {
        for (other, &b) in ordered.iter().enumerate() {
            assert_eq!(
                t.compare(a, b),
                at.cmp(&other),
                "{:?} and {:?}",
                t.get(a),
                t.get(b)
            );
        }
    }
    // Interned in another order, a union of them has its members in the same order.
    let mut rng = Rng::from_seed(81);
    for _ in 0..20 {
        let mut u = Types::new();
        let mut scratch = Types::new();
        let mut shuffled: Vec<usize> = (0..ordered.len()).collect();
        for at in (1..shuffled.len()).rev() {
            shuffled.swap(at, rng.index(at + 1));
        }
        // What `in_order` interns first here is whatever the shuffle says.
        let all = in_order(&mut scratch);
        let mut ids = vec![TypeId::NEVER; all.len()];
        for &at in &shuffled {
            ids[at] = copy(&scratch, all[at], &mut u);
        }
        // Without the primitives, of which `number` would absorb the others.
        let kept: Vec<TypeId> = shuffled
            .iter()
            .filter(|&&at| at > 4)
            .map(|&at| ids[at])
            .collect();
        let union = u.union(kept);
        let expected: Vec<TypeId> = (5..ids.len()).map(|at| ids[at]).collect();
        assert_eq!(members(&u, union), expected);
    }
    let larger = t.union([TypeId::NUMBER, TypeId::STRING, TypeId::NULL]);
    let smaller = t.union([TypeId::NUMBER, TypeId::STRING]);
    let other = t.union([TypeId::NUMBER, TypeId::NULL]);
    let of = |t: &mut Types, element| {
        t.intern(Type::Array {
            element,
            mutated: false,
        })
    };
    let (larger, smaller, other) = (of(&mut t, larger), of(&mut t, smaller), of(&mut t, other));
    assert_eq!(
        t.compare(smaller, larger),
        Ordering::Less,
        "a union inside a type: by size"
    );
    assert_eq!(
        t.compare(smaller, other),
        Ordering::Less,
        "then by its members"
    );
    assert_eq!(t.compare(other, smaller), Ordering::Greater);
}

/// Interns in `to` the type `id` of `from`.
fn copy(from: &Types, id: TypeId, to: &mut Types) -> TypeId {
    let mut inner = |id: &TypeId| copy(from, *id, to);
    let ty = match from.get(id) {
        Type::Object(properties) => Type::Object(
            properties
                .iter()
                .map(|p| property(&p.key, inner(&p.ty), p.literal))
                .collect(),
        ),
        Type::Dictionary(value) => Type::Dictionary(inner(value)),
        Type::Array { element, mutated } => Type::Array {
            element: inner(element),
            mutated: *mutated,
        },
        Type::Tuple(elements) => Type::Tuple(elements.iter().map(inner).collect()),
        Type::Function(function) => Type::Function(Function {
            type_params: function.type_params.clone(),
            params: function
                .params
                .iter()
                .map(|p| Param {
                    ty: inner(&p.ty),
                    optional: p.optional,
                })
                .collect(),
            rest: function.rest.as_ref().map(&mut inner),
            returns: inner(&function.returns),
        }),
        Type::Promise(value) => Type::Promise(inner(value)),
        Type::Iterator { item, asynchronous } => Type::Iterator {
            item: inner(item),
            asynchronous: *asynchronous,
        },
        Type::Generator {
            yields,
            returns,
            asynchronous,
        } => Type::Generator {
            yields: inner(yields),
            returns: inner(returns),
            asynchronous: *asynchronous,
        },
        Type::Builtin(builtin, arguments) => {
            Type::Builtin(*builtin, arguments.iter().map(inner).collect())
        }
        Type::Union(members) => Type::Union(members.iter().map(inner).collect()),
        other => other.clone(),
    };
    to.intern(ty)
}

#[test]
fn numbers_and_integers_are_values() {
    let nan = Number::new(f64::NAN);
    assert_eq!(nan, Number::new(-f64::NAN), "every NaN is one");
    assert!(nan.value().is_nan());
    assert_eq!(Number::new(1.5).value().to_bits(), 1.5f64.to_bits());
    assert_ne!(Number::new(0.0), Number::new(-0.0));
    assert!(Number::new(-0.0) < Number::new(0.0) && Number::new(-1.0) < Number::new(-0.0));
    assert_eq!(
        Number::new(2.0).partial_cmp(&Number::new(10.0)),
        Some(Ordering::Less)
    );
    for int32 in [0.0, 1.0, -1.0, 1e3, 2_147_483_647.0, -2_147_483_648.0] {
        assert!(Number::new(int32).is_int32(), "{int32}");
        assert_eq!(
            Literal::Number(Number::new(int32)).primitive(),
            TypeId::INT32
        );
    }
    for other in [
        -0.0,
        0.5,
        2_147_483_648.0,
        -2_147_483_649.0,
        f64::NAN,
        f64::INFINITY,
        1e300,
    ] {
        assert!(!Number::new(other).is_int32(), "{other}");
        assert_eq!(
            Literal::Number(Number::new(other)).primitive(),
            TypeId::NUMBER
        );
    }
    assert_eq!(Literal::String(String::new()).primitive(), TypeId::STRING);
    assert_eq!(Literal::Boolean(false).primitive(), TypeId::BOOLEAN);
    let parse = |text: &str| Integer::parse(text).map(|integer| integer.decimal());
    assert_eq!(
        Literal::BigInt(Integer::parse("1").expect("1")).primitive(),
        TypeId::BIGINT
    );
    assert_eq!(
        parse("12345678901234567890123456789").as_deref(),
        Some("12345678901234567890123456789")
    );
    assert_eq!(parse("-42").as_deref(), Some("-42"));
    assert_eq!(parse("007").as_deref(), Some("7"));
    assert_eq!(
        parse("-000").as_deref(),
        Some("0"),
        "zero is never negative"
    );
    assert_eq!(parse("0").as_deref(), Some("0"));
    assert_eq!(Integer::parse("-0"), Integer::parse("0"));
    for text in ["", "-", "1n", "1.5", "+1", "--1", "1e3", " 1", "x"] {
        assert_eq!(Integer::parse(text), None, "{text:?}");
    }
    let order = ["-100", "-99", "-10", "-9", "0", "9", "10", "99", "100"];
    for (at, a) in order.iter().enumerate() {
        for (other, b) in order.iter().enumerate() {
            let (a, b) = (Integer::parse(a).expect("a"), Integer::parse(b).expect("b"));
            assert_eq!(a.cmp(&b), at.cmp(&other), "{a:?} and {b:?}");
            assert_eq!(a.partial_cmp(&b), Some(at.cmp(&other)));
        }
    }
}
