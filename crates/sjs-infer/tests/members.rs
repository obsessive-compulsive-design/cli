//! STORIES.md US-086: property reads and writes.

use sjs_bind::bind;
use sjs_diag::SourceFile;
use sjs_infer::{check_expressions, infer_bindings, Facts};
use sjs_syntax::parse;
use sjs_types::{ClassId, Literal, Printer, Property, Type, TypeId, Types};
use std::collections::{BTreeMap, BTreeSet};

fn class_name(class: ClassId) -> String {
    format!("Class{}", class.0)
}

fn shape(types: &mut Types, keys: &[(&str, TypeId)]) -> TypeId {
    let properties = keys.iter().map(|&(key, ty)| Property {
        key: key.to_owned(),
        ty,
        literal: false,
    });
    types.intern(Type::Object(properties.collect()))
}

/// The names every test may use, each declared `const name = f()` and given a type:
/// `Point` (class 0: fields `x`, `y`; method `norm`), `Point3` (class 1, extends 0:
/// field `z`; method `flat`), and an instance of a class nothing is known of.
fn known(types: &mut Types) -> Vec<(&'static str, TypeId)> {
    let (num, text, absent) = (TypeId::NUMBER, TypeId::STRING, TypeId::UNDEFINED);
    let wide = shape(types, &[("k", num), ("only", text)]);
    let narrow = shape(types, &[("k", text)]);
    let inner = shape(types, &[("c", TypeId::INT32)]);
    let inner_or = types.union([inner, absent]);
    let outer = shape(types, &[("b", inner_or)]);
    let literal = |types: &mut Types, key: &str| types.literal(Literal::String(key.to_owned()));
    let (key_k, key_only, key_z) = (
        literal(types, "k"),
        literal(types, "only"),
        literal(types, "z"),
    );
    vec![
        ("one", wide),
        ("either", types.union([wide, narrow])),
        ("maybe", types.union([wide, absent])),
        ("nullable", types.union([wide, TypeId::NULL])),
        ("deep", types.union([outer, absent])),
        ("outer", outer),
        ("point", types.intern(Type::Class(ClassId(0), Vec::new()))),
        ("point3", types.intern(Type::Class(ClassId(1), Vec::new()))),
        (
            "stranger",
            types.intern(Type::Class(ClassId(2), Vec::new())),
        ),
        (
            "list",
            types.intern(Type::Array {
                element: num,
                mutated: true,
            }),
        ),
        ("pair", types.intern(Type::Tuple(vec![num, text]))),
        ("text", text),
        ("dict", types.intern(Type::Dictionary(num))),
        ("raw", TypeId::UNKNOWN),
        ("num", num),
        ("promise", types.intern(Type::Promise(num))),
        ("kk", key_k),
        ("k_or_only", types.union([key_k, key_only])),
        ("k_or_z", types.union([key_k, key_z])),
        ("anykey", text),
    ]
}

/// The type inferred for `x` (`?` for none) and the findings: `CODE @line:column text`.
fn checked(body: &str) -> (String, Vec<String>, Vec<String>) {
    let mut types = Types::new();
    let names = known(&mut types);
    let head: String = names
        .iter()
        .map(|(name, _)| format!("const {name} = f()\n"))
        .collect::<Vec<_>>()
        .concat();
    let lines = u32::try_from(names.len()).expect("few names");
    let text = format!("{head}{body}");
    let source = SourceFile::new("a.js", text.as_bytes().to_vec()).expect("utf-8");
    let (tree, _) = parse(&source);
    let (bindings, _) = bind(&source, &tree);
    let mut facts = Facts::default();
    for (name, ty) in names {
        facts
            .declarations
            .insert(bindings.lookup(0, name).expect("declared"), ty);
    }
    let fields = |pairs: &[(&str, TypeId)]| -> BTreeMap<String, TypeId> {
        pairs
            .iter()
            .map(|(key, ty)| ((*key).to_owned(), *ty))
            .collect()
    };
    facts.fields.insert(
        ClassId(0),
        fields(&[("x", TypeId::NUMBER), ("y", TypeId::NUMBER)]).into(),
    );
    facts
        .fields
        .insert(ClassId(1), fields(&[("z", TypeId::NUMBER)]).into());
    facts
        .methods
        .insert(ClassId(0), BTreeSet::from(["norm".to_owned()]).into());
    facts
        .methods
        .insert(ClassId(1), BTreeSet::from(["flat".to_owned()]).into());
    facts.superclass.insert(ClassId(1), ClassId(0));
    facts.class_names.insert(ClassId(0), "Point".to_owned());
    let facts = infer_bindings(&source, &tree, &bindings, &facts, &mut types);
    let diagnostics = check_expressions(&source, &tree, &bindings, &facts, &mut types);
    let found = diagnostics
        .iter()
        .map(|d| {
            let covered = source.slice(&d.span).expect("in the file");
            // Lines are counted from the body's first.
            format!(
                "{} @{}:{} {covered}",
                d.code.as_str(),
                d.span.start.line - lines,
                d.span.start.column
            )
        })
        .collect();
    let messages = diagnostics.iter().map(|d| d.message.clone()).collect();
    let x = bindings.declarations.iter().position(|d| d.name == "x");
    let ty = x.and_then(|at| facts.declarations.get(&at));
    let printed = ty.map_or("?".to_owned(), |&ty| {
        Printer::new(&types, &class_name).print(ty)
    });
    (printed, found, messages)
}

