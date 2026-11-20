//! STORIES.md US-080: property tests over a generator of types. A type is generated as
//! a description, which an oracle written apart from the interner puts in normal form
//! as text; interning must agree with it.

mod common;

use common::{description, intern, Desc, BUILTINS, PRIMITIVES};
use sjs_testkit::prng::Rng;
use sjs_testkit::prop::{check, Gen};
use sjs_types::{Literal, Type, TypeId, Types};
use std::collections::{BTreeMap, BTreeSet};

/// The same type written another way: the members of its unions and the keys of its
/// shapes in another order, a member twice, a `never` among them.
fn rewritten(rng: &mut Rng, desc: &Desc) -> Desc {
    let again = |rng: &mut Rng, inner: &Desc| Box::new(rewritten(rng, inner));
    let all = |rng: &mut Rng, items: &[Desc]| -> Vec<Desc> {
        items.iter().map(|item| rewritten(rng, item)).collect()
    };
    match desc {
        Desc::Object(properties) => {
            let mut properties: Vec<_> = properties
                .iter()
                .map(|(key, value, literal)| (*key, rewritten(rng, value), *literal))
                .collect();
            // Only keys given once may move: of a key given twice the last stays.
            let once = |key: &str| properties.iter().filter(|p| p.0 == key).count() == 1;
            if properties.len() > 1 && properties.iter().all(|p| once(p.0)) {
                let at = rng.index(properties.len());
                properties.rotate_left(at);
            }
            Desc::Object(properties)
        }
        Desc::Union(members) => {
            let mut members = all(rng, members);
            if !members.is_empty() {
                let at = rng.index(members.len());
                members.rotate_left(at);
                members.push(members[rng.index(members.len())].clone());
            }
            members.push(Desc::Primitive(8));
            if rng.chance(1, 2) {
                let split = rng.index(members.len());
                let tail = members.split_off(split);
                members.push(Desc::Union(tail));
            }
            Desc::Union(members)
        }
        Desc::Dictionary(value) => Desc::Dictionary(again(rng, value)),
        Desc::Array(element, mutated) => Desc::Array(again(rng, element), *mutated),
        Desc::Tuple(elements) => Desc::Tuple(all(rng, elements)),
        Desc::Function(params, rest, returns) => Desc::Function(
            params
                .iter()
                .map(|(p, optional)| (rewritten(rng, p), *optional))
                .collect(),
            rest.as_ref().map(|rest| again(rng, rest)),
            again(rng, returns),
        ),
        Desc::Promise(value) => Desc::Promise(again(rng, value)),
        Desc::Iterator(item, asynchronous) => Desc::Iterator(again(rng, item), *asynchronous),
        Desc::Generator(yields, returns, asynchronous) => {
            Desc::Generator(again(rng, yields), again(rng, returns), *asynchronous)
        }
        Desc::Builtin(builtin, arguments) => Desc::Builtin(*builtin, all(rng, arguments)),
        // A type alone is the union of itself.
        other if rng.chance(1, 4) => Desc::Union(vec![other.clone()]),
        other => other.clone(),
    }
}

