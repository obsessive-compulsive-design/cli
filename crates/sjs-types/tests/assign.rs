//! STORIES.md US-082: assignability.

mod common;

use common::{description, intern, Desc};
use sjs_testkit::prng::Rng;
use sjs_testkit::prop::{check, Gen};
use sjs_types::{
    Assignability, Builtin, ClassId, Function, Integer, Literal, Number, Param, Printer, Property,
    Type, TypeId, Types, VarId,
};
use std::fmt::Write as _;

/// `Derived` (1) extends `Base` (0), `Leaf` (3) extends `Derived`; `Other` (2) and the
/// rest extend nothing.
fn superclass(class: ClassId) -> Option<ClassId> {
    match class.0 {
        1 => Some(ClassId(0)),
        3 => Some(ClassId(1)),
        _ => None,
    }
}

fn class_name(class: ClassId) -> String {
    ["Base", "Derived", "Other", "Leaf"]
        .get(class.0 as usize)
        .map_or(format!("Class{}", class.0), |name| (*name).to_owned())
}

fn property(key: &str, ty: TypeId, literal: bool) -> Property {
    Property {
        key: key.to_owned(),
        ty,
        literal,
    }
}

fn shape(t: &mut Types, keys: &[(&str, TypeId)]) -> TypeId {
    t.intern(Type::Object(
        keys.iter()
            .map(|&(key, ty)| property(key, ty, false))
            .collect(),
    ))
}

fn array(t: &mut Types, element: TypeId, mutated: bool) -> TypeId {
    t.intern(Type::Array { element, mutated })
}

fn function(
    t: &mut Types,
    params: &[(TypeId, bool)],
    rest: Option<TypeId>,
    returns: TypeId,
) -> TypeId {
    t.intern(Type::Function(Function {
        type_params: Vec::new(),
        params: params
            .iter()
            .map(|&(ty, optional)| Param { ty, optional })
            .collect(),
        rest,
        returns,
    }))
}

fn class(t: &mut Types, id: u32) -> TypeId {
    t.intern(Type::Class(ClassId(id)))
}

fn yes(t: &Types, from: TypeId, to: TypeId) -> bool {
    Assignability::new(t, &superclass).assignable(from, to)
}

const N: TypeId = TypeId::NUMBER;
const I: TypeId = TypeId::INT32;
const S: TypeId = TypeId::STRING;
const U: TypeId = TypeId::UNDEFINED;

/// The representative types of the matrix.
// A list of types, one to a row of the matrix: long, and nothing but the list.
#[allow(clippy::too_many_lines)]
fn representatives(t: &mut Types) -> Vec<TypeId> {
    let one = t.literal(Literal::Number(Number::new(1.0)));
    let half = t.literal(Literal::Number(Number::new(1.5)));
    let a = t.literal(Literal::String("a".to_owned()));
    let truth = t.literal(Literal::Boolean(true));
    let big = t.literal(Literal::BigInt(Integer::parse("1").expect("an integer")));
    let var = t.intern(Type::Var(VarId(0)));
    let generic = t.intern(Type::Function(Function {
        type_params: vec![VarId(0)],
        params: vec![Param {
            ty: var,
            optional: false,
        }],
        rest: None,
        returns: var,
    }));
    let retained = t.intern(Type::Object(vec![property("a", one, true)]));
    vec![
        N,
        I,
        TypeId::BIGINT,
        S,
        TypeId::BOOLEAN,
        U,
        TypeId::NULL,
        TypeId::UNKNOWN,
        TypeId::NEVER,
        one,
        half,
        a,
        truth,
        big,
        shape(t, &[]),
        shape(t, &[("a", N)]),
        shape(t, &[("a", I)]),
        shape(t, &[("a", N), ("b", S)]),
        retained,
        t.intern(Type::Dictionary(N)),
        t.intern(Type::Dictionary(I)),
        array(t, N, true),
        array(t, I, true),
        array(t, N, false),
        array(t, I, false),
        t.intern(Type::Tuple(vec![I, I])),
        t.intern(Type::Tuple(vec![N, S])),
        t.intern(Type::Tuple(vec![I])),
        function(t, &[], None, N),
        function(t, &[(N, false)], None, N),
        function(t, &[(I, false)], None, I),
        function(t, &[(N, true)], None, N),
        function(t, &[], Some(N), N),
        function(t, &[(N, false), (S, false)], None, N),
        generic,
        class(t, 0),
        class(t, 1),
        class(t, 2),
        var,
        t.intern(Type::Promise(N)),
        t.intern(Type::Promise(I)),
        t.intern(Type::Iterator {
            item: N,
            asynchronous: false,
        }),
        t.intern(Type::Iterator {
            item: N,
            asynchronous: true,
        }),
        t.intern(Type::Generator {
            yields: I,
            returns: S,
            asynchronous: false,
        }),
        t.intern(Type::Builtin(Builtin::Map, vec![S, N])),
        t.intern(Type::Builtin(Builtin::Set, vec![I])),
        t.union([S, U]),
        t.union([N, S]),
        t.union([one, a]),
    ]
}

