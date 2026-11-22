//! STORIES.md US-088: the operators of SPEC.md §4.1.

use sjs_bind::bind;
use sjs_diag::SourceFile;
use sjs_infer::{check_expressions, infer_bindings, Facts};
use sjs_syntax::parse;
use sjs_types::{ClassId, Function, Literal, Number, Printer, Property, Type, TypeId, Types};

fn class_name(class: ClassId) -> String {
    format!("Class{}", class.0)
}

/// The names every test may use, each declared `const name = f()` and given a type.
fn known(types: &mut Types) -> Vec<(&'static str, TypeId)> {
    let literal =
        |types: &mut Types, value: f64| types.literal(Literal::Number(Number::new(value)));
    let (one, two, three) = (
        literal(types, 1.0),
        literal(types, 2.0),
        literal(types, 3.0),
    );
    let shape = types.intern(Type::Object(vec![Property {
        key: "a".to_owned(),
        ty: TypeId::NUMBER,
        literal: false,
    }]));
    let function = types.intern(Type::Function(Function {
        type_params: Vec::new(),
        params: Vec::new(),
        rest: None,
        returns: TypeId::NUMBER,
    }));
    vec![
        ("num", TypeId::NUMBER),
        ("int", TypeId::INT32),
        ("big", TypeId::BIGINT),
        ("str", TypeId::STRING),
        ("flag", TypeId::BOOLEAN),
        ("maybe", types.union([TypeId::STRING, TypeId::UNDEFINED])),
        ("nullable", types.union([TypeId::NUMBER, TypeId::NULL])),
        ("obj", shape),
        ("fun", function),
        ("point", types.intern(Type::Class(ClassId(0), Vec::new()))),
        ("one_two", types.union([one, two])),
        ("two_three", types.union([two, three])),
        ("three", three),
        ("raw", TypeId::UNKNOWN),
        ("mixed", types.union([TypeId::NUMBER, TypeId::STRING])),
    ]
}