/// The type of an expression that is read without a finding.
fn typed(expression: &str) -> String {
    let (printed, found, _) = checked(&format!("const x = {expression}\n"));
    assert_eq!(found, Vec::<String>::new(), "{expression}");
    printed
}

fn found(body: &str) -> Vec<String> {
    checked(body).1
}

const NONE: [&str; 0] = [];

#[test]
fn a_read_gives_what_every_member_holds() {
    assert_eq!(typed("one.k"), "number");
    assert_eq!(
        typed("either.k"),
        "number | string",
        "on a union, the union"
    );
    assert_eq!(typed("outer.b"), "{ c: int32 } | undefined");
    assert_eq!(
        typed("{ a: { b: 1 } }.a.b"),
        "1",
        "from a literal, its literal"
    );
    assert_eq!(typed("point.x"), "number", "a field of a class");
    assert_eq!(
        typed("point3.z + point3.x"),
        "number",
        "and of the class it extends"
    );
    assert_eq!(typed("list.length"), "int32");
    assert_eq!(typed("pair.length"), "int32");
    assert_eq!(typed("text.length"), "int32");
    assert_eq!(typed("\"abc\".length"), "int32");
    assert_eq!(typed("dict.anything"), "number | undefined");
    for later in ["stranger.field", "unknownName.k", "f().k"] {
        assert_eq!(typed(later), "?", "{later} is a later pass's");
    }
}

#[test]
fn a_key_some_member_lacks_is_e137() {
    assert_eq!(found("one.missing\n"), ["E137 @1:5 missing"]);
    assert_eq!(
        found("either.only\n"),
        ["E137 @1:8 only"],
        "on one member only"
    );
    assert_eq!(
        found("maybe.k\nnullable.k\n"),
        ["E137 @1:7 k", "E137 @2:10 k"],
        "`undefined` and `null` have no keys"
    );
    assert_eq!(
        found("raw.k\n"),
        ["E137 @1:5 k"],
        "nor has `unknown`, until it is narrowed"
    );
    assert_eq!(
        found("point.w\npoint3.w\npoint.z\n"),
        ["E137 @1:7 w", "E137 @2:8 w", "E137 @3:7 z"]
    );
    assert_eq!(
        found("one.missing.deeper\nconst x = [one.missing, either.k]\n"),
        ["E137 @1:5 missing", "E137 @2:16 missing"],
        "once, at the key"
    );
    assert_eq!(checked("const x = one.missing\n").0, "?");
    assert_eq!(
        found("stranger.w\n"),
        NONE,
        "what a later pass gives members is not judged"
    );
    let messages = checked("one.missing\nmaybe.k\npoint.w\n").2;
    assert_eq!(
        messages,
        [
            "`missing` is not a key of `{ k: number, only: string }`: every value this may be must have it (SPEC.md §3.2)",
            "`k` is not a key of `undefined`: every value this may be must have it (SPEC.md §3.2)",
            "`w` is not a key of `Point`: every value this may be must have it (SPEC.md §3.2)",
        ]
    );
}