#[test]
fn the_matrix_of_representative_types() {
    let mut t = Types::new();
    let types = representatives(&mut t);
    let printer = Printer::new(&t, &class_name);
    let mut out = String::from(
        "Each type, and the types of this list it is assignable to besides itself.\n\n",
    );
    for &from in &types {
        assert!(yes(&t, from, from), "reflexive");
        let to: Vec<String> = types
            .iter()
            .filter(|&&to| to != from && yes(&t, from, to))
            .map(|&to| printer.print(to))
            .collect();
        writeln!(out, "{}\n    => {}", printer.print(from), to.join("; ")).expect("a string");
    }
    sjs_testkit::snapshot!("assignability", &out);
}

#[test]
fn primitives_literals_and_unions() {
    let mut t = Types::new();
    let one = t.literal(Literal::Number(Number::new(1.0)));
    let half = t.literal(Literal::Number(Number::new(1.5)));
    let a = t.literal(Literal::String("a".to_owned()));
    let or_undefined = t.union([S, U]);
    let or_null = t.union([S, TypeId::NULL]);
    let wide = t.union([S, U, N]);
    assert!(yes(&t, I, N) && !yes(&t, N, I), "int32 <: number");
    assert!(
        yes(&t, one, I) && yes(&t, one, N) && !yes(&t, I, one),
        "literal <: primitive"
    );
    assert!(yes(&t, half, N) && !yes(&t, half, I) && !yes(&t, one, half));
    assert!(yes(&t, a, S) && !yes(&t, a, N) && !yes(&t, S, a));
    assert!(
        !yes(&t, U, TypeId::NULL) && !yes(&t, TypeId::NULL, U),
        "null and undefined are distinct"
    );
    assert!(!yes(&t, or_null, or_undefined) && !yes(&t, or_undefined, or_null));
    for ty in [N, a, or_undefined, U, TypeId::NEVER, TypeId::UNKNOWN] {
        assert!(yes(&t, ty, TypeId::UNKNOWN), "unknown accepts everything");
        assert!(yes(&t, TypeId::NEVER, ty), "never goes everywhere");
        assert_eq!(
            yes(&t, TypeId::UNKNOWN, ty),
            ty == TypeId::UNKNOWN,
            "unknown only to unknown"
        );
        assert_eq!(yes(&t, ty, TypeId::NEVER), ty == TypeId::NEVER);
    }
    assert!(
        yes(&t, S, or_undefined) && yes(&t, U, or_undefined) && yes(&t, a, or_undefined),
        "a union target accepts any member"
    );
    assert!(!yes(&t, N, or_undefined));
    assert!(
        yes(&t, or_undefined, wide),
        "a union source needs every member assignable"
    );
    assert!(!yes(&t, wide, or_undefined) && !yes(&t, or_undefined, S));
    let ints = t.union([I, a]);
    let numbers = t.union([N, S]);
    assert!(
        yes(&t, ints, numbers) && !yes(&t, numbers, ints),
        "member by member"
    );
}