/// The type inferred for `x` (`?` for none), the findings as `CODE @line:column text`
/// with lines counted from the body's first, and the messages.
fn checked(body: &str) -> (String, Vec<String>, Vec<String>) {
    let mut types = Types::new();
    let names = known(&mut types);
    let head = names
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
    let facts = infer_bindings(&source, &tree, &bindings, &facts, &mut types);
    let diagnostics = check_expressions(&source, &tree, &bindings, &facts, &mut types);
    let found = diagnostics
        .iter()
        .map(|d| {
            let covered = source.slice(&d.span).expect("in the file");
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

/// The type of an expression no finding is made on.
fn typed(expression: &str) -> String {
    let (printed, found, _) = checked(&format!("const x = {expression}\n"));
    assert_eq!(found, Vec::<String>::new(), "{expression}");
    printed
}

/// The one finding on an expression, without its line.
fn refused(expression: &str) -> String {
    let (printed, found, _) = checked(&format!("const x = {expression}\n"));
    assert_eq!(found.len(), 1, "{expression}: {found:?} ({printed})");
    found[0].replace(" @1:", " @")
}

#[test]
fn arithmetic_takes_two_of_a_kind() {
    for operator in ["+", "-", "*", "/", "**", "%"] {
        assert_eq!(
            typed(&format!("num {operator} num")),
            "number",
            "{operator}"
        );
        assert_eq!(
            typed(&format!("num {operator} int")),
            "number",
            "{operator}"
        );
        assert_eq!(
            typed(&format!("big {operator} big")),
            "bigint",
            "{operator}"
        );
        assert_eq!(
            typed(&format!("one_two {operator} 1.5")),
            "number",
            "{operator}"
        );
        assert_eq!(
            refused(&format!("num {operator} big")),
            format!("E138 @15 {operator}")
        );
        assert_eq!(
            refused(&format!("flag {operator} flag")),
            format!("E138 @16 {operator}")
        );
        assert_eq!(
            refused(&format!("num {operator} maybe")),
            format!("E138 @15 {operator}")
        );
        assert_eq!(
            refused(&format!("obj {operator} 1")),
            format!("E138 @15 {operator}")
        );
        assert_eq!(
            typed(&format!("num {operator} later()")),
            "?",
            "what has no type yet is not judged"
        );
        assert_eq!(typed(&format!("later() {operator} str")), "?");
    }
    for operator in ["+", "-", "*", "/", "**"] {
        assert_eq!(
            typed(&format!("int {operator} int")),
            "number",
            "an int32 may leave its range: {operator}"
        );
        assert_eq!(typed(&format!("1 {operator} 2")), "number");
    }
    assert_eq!(typed("int % int"), "int32", "`%` on two int32s stays one");
    assert_eq!(typed("7 % 2"), "int32");
    assert_eq!(typed("int % num"), "number");
    assert_eq!(typed("str + str"), "string");
    assert_eq!(typed("\"a\" + str"), "string");
    assert_eq!(refused("\"a\" + 1"), "E138 @15 +");
    assert_eq!(refused("1 + 1n"), "E138 @13 +");
    assert_eq!(refused("str - str"), "E138 @15 -");
    assert_eq!(
        refused("mixed + mixed"),
        "E138 @17 +",
        "a union of kinds is none of them"
    );
    assert_eq!(
        checked("num + str\nstr * str\n").2,
        [
            "`+` takes two numbers, two bigints or two strings: these are a `number` and a `string` (SPEC.md §4.1)",
            "`*` takes two numbers or two bigints: these are a `string` and a `string` (SPEC.md §4.1)",
        ]
    );
}

#[test]
fn unary_minus_plus_and_tilde() {
    assert_eq!(typed("-num"), "number");
    assert_eq!(typed("-int"), "number");
    assert_eq!(typed("-big"), "bigint");
    assert_eq!(typed("+num"), "number");
    assert_eq!(typed("+int"), "number");
    assert_eq!(typed("+big"), "bigint");
    assert_eq!(typed("+1"), "number");
    assert_eq!(typed("+2n"), "bigint");
    assert_eq!(typed("-2n"), "-2n");
    assert_eq!(typed("-1"), "-1");
    assert_eq!(typed("-one_two"), "number");
    assert_eq!(refused("-\"1\""), "E138 @12 \"1\"");
    assert_eq!(refused("-str"), "E138 @12 str");
    assert_eq!(refused("+flag"), "E138 @12 flag");
    assert_eq!(refused("-maybe"), "E138 @12 maybe");
    assert_eq!(typed("~int"), "int32");
    assert_eq!(typed("~1"), "int32");
    assert_eq!(typed("~big"), "bigint");
    assert_eq!(refused("~num"), "E104 @12 num");
    assert_eq!(refused("~1.5"), "E104 @12 1.5");
    assert_eq!(refused("~str"), "E138 @12 str");
    assert_eq!(typed("-later()"), "?");
    assert_eq!(typed("~later()"), "?");
    assert_eq!(
        checked("const a = -str\nconst b = ~num\nconst c = ~flag\n").2,
        [
            "`-` takes a number or a bigint: this is a `string` (SPEC.md §4.1)",
            "`~` on a `number`: a bitwise operator takes an `int32` or a bigint, and a number that may be no integer in range is neither (SPEC.md §3.5)",
            "`~` takes an int32 or a bigint: this is a `boolean` (SPEC.md §4.1)",
        ]
    );
}

#[test]
fn bitwise_operators_take_int32s_or_bigints() {
    for operator in ["&", "|", "^", "<<", ">>"] {
        assert_eq!(typed(&format!("int {operator} int")), "int32", "{operator}");
        assert_eq!(typed(&format!("int {operator} 1")), "int32", "{operator}");
        assert_eq!(
            typed(&format!("big {operator} big")),
            "bigint",
            "{operator}"
        );
        assert_eq!(
            refused(&format!("num {operator} int")),
            format!("E104 @15 {operator}")
        );
        assert_eq!(
            refused(&format!("int {operator} 1.5")),
            format!("E104 @15 {operator}")
        );
        assert_eq!(
            refused(&format!("int {operator} big")),
            format!("E138 @15 {operator}")
        );
        assert_eq!(
            refused(&format!("str {operator} str")),
            format!("E138 @15 {operator}")
        );
        assert_eq!(typed(&format!("int {operator} later()")), "?");
    }
    assert_eq!(typed("int >>> int"), "number", "up to 2^32 - 1");
    assert_eq!(refused("big >>> big"), "E138 @15 >>>");
    assert_eq!(refused("num >>> int"), "E104 @15 >>>");
    assert_eq!(refused("int >>> str"), "E138 @15 >>>");
    assert_eq!(
        refused("(int + int) & 1"),
        "E104 @23 &",
        "the sum of two int32s is a number"
    );
    assert_eq!(typed("(int % int) & 1"), "int32");
    assert_eq!(
        checked("num & int\nbig >>> big\nint | big\n").2,
        [
            "`&` on a `number` and a `int32`: a bitwise operator takes int32s, and a number that may be no integer in range is none (SPEC.md §3.5)",
            "`>>>` takes two int32s: these are a `bigint` and a `bigint` (SPEC.md §4.1)",
            "`|` takes two int32s or two bigints: these are a `int32` and a `bigint` (SPEC.md §4.1)",
        ]
    );
}

#[test]
fn ordering_takes_two_numbers_bigints_or_strings() {
    for operator in ["<", ">", "<=", ">="] {
        assert_eq!(
            typed(&format!("num {operator} int")),
            "boolean",
            "{operator}"
        );
        assert_eq!(
            typed(&format!("big {operator} 1n")),
            "boolean",
            "{operator}"
        );
        assert_eq!(
            typed(&format!("str {operator} \"a\"")),
            "boolean",
            "{operator}"
        );
        assert_eq!(
            refused(&format!("num {operator} str")),
            format!("E138 @15 {operator}")
        );
        assert_eq!(
            refused(&format!("num {operator} big")),
            format!("E138 @15 {operator}")
        );
        assert_eq!(
            refused(&format!("flag {operator} true")),
            format!("E138 @16 {operator}")
        );
        assert_eq!(
            refused(&format!("maybe {operator} str")),
            format!("E138 @17 {operator}")
        );
        assert_eq!(typed(&format!("num {operator} later()")), "boolean");
        assert_eq!(
            checked(&format!("const x = num {operator} obj\n")).0,
            "boolean",
            "a boolean all the same"
        );
    }
}

#[test]
fn strict_equality_needs_types_that_overlap() {
    for same in [
        "num === num2",
        "num === int",
        "num === 1",
        "1 === num",
        "1 === int",
        "1.5 === num",
        "int === 1",
        "num === 1.5",
        "str === \"a\"",
        "flag === true",
        "big === 1n",
        "maybe === undefined",
        "maybe === str",
        "nullable === null",
        "one_two === two_three",
        "one_two === 2",
        "raw === 1",
        "raw === obj",
        "obj === obj2",
        "obj === point",
        "mixed === str",
        "num !== int",
        "later() === 1",
        "1 === later()",
        "typeof num === \"number\"",
        "typeof raw === \"object\"",
        "typeof later() === \"function\"",
        "typeof maybe === \"undefined\"",
        "typeof obj === \"object\"",
        "typeof nullable === \"object\"",
        "typeof fun === \"function\"",
        "typeof big === \"bigint\"",
        "typeof flag === \"boolean\"",
        "typeof str === \"string\"",
        "1 === 1",
        "fun === fun2",
    ] {
        let body =
            format!("const num2 = num\nconst obj2 = obj\nconst fun2 = fun\nconst x = {same}\n");
        assert_eq!(checked(&body).1, Vec::<String>::new(), "{same}");
        assert_eq!(checked(&body).0, "boolean", "{same}");
    }
    for (disjoint, at) in [
        ("num === undefined", 15),
        ("num === str", 15),
        ("num === big", 15),
        ("int === 1.5", 15),
        ("1.5 === int", 15),
        ("1 === 2", 13),
        ("str === 1", 15),
        ("flag === 1", 16),
        ("maybe === 1", 17),
        ("nullable === undefined", 20),
        ("one_two === three", 19),
        ("one_two === \"a\"", 19),
        ("obj === 1", 15),
        ("obj === undefined", 15),
        ("point === \"a\"", 17),
        ("num === null", 15),
        ("fun === 1", 15),
        ("typeof num === \"symbol\"", 22),
        ("typeof raw === \"symbol\"", 22),
        ("typeof num === \"string\"", 22),
        ("typeof str === \"object\"", 22),
        ("typeof later() === \"nothing\"", 26),
        ("\"a\" === \"b\"", 15),
        ("true === false", 16),
        ("1n === 2n", 14),
    ] {
        assert_eq!(refused(disjoint), format!("E105 @{at} ==="), "{disjoint}");
    }
    assert_eq!(refused("num !== str"), "E105 @15 !==");
    assert_eq!(
        checked("num === undefined\n").2,
        ["a `number` and a `undefined` share no value: `===` between them always gives the same answer (SPEC.md §4.1)"]
    );
}

#[test]
fn boolean_operators_take_booleans() {
    assert_eq!(typed("flag && flag"), "boolean");
    assert_eq!(typed("flag || true"), "boolean");
    assert_eq!(typed("!flag"), "boolean");
    assert_eq!(typed("!(num < 1) && later()"), "boolean");
    assert_eq!(refused("num && flag"), "E138 @11 num");
    assert_eq!(refused("flag || str"), "E138 @19 str");
    assert_eq!(
        refused("maybe || \"d\""),
        "E138 @11 maybe",
        "the first that is none, once"
    );
    assert_eq!(refused("!num"), "E138 @12 num");
    assert_eq!(refused("!maybe"), "E138 @12 maybe");
    assert_eq!(refused("!obj"), "E138 @12 obj");
    assert_eq!(checked("const x = num && str\n").0, "boolean");
    assert_eq!(
        checked("num && flag\n!str\n").2,
        [
            "`&&` takes booleans: this is a `number`; use `??` for a default and `?:` to choose (SPEC.md §4.1)",
            "`!` takes a boolean: this is a `string` (SPEC.md §4.1)",
        ]
    );
}

#[test]
fn nullish_coalescing_and_the_conditional() {
    assert_eq!(typed("maybe ?? \"d\""), "string");
    assert_eq!(typed("nullable ?? str"), "number | string");
    assert_eq!(typed("undefined ?? 1"), "1");
    assert_eq!(typed("maybe ?? later()"), "?");
    assert_eq!(typed("later() ?? 1"), "?");
    assert_eq!(refused("num ?? 1"), "E138 @11 num");
    assert_eq!(refused("obj ?? obj"), "E138 @11 obj");
    assert_eq!(
        checked("const x = str ?? 1\n").0,
        "string",
        "the left is all there is"
    );
    assert_eq!(typed("flag ? 1 : \"a\""), "1 | \"a\"");
    assert_eq!(typed("num < 1 ? num : str"), "number | string");
    assert_eq!(typed("later() ? 1 : 2"), "1 | 2");
    assert_eq!(typed("flag ? later() : 2"), "?");
    assert_eq!(typed("flag ? 1 : later()"), "?");
    assert_eq!(refused("num ? 1 : 2"), "E138 @11 num");
    assert_eq!(refused("maybe ? 1 : 2"), "E138 @11 maybe");
    assert_eq!(checked("const x = str ? 1 : 2\n").0, "1 | 2");
    assert_eq!(
        checked("num ?? 1\nnum ? 1 : 2\n").2,
        [
            "the left of `??` is a `number`, which is never `undefined` or `null`: the right is never used (SPEC.md §4.1)",
            "the condition of `?:` is a boolean: this is a `number` (SPEC.md §4.1)",
        ]
    );
}

#[test]
fn typeof_is_the_strings_its_operand_may_give() {
    assert_eq!(typed("typeof num"), "\"number\"");
    assert_eq!(typed("typeof int"), "\"number\"");
    assert_eq!(typed("typeof 1"), "\"number\"");
    assert_eq!(typed("typeof big"), "\"bigint\"");
    assert_eq!(typed("typeof 1n"), "\"bigint\"");
    assert_eq!(typed("typeof str"), "\"string\"");
    assert_eq!(typed("typeof \"a\""), "\"string\"");
    assert_eq!(typed("typeof flag"), "\"boolean\"");
    assert_eq!(typed("typeof true"), "\"boolean\"");
    assert_eq!(typed("typeof maybe"), "\"string\" | \"undefined\"");
    assert_eq!(
        typed("typeof nullable"),
        "\"number\" | \"object\"",
        "`typeof null` is \"object\""
    );
    assert_eq!(typed("typeof obj"), "\"object\"");
    assert_eq!(typed("typeof point"), "\"object\"");
    assert_eq!(typed("typeof [1]"), "\"object\"");
    assert_eq!(typed("typeof fun"), "\"function\"");
    let all = "\"bigint\" | \"boolean\" | \"function\" | \"number\" | \"object\" | \"string\" | \"undefined\"";
    assert_eq!(
        typed("typeof raw"),
        all,
        "anything but a symbol: there are none"
    );
    assert_eq!(typed("typeof later()"), all);
    assert_eq!(typed("typeof typeof num"), "\"string\"");
}

#[test]
fn instanceof_takes_a_class_on_its_right() {
    assert_eq!(typed("point instanceof Point"), "boolean");
    assert_eq!(typed("raw instanceof Error"), "boolean");
    assert_eq!(typed("later() instanceof ns.Thing"), "boolean");
    assert_eq!(refused("raw instanceof obj"), "E138 @26 obj");
    assert_eq!(refused("raw instanceof 1"), "E138 @26 1");
    assert_eq!(refused("raw instanceof \"Point\""), "E138 @26 \"Point\"");
    assert_eq!(
        refused("raw instanceof point"),
        "E138 @26 point",
        "an instance is no class"
    );
    assert_eq!(
        checked("raw instanceof obj\n").2,
        ["the right of `instanceof` is a class: this is a `{ a: number }` (SPEC.md §4.1)"]
    );
    assert_eq!(
        checked("class Point {}\nconst x = raw instanceof Point\n"),
        ("boolean".to_owned(), Vec::new(), Vec::new())
    );
}

#[test]
fn a_place_compared_with_or_assigned_to_itself_is_e120() {
    for (compare, text) in [
        ("num === num", "num === num"),
        ("num !== num", "num !== num"),
        ("str < str", "str < str"),
        ("num >= num", "num >= num"),
        ("obj.a === obj.a", "obj.a === obj.a"),
        ("obj.a  ===  obj . a", "obj.a  ===  obj . a"),
        ("obj[\"a\"] <= obj[\"a\"]", "obj[\"a\"] <= obj[\"a\"]"),
        ("later === later", "later === later"),
    ] {
        assert_eq!(refused(compare), format!("E120 @11 {text}"), "{compare}");
        assert_eq!(checked(&format!("const x = {compare}\n")).0, "boolean");
    }
    for other in [
        "num === num2",
        "obj.a === obj.b",
        "f() === f()",
        "1 === 1",
        "num + num",
        "num - num",
        "obj.a === obj[\"a\"]",
        "(num) === num",
        "flag && flag",
        "obj.f().a === obj.f().a",
        "obj[f()] === obj[f()]",
    ] {
        let body = format!("const num2 = num\nconst x = {other}\n");
        let found = checked(&body).1;
        assert!(
            found.iter().all(|found| !found.starts_with("E120")),
            "{other}: {found:?}"
        );
    }
    assert_eq!(
        checked("let v = 1\nv = v\nobj.a = obj.a\nobj[\"a\"] = obj[\"a\"]\n").1,
        [
            "E120 @2:1 v = v",
            "E120 @3:1 obj.a = obj.a",
            "E120 @4:1 obj[\"a\"] = obj[\"a\"]"
        ]
    );
    assert_eq!(
        checked(
            "let v = 1\nv = v + 1\nv += v\nobj.a = num\nlet w = 2\nw = v\nv = (v)\n;[v] = [v]\n"
        )
        .1,
        Vec::<String>::new(),
        "another value, or another operator"
    );
    assert_eq!(
        checked("num === num\nlet v = 1\nv = v\n").2,
        [
            "`num` is compared with itself: the answer is always the same (SPEC.md §8.1)",
            "`v` is assigned to itself (SPEC.md §8.1)",
        ]
    );
    let source = SourceFile::new(
        "a.js",
        b"class K {\n  x = 1\n  m() {\n    this.x = this.x\n    return this === this\n  }\n}\n"
            .to_vec(),
    )
    .expect("utf-8");
    let (tree, _) = parse(&source);
    let (bindings, _) = bind(&source, &tree);
    let mut types = Types::new();
    let facts = infer_bindings(&source, &tree, &bindings, &Facts::default(), &mut types);
    let found = check_expressions(&source, &tree, &bindings, &facts, &mut types);
    let codes: Vec<(&str, u32)> = found
        .iter()
        .map(|d| (d.code.as_str(), d.span.start.line))
        .collect();
    assert_eq!(codes, [("E120", 4), ("E120", 5)], "`this` is a place too");
}