#[test]
fn an_optional_read_is_for_a_receiver_that_may_be_nullish() {
    assert_eq!(typed("maybe?.k"), "number | undefined");
    assert_eq!(typed("nullable?.k"), "number | undefined");
    assert_eq!(typed("deep?.b"), "{ c: int32 } | undefined");
    assert_eq!(typed("deep?.b?.c"), "int32 | undefined");
    assert_eq!(typed("maybe?.k.toFixed()"), "string | undefined");
    assert_eq!(typed("maybe?.[\"k\"]"), "number | undefined");
    assert_eq!(
        typed("maybe?.only.length"),
        "int32 | undefined",
        "`a?.b.c`: the chain ends where `a` is absent"
    );
    assert_eq!(typed("(maybe?.only)"), "string | undefined");
    assert_eq!(checked("const x = (maybe?.only).length\n"), ("?".to_owned(), vec!["E137 @1:25 length".to_owned()], vec!["`length` is not a key of `undefined`: every value this may be must have it (SPEC.md §3.2)".to_owned()]), "in parentheses the chain has ended");
    assert_eq!(
        found("deep?.b.c\n"),
        ["E137 @1:9 c"],
        "`b` itself may be absent"
    );
    assert_eq!(
        found("one?.k\npoint?.x\nlist?.[0]\n"),
        ["E121 @1:4 ?.", "E121 @2:6 ?.", "E121 @3:5 ?."]
    );
    assert_eq!(
        checked("const x = one?.k\n").0,
        "number | undefined".replace(" | undefined", ""),
        "typed as if it were `.`"
    );
    assert_eq!(found("maybe?.missing\n"), ["E137 @1:8 missing"]);
    assert_eq!(
        found("stranger?.k\nunknownName?.k\n"),
        ["E121 @1:9 ?."],
        "a class is never nullish; what has no type yet is not judged"
    );
    assert_eq!(
        checked("one?.k\n").2,
        ["`?.` on a `{ k: number, only: string }`, which is never `undefined` or `null`: write `.` (SPEC.md §8.1)"]
    );
}

#[test]
fn a_computed_read_takes_a_literal_of_the_keys() {
    assert_eq!(typed("one[\"k\"]"), "number");
    assert_eq!(typed("one[kk]"), "number");
    assert_eq!(
        typed("one[k_or_only]"),
        "number | string",
        "a union of literals: the union of those keys"
    );
    assert_eq!(typed("either[kk]"), "number | string");
    assert_eq!(typed("point[\"x\"]"), "number");
    assert_eq!(
        typed("{ 1: \"a\", true: \"b\" }[1]"),
        "\"a\"",
        "a key as ES spells it"
    );
    assert_eq!(typed("{ 1: \"a\", true: \"b\" }[1n]"), "\"a\"");
    assert_eq!(typed("{ 1: \"a\", true: \"b\" }[true]"), "\"b\"");
    assert_eq!(
        typed("list[0]"),
        "number | undefined",
        "an element may be absent"
    );
    assert_eq!(typed("list[num]"), "number | undefined");
    assert_eq!(typed("pair[0]"), "number", "a literal place of a tuple");
    assert_eq!(typed("pair[1]"), "string");
    assert_eq!(
        typed("pair[2]"),
        "number | string | undefined",
        "out of range"
    );
    assert_eq!(typed("pair[num]"), "number | string | undefined");
    assert_eq!(typed("pair[1.5]"), "number | string | undefined");
    assert_eq!(typed("text[0]"), "string | undefined");
    assert_eq!(typed("dict[anykey]"), "number | undefined");
    assert_eq!(typed("dict[\"a\"]"), "number | undefined");
    for later in [
        "one[later()]",
        "num[0]",
        "stranger[\"k\"]",
        "promise[kk]",
        "point[\"norm\"]",
    ] {
        assert_eq!(typed(later), "?", "{later}");
    }
    assert_eq!(found("one[k_or_z]\n"), ["E137 @1:5 k_or_z"]);
    assert_eq!(
        found("one[anykey]\none[num]\none[0]\neither[\"only\"]\n"),
        [
            "E137 @1:5 anykey",
            "E137 @2:5 num",
            "E137 @3:5 0",
            "E137 @4:8 \"only\""
        ]
    );
    assert_eq!(
        found("list[\"a\"]\npair[kk]\ntext[anykey]\ndict[0]\nraw[kk]\nmaybe[kk]\nnullable[0]\n")
            .len(),
        7
    );
    assert_eq!(
        found("point[\"w\"]\npoint[anykey]\n"),
        ["E137 @1:7 \"w\"", "E137 @2:7 anykey"]
    );
    assert_eq!(
        checked("one[k_or_z]\none[anykey]\nlist[kk]\n").2,
        [
            "`z` is not a key of `{ k: number, only: string }`: every key this may be must be one (SPEC.md §4.6)",
            "a `{ k: number, only: string }` is not read by a `string`: a computed read takes a literal of the keys, or a number for an array (SPEC.md §4.6)",
            "a `number[]` is not read by a `\"k\"`: a computed read takes a literal of the keys, or a number for an array (SPEC.md §4.6)",
        ]
    );
}

#[test]
fn the_keys_and_entries_of_a_shape_are_its_literal_keys() {
    assert_eq!(typed("Object.keys(one)"), "readonly (\"k\" | \"only\")[]");
    assert_eq!(
        typed("Object.entries(one)"),
        "readonly ([\"k\", number] | [\"only\", string])[]"
    );
    assert_eq!(typed("Object.keys({})"), "readonly never[]");
    assert_eq!(typed("Object.keys({ a: 1 }).length"), "int32");
    for later in [
        "Object.keys(either)",
        "Object.keys(list)",
        "Object.keys()",
        "Object.keys(later())",
    ] {
        assert_eq!(typed(later), "?", "{later}");
    }
    assert_eq!(
        checked("const Object = f()\nconst x = Object.keys(one)\n").0,
        "?",
        "an `Object` the file declares"
    );
}