#[test]
fn shapes_are_structural_and_classes_nominal() {
    let mut t = Types::new();
    let (narrow, wide) = (shape(&mut t, &[("a", I)]), shape(&mut t, &[("a", N)]));
    let more = shape(&mut t, &[("a", I), ("b", S)]);
    let empty = shape(&mut t, &[]);
    assert!(
        yes(&t, more, narrow) && yes(&t, more, wide) && yes(&t, more, empty),
        "width"
    );
    assert!(!yes(&t, narrow, more), "a key it lacks");
    assert!(
        yes(&t, narrow, wide) && !yes(&t, wide, narrow),
        "each key by its type"
    );
    let one = t.literal(Literal::Number(Number::new(1.0)));
    let retained = t.intern(Type::Object(vec![property("a", one, true)]));
    assert!(
        yes(&t, retained, narrow) && !yes(&t, narrow, retained),
        "a retained literal is its type"
    );
    assert!(
        !yes(&t, narrow, N) && !yes(&t, N, empty),
        "a shape is no primitive"
    );
    let (base, derived, other, leaf) = (
        class(&mut t, 0),
        class(&mut t, 1),
        class(&mut t, 2),
        class(&mut t, 3),
    );
    assert!(
        yes(&t, derived, base) && yes(&t, leaf, base) && yes(&t, leaf, derived),
        "up the classes it extends"
    );
    assert!(
        !yes(&t, base, derived) && !yes(&t, other, base) && !yes(&t, base, other),
        "and nowhere else"
    );
    assert!(
        !yes(&t, base, empty) && !yes(&t, empty, base),
        "a class is no shape"
    );
    let looping = |class: ClassId| Some(ClassId(1 - class.0));
    let judge = Assignability::new(&t, &looping);
    assert!(
        judge.assignable(base, derived) && !judge.assignable(base, other),
        "classes that extend one another end"
    );
    let (dict, wider) = (t.intern(Type::Dictionary(I)), t.intern(Type::Dictionary(N)));
    assert!(
        yes(&t, dict, wider)
            && !yes(&t, wider, dict)
            && !yes(&t, narrow, wider)
            && !yes(&t, wider, empty)
    );
    let (var, another) = (t.intern(Type::Var(VarId(0))), t.intern(Type::Var(VarId(1))));
    assert!(
        !yes(&t, var, another) && !yes(&t, var, N) && !yes(&t, N, var),
        "a type variable is itself only"
    );
}

#[test]
fn arrays_and_tuples() {
    let mut t = Types::new();
    let (ints, numbers) = (array(&mut t, I, false), array(&mut t, N, false));
    let (mut_ints, mut_numbers) = (array(&mut t, I, true), array(&mut t, N, true));
    assert!(
        yes(&t, ints, numbers) && !yes(&t, numbers, ints),
        "never mutated: covariant"
    );
    assert!(
        !yes(&t, mut_ints, mut_numbers) && !yes(&t, mut_numbers, mut_ints),
        "mutated: invariant"
    );
    assert!(
        !yes(&t, mut_ints, numbers) && !yes(&t, mut_ints, ints),
        "mutated anywhere is mutated: only to itself"
    );
    assert!(!yes(&t, ints, mut_ints) && !yes(&t, ints, mut_numbers));
    let pair = t.intern(Type::Tuple(vec![I, S]));
    let wider = t.intern(Type::Tuple(vec![N, S]));
    let single = t.intern(Type::Tuple(vec![I]));
    let swapped = t.intern(Type::Tuple(vec![S, I]));
    assert!(
        yes(&t, pair, wider) && !yes(&t, wider, pair),
        "element by element"
    );
    assert!(
        !yes(&t, pair, single) && !yes(&t, single, pair) && !yes(&t, pair, swapped),
        "of the same length"
    );
    let either = t.union([I, S]);
    let (of_either, mut_either) = (array(&mut t, either, false), array(&mut t, either, true));
    assert!(yes(&t, pair, of_either), "to an array of its element union");
    assert!(
        !yes(&t, pair, mut_either),
        "only when the array is unmutated"
    );
    assert!(!yes(&t, pair, ints) && yes(&t, single, ints) && yes(&t, single, numbers));
    assert!(
        !yes(&t, of_either, pair) && !yes(&t, ints, single),
        "an array is no tuple"
    );
    let empty = t.intern(Type::Tuple(Vec::new()));
    assert!(yes(&t, empty, ints) && !yes(&t, empty, mut_ints) && !yes(&t, empty, single));
}

