//! STORIES.md US-083: the types of literal and primitive expressions, E100 and E125.

use sjs_bind::bind;
use sjs_diag::SourceFile;
use sjs_infer::{check_expressions, Facts, Typer};
use sjs_syntax::{parse, Element, NodeKind};
use sjs_types::{ClassId, Printer, Type, TypeId, Types};

fn class_name(class: ClassId) -> String {
    format!("Class{}", class.0)
}

/// What a test knows of the names of its text: the type of each, and what the
/// `toString()` of the classes returns.
type Names = (Vec<(&'static str, TypeId)>, Vec<(u32, TypeId)>);
type Known<'a> = &'a dyn Fn(&mut Types) -> Names;

fn facts(text: &str, types: &mut Types, known: Known<'_>) -> Facts {
    let source = SourceFile::new("a.js", text.as_bytes().to_vec()).expect("utf-8");
    let (tree, _) = parse(&source);
    let (bindings, _) = bind(&source, &tree);
    let (names, classes) = known(types);
    let mut facts = Facts::default();
    for (name, ty) in names {
        let declaration = bindings.lookup(0, name).expect("a declared name");
        facts.declarations.insert(declaration, ty);
    }
    for (class, returns) in classes {
        facts.to_string.insert(ClassId(class), returns);
        facts
            .class_names
            .insert(ClassId(class), format!("Named{class}"));
    }
    facts
}

/// The type of the value of the last `const` of `text`, printed; `?` when it cannot
/// be told yet.
fn typed_with(text: &str, known: Known<'_>) -> String {
    let mut types = Types::new();
    let facts = facts(text, &mut types, known);
    let source = SourceFile::new("a.js", text.as_bytes().to_vec()).expect("utf-8");
    let (tree, _) = parse(&source);
    let (bindings, _) = bind(&source, &tree);
    let last = tree
        .root()
        .children()
        .filter_map(|child| match child {
            Element::Node(node) if node.kind() == NodeKind::ConstDecl => Some(node),
            _ => None,
        })
        .last()
        .expect("a const");
    let value = last
        .children()
        .find_map(|child| match child {
            Element::Node(node) if node.kind() == NodeKind::Initializer => node.children().last(),
            _ => None,
        })
        .expect("a value");
    let mut judge = Typer::new(&source, &tree, &bindings, &facts, &mut types);
    let ty = judge.type_of(value);
    ty.map_or("?".to_owned(), |ty| {
        Printer::new(&types, &class_name).print(ty)
    })
}

fn nothing(_: &mut Types) -> Names {
    (Vec::new(), Vec::new())
}

fn typed(expression: &str) -> String {
    typed_with(&format!("const t = {expression}\n"), &nothing)
}

/// The findings of `text`: `CODE @line:column text`.
fn found_with(text: &str, known: Known<'_>) -> Vec<String> {
    let mut types = Types::new();
    let facts = facts(text, &mut types, known);
    let source = SourceFile::new("a.js", text.as_bytes().to_vec()).expect("utf-8");
    let (tree, _) = parse(&source);
    let (bindings, _) = bind(&source, &tree);
    check_expressions(&source, &tree, &bindings, &facts, &mut types)
        .iter()
        .map(|d| {
            let at = d.span.start;
            assert_eq!(d.path, "a.js");
            let covered = source.slice(&d.span).expect("in the file");
            format!("{} @{}:{} {covered}", d.code.as_str(), at.line, at.column)
        })
        .collect()
}

fn found(text: &str) -> Vec<String> {
    found_with(text, &nothing)
}

const NONE: [&str; 0] = [];

#[test]
fn a_number_literal_is_an_int32_when_it_is_one() {
    let int32 = |expression: &str| {
        let mut types = Types::new();
        let facts = Facts::default();
        let text = format!("const t = {expression}\n");
        let source = SourceFile::new("a.js", text.into_bytes()).expect("utf-8");
        let (tree, _) = parse(&source);
        let (bindings, _) = bind(&source, &tree);
        let value = tree.root().children().find_map(|child| match child {
            Element::Node(node) => node.children().find_map(|part| match part {
                Element::Node(init) => init.children().last(),
                Element::Token(_) => None,
            }),
            Element::Token(_) => None,
        });
        let mut judge = Typer::new(&source, &tree, &bindings, &facts, &mut types);
        let ty = judge.type_of(value.expect("a value")).expect("a type");
        match types.get(ty) {
            Type::Literal(literal) => literal.primitive() == TypeId::INT32,
            _ => false,
        }
    };
    for (expression, printed) in [
        ("0", "0"),
        ("1", "1"),
        ("1e3", "1000"),
        ("1.0", "1"),
        ("0x7fffffff", "2147483647"),
        ("2147483647", "2147483647"),
        ("-2147483648", "-2147483648"),
        ("-1", "-1"),
        ("(-(1))", "-1"),
        ("- -5", "5"),
        ("0b101", "5"),
    ] {
        assert_eq!(typed(expression), printed, "{expression}");
        assert!(int32(expression), "{expression} is an int32");
    }
    for (expression, printed) in [
        ("2147483648", "2147483648"),
        ("-2147483649", "-2147483649"),
        ("1.5", "1.5"),
        ("-0", "-0"),
        ("-0.0", "-0"),
        ("0.1", "0.1"),
        ("1.5e-7", "1.5e-7"),
        ("0x80000000", "2147483648"),
    ] {
        assert_eq!(typed(expression), printed, "{expression}");
        assert!(!int32(expression), "{expression} is a number");
    }
    assert_eq!(
        typed("9007199254740993"),
        "number",
        "past 2^53 the literal is not its value"
    );
    assert_eq!(typed("-9007199254740993"), "number");
}

#[test]
fn the_other_literals_and_what_is_plain_from_the_text() {
    for (expression, printed) in [
        ("1n", "1n"),
        ("-1n", "-1n"),
        ("- -7n", "7n"),
        ("-0n", "0n"),
        (
            "123456789012345678901234567890n",
            "123456789012345678901234567890n",
        ),
        ("\"a\"", "\"a\""),
        ("'it'", "\"it\""),
        ("`plain`", "\"plain\""),
        ("true", "true"),
        ("false", "false"),
        ("undefined", "undefined"),
        ("NaN", "number"),
        ("Infinity", "number"),
        ("-Infinity", "number"),
        ("null", "null"),
        ("`a${1}b`", "string"),
        ("!x", "boolean"),
        ("typeof x", "\"bigint\" | \"boolean\" | \"function\" | \"number\" | \"object\" | \"string\" | \"undefined\""),
        ("x === null", "boolean"),
        ("x !== 1", "boolean"),
        ("[1, 2]", "readonly int32[]"),
        ("1 + 2", "number"),
        ("+1", "number"),
        ("x ? 1 : 2", "1 | 2"),
        ("x < 1", "boolean"),
        ("{ a: 1 }", "{ a: 1 }"),
        ("[1, 1.5]", "readonly number[]"),
        ("[1, \"a\", true]", "readonly (int32 | string | boolean)[]"),
        (
            "[[1], [\"a\"]]",
            "readonly (readonly int32[] | readonly string[])[]",
        ),
        ("JSON.parse(\"1\")", "unknown"),
        ("(JSON.parse(text))", "unknown"),
        ("1 ?? 2", "1"),
        ("undefined ?? \"d\"", "\"d\""),
        ("null ?? 1", "1"),
    ] {
        assert_eq!(typed(expression), printed, "{expression}");
    }
    for later in [
        "x",
        "f()",
        "a + 1",
        "[]",
        "[x]",
        "[...xs]",
        "x ?? 1",
        "1 ?? x",
        "-x",
        "-\"a\"",
        "this",
        "JSON.stringify(1)",
        "o.JSON.parse(1)",
        "JSON.parse",
        "parse(1)",
        "(JSON.parse)(1)",
        "JSON.parse(1)(2)",
        "-(() => 1)",
        "await x",
        "/re/v",
        "void 0",
    ] {
        assert_eq!(typed(later), "?", "{later} is a later pass's");
    }
}

#[test]
fn names_have_the_types_that_are_known_of_them() {
    let known = |types: &mut Types| {
        let or_undefined = types.union([TypeId::STRING, TypeId::UNDEFINED]);
        let or_null = types.union([TypeId::INT32, TypeId::NULL, TypeId::UNDEFINED]);
        let names = vec![
            ("x", or_undefined),
            ("n", TypeId::NUMBER),
            ("i", TypeId::INT32),
            ("b", TypeId::BIGINT),
            ("m", or_null),
        ];
        (names, Vec::new())
    };
    let head = "import cfg from \"./c.json\" with { type: \"json\" }\nimport { other } from \"./o.js\"\nconst x = f()\nconst n = f()\nconst i = f()\nconst b = f()\nconst m = f()\nconst JSON = f()\n";
    let typed = |expression: &str| typed_with(&format!("{head}const t = {expression}\n"), &known);
    assert_eq!(typed("x"), "string | undefined");
    assert_eq!(
        typed("x ?? \"d\""),
        "string",
        "the left without `undefined`, with the right"
    );
    assert_eq!(typed("x ?? 1"), "string | 1");
    assert_eq!(
        typed("m ?? x"),
        "int32 | string | undefined",
        "without `null` too"
    );
    assert_eq!(
        typed("n ?? x"),
        "number",
        "a left that is never absent is all there is"
    );
    assert_eq!(typed("-n"), "number");
    assert_eq!(
        typed("-i"),
        "number",
        "the negation of the least int32 is none"
    );
    assert_eq!(typed("-b"), "bigint");
    assert_eq!(typed("-x"), "?");
    assert_eq!(typed("[x, i]"), "readonly (int32 | string | undefined)[]");
    assert_eq!(typed("cfg"), "unknown", "a JSON default import");
    assert_eq!(typed("other"), "?", "another import is not known yet");
    assert_eq!(
        typed("JSON.parse(x)"),
        "?",
        "a `JSON` the file declares is not the global"
    );
    assert_eq!(typed("((a) => a)"), "<T>(a: T): T");
    assert_eq!(typed("(() => 1)"), "(): int32");
    assert_eq!(
        typed("(a) => a"),
        "?",
        "a function declared by name is typed from its calls, not as a value"
    );
}

#[test]
fn null_is_written_only_to_the_right_of_a_strict_comparison() {
    assert_eq!(
        found(concat!(
            "const x = null\nconst f = () => {\n  return null\n}\nf(null)\nconst a = [null]\nconst o = { a: null }\n",
            "const p = { [null]: 1 }\nnull.x\nclass K {\n  a = null\n}\n",
            "null === x\nnull !== x\nconst t = x ?? null\nconst u = x === (null)\nconst v = x < null\nconst w = null + x\n",
        )),
        [
            "E100 @1:11 null",
            "E100 @3:10 null",
            "E100 @5:3 null",
            "E100 @6:12 null",
            "E100 @7:16 null",
            "E000 @8:14 null",
            "E100 @8:14 null",
            "E100 @9:1 null",
            "E137 @9:6 x",
            "E100 @11:7 null",
            "E100 @13:1 null",
            "E100 @14:1 null",
            "E100 @15:16 null",
            "E100 @16:18 null",
            "E100 @17:15 null",
            "E100 @18:11 null",
        ],
        "returned, bound, passed, in an array, in an object, on the left, under another operator, in parentheses"
    );
    assert_eq!(
        found("const a = x === null\nconst b = x !== null\nif (f(x) === null) {\n}\nconst c = x.y.z !== null ? 1 : 2\nconst o = { null: 1, a: x === null }\nconst m = { null() {\n} }\no.null\no?.null\nclass K {\n  null = 1\n  static null() {}\n  get null() {\n    return 1\n  }\n}\n"),
        NONE,
        "the right operand of `===` and `!==`; a key named `null` is no `null`"
    );
    let source = SourceFile::new("a.js", b"null\n".to_vec()).expect("utf-8");
    let (tree, _) = parse(&source);
    let (bindings, _) = bind(&source, &tree);
    let found = check_expressions(
        &source,
        &tree,
        &bindings,
        &Facts::default(),
        &mut Types::new(),
    );
    let messages: Vec<&str> = found.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(
        messages,
        ["`null` is written only to the right of `===` or `!==`: the absent value a program makes is `undefined` (SPEC.md §3.1)"]
    );
}

#[test]
fn a_template_is_filled_with_text() {
    let known = |types: &mut Types| {
        let or_undefined = types.union([TypeId::STRING, TypeId::UNDEFINED]);
        let texts = types.union([
            TypeId::STRING,
            TypeId::INT32,
            TypeId::BIGINT,
            TypeId::BOOLEAN,
        ]);
        let (shown, hidden, numbered, plain) = (
            types.intern(Type::Class(ClassId(0))),
            types.intern(Type::Class(ClassId(1))),
            types.intern(Type::Class(ClassId(2))),
            types.intern(Type::Class(ClassId(3))),
        );
        let either = types.union([shown, TypeId::NUMBER]);
        let neither = types.union([hidden, TypeId::NUMBER]);
        let literal = types.literal(sjs_types::Literal::String("fixed".to_owned()));
        let names = vec![
            ("x", or_undefined),
            ("all", texts),
            ("shown", shown),
            ("hidden", hidden),
            ("numbered", numbered),
            ("plain", plain),
            ("either", either),
            ("neither", neither),
            ("raw", TypeId::UNKNOWN),
        ];
        (
            names,
            vec![(0, TypeId::STRING), (2, TypeId::NUMBER), (3, literal)],
        )
    };
    let head = "const x = f()\nconst all = f()\nconst shown = f()\nconst hidden = f()\nconst numbered = f()\nconst plain = f()\nconst either = f()\nconst neither = f()\nconst raw = f()\n";
    let found = |body: &str| found_with(&format!("{head}{body}"), &known);
    assert_eq!(
        found("`${\"a\"} ${1} ${1.5} ${2n} ${true} ${all} ${x ?? \"d\"} ${shown} ${plain} ${either} ${`in ${1}`} ${later()} ${-1}`\n"),
        NONE,
        "strings, numbers, bigints, booleans and their unions; a class whose `toString()` returns a string; what cannot be told yet"
    );
    assert_eq!(
        found("`${x}`\n`${[1]} and ${undefined}`\n`${hidden}${numbered}${neither}${raw}`\n`${`inner ${[x]}`}`\n`${x ?? undefined}`\nconst t = `${JSON.parse(x)}`\n"),
        [
            "E125 @10:4 x",
            "E125 @11:4 [1]",
            "E125 @11:15 undefined",
            "E125 @12:4 hidden",
            "E125 @12:13 numbered",
            "E125 @12:24 neither",
            "E125 @12:34 raw",
            "E125 @13:13 [x]",
            "E125 @14:4 x ?? undefined",
            "E125 @15:14 JSON.parse(x)",
        ],
        "`undefined`, an array, a class without a `toString()` that returns a string, `unknown`; a template in a template once"
    );
    let source_text = format!("{head}`${{x}}${{hidden}}${{[1]}}`\n");
    let mut types = Types::new();
    let facts = facts(&source_text, &mut types, &known);
    let source = SourceFile::new("a.js", source_text.into_bytes()).expect("utf-8");
    let (tree, _) = parse(&source);
    let (bindings, _) = bind(&source, &tree);
    let found = check_expressions(&source, &tree, &bindings, &facts, &mut types);
    let messages: Vec<&str> = found.iter().map(|d| d.message.as_str()).collect();
    let rule = "it takes a string, a number, a bigint or a boolean, or an object whose `toString()` returns a string (SPEC.md §4.1)";
    assert_eq!(
        messages,
        [
            format!("a template is filled with a `string | undefined`: {rule}"),
            format!("a template is filled with a `Named1`: {rule}").replace("Named1", "class #1"),
            format!("a template is filled with a `readonly int32[]`: {rule}"),
        ]
    );
}