/// The oracle: the members of the normal form of a description, each as text. A type
/// that is no union has one; `never` has none.
fn normal_members(desc: &Desc) -> BTreeSet<String> {
    let one = |text: String| BTreeSet::from([text]);
    let list = |items: &[Desc]| items.iter().map(normal).collect::<Vec<_>>().join(", ");
    let members = match desc {
        Desc::Union(members) => members.iter().flat_map(normal_members).collect(),
        Desc::Primitive(at) => one(PRIMITIVES[*at].0.to_owned()),
        Desc::Number(value) => one(format!("{value:?}")),
        Desc::BigInt(text) => one(format!("{text}n")),
        Desc::Text(text) => one(format!("{text:?}")),
        Desc::Truth(value) => one(value.to_string()),
        Desc::Object(properties) => {
            let by_key: BTreeMap<&str, String> = properties
                .iter()
                .map(|(key, value, literal)| {
                    (
                        *key,
                        format!("{}{}", if *literal { "=" } else { ":" }, normal(value)),
                    )
                })
                .collect();
            let keys: Vec<String> = by_key
                .iter()
                .map(|(key, value)| format!("{key}{value}"))
                .collect();
            one(format!("{{{}}}", keys.join(", ")))
        }
        Desc::Dictionary(value) => one(format!("dict<{}>", normal(value))),
        Desc::Array(element, mutated) => one(format!(
            "{}[{}]",
            if *mutated { "mut " } else { "" },
            normal(element)
        )),
        Desc::Tuple(elements) => one(format!("tuple[{}]", list(elements))),
        Desc::Function(params, rest, returns) => {
            let params: Vec<String> = params
                .iter()
                .map(|(p, optional)| format!("{}{}", if *optional { "?" } else { "" }, normal(p)))
                .collect();
            let rest = rest
                .as_ref()
                .map_or(String::new(), |rest| format!(" ...{}", normal(rest)));
            one(format!(
                "fn({}{rest}) -> {}",
                params.join(", "),
                normal(returns)
            ))
        }
        Desc::Class(id) => one(format!("class{id}")),
        Desc::Var(id) => one(format!("var{id}")),
        Desc::Promise(value) => one(format!("promise<{}>", normal(value))),
        Desc::Iterator(item, asynchronous) => one(format!("iter{asynchronous}<{}>", normal(item))),
        Desc::Generator(yields, returns, asynchronous) => one(format!(
            "gen{asynchronous}<{}, {}>",
            normal(yields),
            normal(returns)
        )),
        Desc::Builtin(builtin, arguments) => {
            one(format!("{:?}<{}>", BUILTINS[*builtin], list(arguments)))
        }
    };
    let mut members: BTreeSet<String> = members;
    members.remove("never");
    let has = |members: &BTreeSet<String>, text: &str| members.contains(text);
    if has(&members, "unknown") {
        return one("unknown".to_owned());
    }
    if has(&members, "true") && has(&members, "false") {
        members.insert("boolean".to_owned());
    }
    // The literals the generator makes, by the primitive that absorbs each.
    let int32 = ["0.0", "1.0", "-1.0"];
    let number = ["-0.0", "1.5", "2147483648.0"];
    let absorbed = |member: &str| -> bool {
        let by = |primitive: &str| has(&members, primitive);
        match member {
            "int32" => by("number"),
            "true" | "false" => by("boolean"),
            _ if int32.contains(&member) => by("int32") || by("number"),
            _ if number.contains(&member) => by("number"),
            _ if member.starts_with('"') => by("string"),
            _ if member.ends_with('n')
                && member
                    .trim_start_matches('-')
                    .trim_end_matches('n')
                    .parse::<u32>()
                    .is_ok() =>
            {
                by("bigint")
            }
            _ => false,
        }
    };
    members
        .iter()
        .filter(|member| !absorbed(member))
        .cloned()
        .collect()
}

/// The oracle's normal form, as text.
fn normal(desc: &Desc) -> String {
    let members: Vec<String> = normal_members(desc).into_iter().collect();
    match members.as_slice() {
        [] => "never".to_owned(),
        [one] => one.clone(),
        several => format!("({})", several.join(" | ")),
    }
}

/// An interned type as the oracle writes one, with the members of a union in the
/// oracle's order when `sorted`, and else as they are stored.
fn written(types: &Types, id: TypeId, sorted: bool) -> String {
    let inner = |id: &TypeId| written(types, *id, sorted);
    let list = |ids: &[TypeId]| ids.iter().map(inner).collect::<Vec<_>>().join(", ");
    match types.get(id) {
        Type::Literal(Literal::Number(value)) => format!("{:?}", value.value()),
        Type::Literal(Literal::BigInt(value)) => format!("{}n", value.decimal()),
        Type::Literal(Literal::String(value)) => format!("{value:?}"),
        Type::Literal(Literal::Boolean(value)) => value.to_string(),
        Type::Object(properties) => {
            let mut keys: Vec<String> = properties
                .iter()
                .map(|p| {
                    format!(
                        "{}{}{}",
                        p.key,
                        if p.literal { "=" } else { ":" },
                        inner(&p.ty)
                    )
                })
                .collect();
            // In the oracle's order: a shape is a set of keys.
            keys.sort();
            format!("{{{}}}", keys.join(", "))
        }
        Type::Dictionary(value) => format!("dict<{}>", inner(value)),
        Type::Array { element, mutated } => {
            format!("{}[{}]", if *mutated { "mut " } else { "" }, inner(element))
        }
        Type::Tuple(elements) => format!("tuple[{}]", list(elements)),
        Type::Function(function) => {
            let params: Vec<String> = function
                .params
                .iter()
                .map(|p| format!("{}{}", if p.optional { "?" } else { "" }, inner(&p.ty)))
                .collect();
            let rest = function
                .rest
                .as_ref()
                .map_or(String::new(), |rest| format!(" ...{}", inner(rest)));
            format!(
                "fn({}{rest}) -> {}",
                params.join(", "),
                inner(&function.returns)
            )
        }
        Type::Class(id) => format!("class{}", id.0),
        Type::Var(id) => format!("var{}", id.0),
        Type::Promise(value) => format!("promise<{}>", inner(value)),
        Type::Iterator { item, asynchronous } => format!("iter{asynchronous}<{}>", inner(item)),
        Type::Generator {
            yields,
            returns,
            asynchronous,
        } => {
            format!("gen{asynchronous}<{}, {}>", inner(yields), inner(returns))
        }
        Type::Builtin(builtin, arguments) => format!("{builtin:?}<{}>", list(arguments)),
        Type::Union(members) => {
            let mut members: Vec<String> = members.iter().map(inner).collect();
            if sorted {
                members.sort();
            }
            format!("({})", members.join(" | "))
        }
        _ => {
            let primitive = PRIMITIVES.iter().find(|(_, primitive)| *primitive == id);
            primitive.expect("a primitive").0.to_owned()
        }
    }
}