#[test]
fn functions() {
    let mut t = Types::new();
    let f = |t: &mut Types, params: &[(TypeId, bool)], rest, returns| {
        function(t, params, rest, returns)
    };
    let takes_number = f(&mut t, &[(N, false)], None, N);
    let takes_int = f(&mut t, &[(I, false)], None, N);
    assert!(
        yes(&t, takes_number, takes_int) && !yes(&t, takes_int, takes_number),
        "parameters are contravariant"
    );
    let returns_int = f(&mut t, &[(N, false)], None, I);
    assert!(
        yes(&t, returns_int, takes_number) && !yes(&t, takes_number, returns_int),
        "the return covariant"
    );
    let none = f(&mut t, &[], None, N);
    let two = f(&mut t, &[(N, false), (S, false)], None, N);
    assert!(
        yes(&t, none, takes_number) && yes(&t, takes_number, two),
        "fewer parameters do for more"
    );
    assert!(
        !yes(&t, takes_number, none) && !yes(&t, two, takes_number),
        "more required do not for fewer"
    );
    let optional = f(&mut t, &[(N, true)], None, N);
    assert!(
        yes(&t, optional, takes_number),
        "one that may be left out takes one that is passed"
    );
    assert!(
        !yes(&t, optional, none),
        "but is not hidden: `(a?: T)` is no `()`"
    );
    assert!(
        !yes(&t, takes_number, optional),
        "must be optional where the caller may leave it out"
    );
    let or_undefined = t.union([N, U]);
    let takes_undefined = f(&mut t, &[(or_undefined, false)], None, N);
    assert!(
        !yes(&t, takes_undefined, optional) && !yes(&t, optional, takes_undefined),
        "`T | undefined` is not optional"
    );
    assert!(yes(&t, takes_undefined, takes_number));
    let rest_numbers = f(&mut t, &[], Some(N), N);
    let rest_ints = f(&mut t, &[], Some(I), N);
    let rest_strings = f(&mut t, &[], Some(S), N);
    assert!(
        yes(&t, rest_numbers, rest_ints) && !yes(&t, rest_ints, rest_numbers),
        "the rest is contravariant"
    );
    assert!(
        !yes(&t, rest_numbers, takes_number)
            && !yes(&t, rest_numbers, none)
            && !yes(&t, rest_numbers, optional),
        "a rest goes only where rest arguments are passed"
    );
    assert!(
        !yes(&t, rest_strings, takes_number) && !yes(&t, rest_numbers, two),
        "the rest takes each argument"
    );
    assert!(
        yes(&t, none, rest_numbers),
        "a function that ignores its arguments"
    );
    assert!(
        yes(&t, optional, rest_numbers) && !yes(&t, takes_number, rest_numbers),
        "past the named ones, optional"
    );
    let optional_string = f(&mut t, &[(S, true)], None, N);
    assert!(
        !yes(&t, optional_string, rest_numbers),
        "and taking what the rest passes"
    );
    let named_then_rest = f(&mut t, &[(N, false)], Some(S), N);
    let number_then_optional = f(&mut t, &[(N, false), (S, true)], None, N);
    assert!(yes(&t, number_then_optional, named_then_rest) && !yes(&t, named_then_rest, two));
    let rest_after = f(&mut t, &[(I, false)], Some(I), N);
    assert!(yes(&t, rest_numbers, rest_after) && !yes(&t, rest_after, rest_numbers));
    assert!(!yes(&t, named_then_rest, rest_numbers));
}

#[test]
fn generic_functions_and_methods() {
    let mut t = Types::new();
    let takes_number = function(&mut t, &[(N, false)], None, N);
    let takes_int = function(&mut t, &[(I, false)], None, N);
    let var = t.intern(Type::Var(VarId(0)));
    let generic = |t: &mut Types, vars: Vec<VarId>| {
        t.intern(Type::Function(Function {
            type_params: vars,
            params: vec![Param {
                ty: var,
                optional: false,
            }],
            rest: None,
            returns: N,
        }))
    };
    let (over_t, over_u, plain) = (
        generic(&mut t, vec![VarId(0)]),
        generic(&mut t, vec![VarId(1)]),
        generic(&mut t, Vec::new()),
    );
    assert!(
        !yes(&t, over_t, over_u) && !yes(&t, over_t, plain) && !yes(&t, plain, over_t),
        "generic over the same variables"
    );
    // A method's parameter is contravariant like any other.
    let (with_wide, with_narrow) = (
        shape(&mut t, &[("m", takes_number)]),
        shape(&mut t, &[("m", takes_int)]),
    );
    assert!(yes(&t, with_wide, with_narrow) && !yes(&t, with_narrow, with_wide));
    assert!(!yes(&t, takes_number, N) && !yes(&t, N, takes_number));
}

