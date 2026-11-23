//! STORIES.md US-100: JSON written and read in-house, and the form of `types.json`.

use sjs_diag::{ExportedType, Json, ModuleExports, TypesDocument, TYPES_VERSION};

fn sample() -> Json {
    Json::map(vec![
        ("null", Json::Null),
        (
            "flags",
            Json::List(vec![Json::Bool(true), Json::Bool(false)]),
        ),
        (
            "numbers",
            Json::List(vec![Json::Int(0), Json::Int(-12), Json::Int(i64::MAX)]),
        ),
        (
            "text",
            Json::text("a \"quoted\" \\ back\nline\ttab\rreturn \u{1} é 😀 /"),
        ),
        ("empty", Json::List(Vec::new())),
        ("none", Json::Map(Vec::new())),
        (
            "deep",
            Json::List(vec![Json::map(vec![("k", Json::List(vec![Json::Int(1)]))])]),
        ),
    ])
}

#[test]
fn a_value_is_written_one_way_and_read_back() {
    let written = sample().write();
    assert_eq!(
        written,
        "{\n  \"null\": null,\n  \"flags\": [\n    true,\n    false\n  ],\n  \"numbers\": [\n    0,\n    -12,\n    9223372036854775807\n  ],\n  \"text\": \"a \\\"quoted\\\" \\\\ back\\nline\\ttab\\rreturn \\u0001 é 😀 /\",\n  \"empty\": [],\n  \"none\": {},\n  \"deep\": [\n    {\n      \"k\": [\n        1\n      ]\n    }\n  ]\n}\n"
    );
    assert_eq!(Json::read(&written), Ok(sample()));
    assert_eq!(Json::Int(7).write(), "7\n");
    assert_eq!(
        Json::read(" \t\r\n[ 1 , \"\\u0041\\/\\b\\f\" , { \"a\" : null } ] \n"),
        Ok(Json::List(vec![
            Json::Int(1),
            Json::text("A/\u{8}\u{c}"),
            Json::map(vec![("a", Json::Null)]),
        ]))
    );
    assert_eq!(Json::read("\"\\u00e9x\""), Ok(Json::text("éx")));
}

#[test]
fn what_is_no_json_is_refused_with_its_place() {
    for (text, why) in [
        ("", "byte 0: no value"),
        ("nul", "byte 0: no value"),
        ("-", "byte 1: no value"),
        ("1.5", "byte 1: more after the value"),
        ("1 2", "byte 2: more after the value"),
        ("[1", "byte 2: neither `,` nor `]`"),
        ("[1,]", "byte 3: a comma before the `]`"),
        ("[1 2]", "byte 3: neither `,` nor `]`"),
        ("{\"a\":1,}", "byte 7: a comma before the `}`"),
        ("{\"a\" 1}", "byte 5: no `:` after a key"),
        ("{\"a\":}", "byte 5: no value"),
        ("{\"a\":1 \"b\":2}", "byte 7: neither `,` nor `}`"),
        ("{a:1}", "byte 1: no string"),
        ("\"abc", "byte 4: a string that does not end"),
        ("\"a\\x\"", "byte 4: no such escape"),
        ("\"a\\", "byte 3: no such escape"),
        ("\"\\ud800\"", "byte 7: no such `\\u` escape"),
        ("\"\\u12\"", "byte 6: no such `\\u` escape"),
        ("\"\\uzzzz\"", "byte 7: no such `\\u` escape"),
        ("\"a\nb\"", "byte 3: a control character"),
        ("99999999999999999999", "byte 20: no value"),
    ] {
        assert_eq!(Json::read(text), Err(why.to_owned()), "{text:?}");
    }
}