/// Three descriptions: a type, the same type written another way, and another type.
struct Triples;

impl Gen for Triples {
    type Value = (Desc, Desc, Desc);

    fn generate(&self, rng: &mut Rng) -> Self::Value {
        let first = description(rng, 0);
        let same = rewritten(rng, &first);
        (first, same, description(rng, 0))
    }

    fn shrink(&self, _: &Self::Value) -> Vec<Self::Value> {
        Vec::new()
    }
}

#[test]
fn interning_gives_one_id_per_structurally_equal_type() {
    check(&Triples, |(first, same, other)| {
        let mut types = Types::new();
        let ids = [first, same, other].map(|desc| intern(&mut types, desc));
        let normals = [first, same, other].map(normal);
        assert_eq!(
            normals[0], normals[1],
            "the oracle: a rewriting is the same type"
        );
        assert_eq!(ids[0], ids[1], "one id however the type is written");
        assert_eq!(
            ids[0] == ids[2],
            normals[0] == normals[2],
            "an id per type, and no more"
        );
        for (id, normal) in ids.iter().zip(&normals) {
            assert_eq!(
                &written(&types, *id, true),
                normal,
                "the normal form is the oracle's"
            );
            assert_eq!(
                intern(&mut types, first),
                ids[0],
                "interning again changes nothing"
            );
            assert_eq!(types.compare(*id, *id), std::cmp::Ordering::Equal);
        }
        Ok(())
    });
}

#[test]
fn the_normal_form_does_not_depend_on_what_was_interned_before() {
    check(&Triples, |(first, same, other)| {
        let mut alone = Types::new();
        let mut after = Types::new();
        intern(&mut after, other);
        intern(&mut after, same);
        let (a, b) = (intern(&mut alone, first), intern(&mut after, first));
        assert_eq!(
            written(&alone, a, false),
            written(&after, b, false),
            "the members of a union are in one order, whatever came first"
        );
        let (x, y) = (intern(&mut alone, other), intern(&mut after, other));
        assert_eq!(
            alone.compare(a, x),
            after.compare(b, y),
            "and so is any pair of types"
        );
        assert_eq!(alone.compare(a, x), alone.compare(x, a).reverse());
        assert_eq!(alone.compare(a, x) == std::cmp::Ordering::Equal, a == x);
        Ok(())
    });
}

#[test]
fn the_union_laws_hold_for_generated_types() {
    check(&Triples, |(first, _, other)| {
        let mut types = Types::new();
        let (a, b) = (intern(&mut types, first), intern(&mut types, other));
        let both = types.union([a, b]);
        assert_eq!(types.union([b, a]), both, "norm(a | b) == norm(b | a)");
        assert_eq!(types.union([a, a]), a, "norm(T | T) == T");
        assert_eq!(types.union([TypeId::NEVER, a]), a, "norm(never | T) == T");
        assert_eq!(types.union([TypeId::UNKNOWN, a]), TypeId::UNKNOWN);
        assert_eq!(
            types.union([both, a]),
            both,
            "a union holds its members already"
        );
        assert_eq!(types.union([both, b]), both);
        let same = Desc::Union(vec![first.clone(), other.clone()]);
        assert_eq!(written(&types, both, true), normal(&same));
        Ok(())
    });
}