#[test]
fn what_holds_a_type() {
    let mut t = Types::new();
    let (p_int, p_number) = (t.intern(Type::Promise(I)), t.intern(Type::Promise(N)));
    assert!(yes(&t, p_int, p_number) && !yes(&t, p_number, p_int));
    let iterator =
        |t: &mut Types, item, asynchronous| t.intern(Type::Iterator { item, asynchronous });
    let generator = |t: &mut Types, yields, returns, asynchronous| {
        t.intern(Type::Generator {
            yields,
            returns,
            asynchronous,
        })
    };
    let (ints, numbers, later) = (
        iterator(&mut t, I, false),
        iterator(&mut t, N, false),
        iterator(&mut t, I, true),
    );
    assert!(yes(&t, ints, numbers) && !yes(&t, numbers, ints));
    assert!(
        !yes(&t, ints, later) && !yes(&t, later, ints),
        "async or not"
    );
    let gen = generator(&mut t, I, S, false);
    let wider = generator(&mut t, N, S, false);
    let other_return = generator(&mut t, I, N, false);
    let async_gen = generator(&mut t, I, S, true);
    assert!(yes(&t, gen, wider) && !yes(&t, wider, gen) && !yes(&t, gen, other_return));
    assert!(!yes(&t, gen, async_gen) && !yes(&t, async_gen, gen));
    let returns_int = generator(&mut t, I, I, false);
    assert!(
        yes(&t, returns_int, other_return) && !yes(&t, other_return, returns_int),
        "what it returns too"
    );
    assert!(
        yes(&t, gen, ints) && yes(&t, gen, numbers) && !yes(&t, wider, ints),
        "a generator is an iterator of what it yields"
    );
    assert!(!yes(&t, gen, later) && yes(&t, async_gen, later) && !yes(&t, ints, gen));
    let map = |t: &mut Types, value| t.intern(Type::Builtin(Builtin::Map, vec![S, value]));
    let (map_int, map_number) = (map(&mut t, I), map(&mut t, N));
    assert!(
        !yes(&t, map_int, map_number) && !yes(&t, map_number, map_int),
        "a collection is mutable: itself only"
    );
    assert!(!yes(&t, p_int, ints) && !yes(&t, ints, p_int));
}

/// Three types to intern together.
struct Triples;

impl Gen for Triples {
    type Value = Vec<Desc>;

    fn generate(&self, rng: &mut Rng) -> Self::Value {
        (0..3).map(|_| description(rng, 0)).collect()
    }

    fn shrink(&self, _: &Self::Value) -> Vec<Self::Value> {
        Vec::new()
    }
}

#[test]
fn assignability_is_reflexive_and_transitive() {
    check(&Triples, |descriptions| {
        let mut t = Types::new();
        let mut types: Vec<TypeId> = descriptions.iter().map(|d| intern(&mut t, d)).collect();
        // With what the three make together, so that many pairs are related.
        let (a, b, c) = (types[0], types[1], types[2]);
        types.extend([t.union([a, b]), t.union([b, c]), t.union([a, b, c])]);
        for at in 0..6 {
            let element = types[at];
            types.push(array(&mut t, element, false));
            types.push(t.intern(Type::Tuple(vec![element])));
            types.push(shape(&mut t, &[("k", element)]));
            types.push(function(&mut t, &[(element, false)], None, element));
            types.push(function(&mut t, &[], None, element));
            types.push(t.intern(Type::Promise(element)));
        }
        let judge = Assignability::new(&t, &superclass);
        let printer = Printer::new(&t, &class_name);
        for &x in &types {
            assert!(judge.assignable(x, x), "reflexive");
            assert!(judge.assignable(x, TypeId::UNKNOWN) && judge.assignable(TypeId::NEVER, x));
        }
        assert!(
            judge.assignable(a, types[3]) && judge.assignable(b, types[3]),
            "a member to its union"
        );
        for &x in &types {
            for &y in types.iter().filter(|&&y| judge.assignable(x, y)) {
                for &z in types.iter().filter(|&&z| judge.assignable(y, z)) {
                    if !judge.assignable(x, z) {
                        return Err(format!(
                            "`{}` goes to `{}`, which goes to `{}`, but the first does not go to the last",
                            printer.print(x),
                            printer.print(y),
                            printer.print(z)
                        ));
                    }
                }
            }
        }
        Ok(())
    });
}