#[test]
fn the_members_of_a_value_are_asked_for_by_kind() {
    let value = sample();
    assert_eq!(value.get("null"), Some(&Json::Null));
    assert_eq!(value.get("missing"), None);
    assert_eq!(Json::Int(1).get("null"), None);
    assert_eq!(
        value.get("text").and_then(Json::as_text).map(str::len),
        Some(45)
    );
    assert_eq!(Json::Int(1).as_text(), None);
    assert_eq!(Json::Bool(true).as_bool(), Some(true));
    assert_eq!(Json::Bool(false).as_bool(), Some(false));
    assert_eq!(Json::Null.as_bool(), None);
    assert_eq!(
        value
            .get("flags")
            .and_then(Json::as_list)
            .map(<[Json]>::len),
        Some(2)
    );
    assert_eq!(Json::Null.as_list(), None);
}

fn document() -> TypesDocument {
    TypesDocument {
        modules: vec![
            ModuleExports {
                path: "src/a.js".to_owned(),
                syntax_error: false,
                exports: vec![ExportedType {
                    name: "greet".to_owned(),
                    inferred: true,
                    notation: "greet(name: string): string".to_owned(),
                    structure: Json::map(vec![("kind", Json::text("function"))]),
                }],
            },
            ModuleExports {
                path: "src/b.js".to_owned(),
                syntax_error: true,
                exports: vec![ExportedType {
                    name: "broken".to_owned(),
                    inferred: false,
                    notation: "broken: unknown".to_owned(),
                    structure: Json::map(vec![("kind", Json::text("unknown"))]),
                }],
            },
        ],
    }
}

#[test]
fn types_json_is_read_back_as_it_was_written() {
    let written = document().to_json();
    assert!(written.starts_with("{\n  \"version\": 1,\n  \"modules\": [\n    {\n      \"path\": \"src/a.js\",\n      \"syntaxError\": false,\n      \"exports\": [\n        {\n          \"name\": \"greet\",\n          \"inferred\": true,\n          \"type\": \"greet(name: string): string\",\n          \"structure\": {\n            \"kind\": \"function\"\n          }\n"));
    assert_eq!(TypesDocument::from_json(&written), Ok(document()));
    let empty = TypesDocument::default();
    assert_eq!(
        empty.to_json(),
        "{\n  \"version\": 1,\n  \"modules\": []\n}\n"
    );
    assert_eq!(TypesDocument::from_json(&empty.to_json()), Ok(empty));
    assert_eq!(TYPES_VERSION, 1);
}

#[test]
fn another_document_is_refused() {
    let wrong = |from: &str, to: &str| {
        let written = document().to_json();
        assert!(written.contains(from), "{from}");
        TypesDocument::from_json(&written.replacen(from, to, 1))
    };
    let missing = |what: &str| Err(format!("`{what}` is missing or of another kind"));
    assert_eq!(
        TypesDocument::from_json("["),
        Err("byte 1: no value".to_owned())
    );
    assert_eq!(
        wrong("\"version\": 1", "\"version\": 2"),
        Err("not version 1 of `types.json`".to_owned())
    );
    assert_eq!(
        TypesDocument::from_json("{}"),
        Err("not version 1 of `types.json`".to_owned())
    );
    assert_eq!(wrong("\"modules\": [", "\"other\": ["), missing("modules"));
    assert_eq!(
        TypesDocument::from_json("{\"version\": 1, \"modules\": 1}"),
        missing("modules")
    );
    assert_eq!(
        wrong("\"path\": \"src/a.js\"", "\"path\": 1"),
        missing("path")
    );
    assert_eq!(
        wrong("\"syntaxError\": false", "\"syntaxError\": \"no\""),
        missing("syntaxError")
    );
    assert_eq!(
        wrong("\"exports\": [", "\"exported\": ["),
        missing("exports")
    );
    assert_eq!(
        wrong("\"name\": \"greet\"", "\"name\": null"),
        missing("name")
    );
    assert_eq!(
        wrong("\"inferred\": true", "\"inferred\": 1"),
        missing("inferred")
    );
    assert_eq!(
        wrong("\"type\": \"greet", "\"kind\": \"greet"),
        missing("type")
    );
    assert_eq!(
        wrong("\"structure\": {", "\"shape\": {"),
        missing("structure")
    );
}