#[test]
fn a_write_needs_the_key_and_a_value_it_takes() {
    assert_eq!(found("one.k = 2\none.only = \"s\"\none[\"k\"] = 1.5\none[kk] = 0\npoint.x = 1\npoint3.x = point.y\n"), NONE);
    assert_eq!(
        found("one.fresh = 1\npoint.w = 1\none[\"fresh\"] = 1\n"),
        ["E129 @1:5 fresh", "E129 @2:7 w", "E129 @3:5 \"fresh\""],
        "a key is not added"
    );
    assert_eq!(
        found("either.only = \"s\"\none[k_or_z] = 1\n"),
        ["E129 @1:8 only", "E129 @2:5 k_or_z"],
        "through a union, on every member"
    );
    assert_eq!(
        found("one.k = \"s\"\none.only = 1\npoint.x = text\neither.k = 1\n"),
        [
            "E138 @1:9 \"s\"",
            "E138 @2:12 1",
            "E138 @3:11 text",
            "E138 @4:12 1"
        ],
        "a value the key does not take, on any member"
    );
    assert_eq!(found("one.fresh += 1\none.fresh++\n--one.fresh\nfor (const i of list) {\n  one.fresh ??= 1\n}\n"), ["E129 @1:5 fresh", "E129 @2:5 fresh", "E129 @3:7 fresh", "E129 @5:7 fresh"]);
    assert_eq!(
        found("one.k += \"s\"\none.k++\none.only ??= 1\n"),
        NONE,
        "only `=` is judged for its value here"
    );
    assert_eq!(found("const o = { r: 1, deep: { n: 0 } }\no.r = 2\no.r = 1.5\no.deep.n = 2.5\no.deep = { n: 1 }\n"), NONE, "a key that held an int32 takes any number");
    assert_eq!(
        found("const o = { r: 1 }\no.r = \"s\"\no.deep.n = 1\n"),
        ["E138 @2:7 \"s\"", "E137 @3:3 deep"],
        "what is read on the way to the key is a read"
    );
    assert_eq!(
        found("maybe.k = 1\nraw.k = 1\nmaybe?.k\n"),
        ["E129 @1:7 k", "E129 @2:5 k"]
    );
    assert_eq!(
        found("one?.k\n;[one.k, one.missing] = [1, 2]\n"),
        ["E121 @1:4 ?.", "E137 @2:14 missing"]
    );
    assert_eq!(found("stranger.k = 1\nlist[0] = 1\none.k = later()\none[later()] = 1\npoint.norm = 1\nunknownName.k = 1\none[anykey] = 1\n"), NONE, "what is not known yet is not judged");
    assert_eq!(
        found("point.#p = 1\nconst s = point.#secret\n"),
        ["E129 @1:7 #p", "E137 @2:17 #secret"],
        "a private name is a key like another"
    );
    assert_eq!(
        checked("one.fresh = 1\none.k = \"s\"\neither[kk] = true\n").2,
        [
            "`fresh` is not a key of `{ k: number, only: string }`: a key is not added after the object is made (SPEC.md §3.2)",
            "a `\"s\"` is assigned to `k`, which holds a `number` (SPEC.md §3.2)",
            "a `true` is assigned to `k`, which holds a `string` (SPEC.md §3.2)",
        ]
    );
    assert_eq!(
        found("point3.x = point3\nconst q = { p: point }\nq.p = point3\nq.p = one\n"),
        ["E138 @1:12 point3", "E138 @4:7 one"],
        "a class goes where a class it extends is held"
    );
}

#[test]
fn a_method_is_called_not_read() {
    assert_eq!(
        found("point.norm()\npoint3.norm()\npoint3.flat()\npoint?.norm\n"),
        ["E121 @4:6 ?.", "E138 @4:8 norm"]
    );
    assert_eq!(
        found("const m = point.norm\nf(point3.flat)\npoint.norm.name\n(point.norm)()\n"),
        [
            "E138 @1:17 norm",
            "E138 @2:10 flat",
            "E138 @3:7 norm",
            "E138 @4:8 norm"
        ]
    );
    assert_eq!(
        checked("const x = point.norm()\n").0,
        "?",
        "what a call gives is a later pass's"
    );
    assert_eq!(
        checked("const m = point.norm\n").2,
        ["the method `norm` is read as a value: call it, or wrap the call in an arrow (SPEC.md §12)"]
    );
    assert_eq!(found("f(point.norm())\npoint.norm().x\n"), NONE);
}
