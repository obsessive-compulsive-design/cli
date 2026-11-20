//! STORIES.md US-068: names a file does not declare, against `ambient/globals.toml`.

use sjs_bind::bind;
use sjs_diag::SourceFile;
use sjs_syntax::parse;

/// The findings about undeclared names in `text` as the file `path`:
/// `CODE @line:column text-the-span-covers`.
fn found_in(path: &str, text: &str) -> Vec<String> {
    let source = SourceFile::new(path, text.as_bytes().to_vec()).expect("utf-8");
    let (tree, _) = parse(&source);
    let (_, diagnostics) = bind(&source, &tree);
    diagnostics
        .iter()
        .filter(|d| matches!(d.code.as_str(), "E204" | "E133" | "E138"))
        .map(|d| {
            let at = d.span.start;
            let covered = source.slice(&d.span).expect("in the file");
            format!("{} @{}:{} {covered}", d.code.as_str(), at.line, at.column)
        })
        .collect()
}

fn found(text: &str) -> Vec<String> {
    found_in("a.js", text)
}

fn messages(path: &str, text: &str) -> Vec<String> {
    let source = SourceFile::new(path, text.as_bytes().to_vec()).expect("utf-8");
    let (tree, _) = parse(&source);
    let (_, diagnostics) = bind(&source, &tree);
    let about = diagnostics
        .iter()
        .filter(|d| matches!(d.code.as_str(), "E204" | "E133" | "E138"));
    about.map(|d| d.message.clone()).collect()
}

const NONE: [&str; 0] = [];

#[test]
fn the_globals_of_the_table_and_their_kept_members_resolve() {
    assert_eq!(
        found(concat!(
            "const o = [undefined, NaN, Infinity]\n",
            "fetch(\"x\")\nObject.keys(o)\nMath.max(1)\nSymbol.iterator\nencodeURIComponent(\"a\")\n",
            "navigator.hardwareConcurrency\nconsole.log(o)\nIntl.Anything\nURL.canParse(\"a\")\n",
            "Object[\"keys\"]\nObject?.keys\nObject?.[\"keys\"]\nNaN.toFixed\nUint8Array.from(o)\n",
            "new Map()\nnew Promise(o)\nsetTimeout(o, 1)\nconst k = \"nope\"\nObject[k]\nSymbol[k]\nObject[0]\n",
            "const f = (Object, missing) => Object.nope + missing\nf(Symbol.asyncDispose)\n",
        )),
        NONE,
        "values, functions, namespaces, kept members by name or string, any member of an open global, a computed key (the checker's), a parameter of a global's name"
    );
}

#[test]
fn a_name_that_is_no_global_is_e204() {
    assert_eq!(
        found("foo\nfoo.bar\nbar()\ntypeof baz\nconst a = Boolean\nexport { nowhere }\n"),
        [
            "E204 @1:1 foo",
            "E204 @2:1 foo",
            "E204 @3:1 bar",
            "E204 @4:8 baz",
            "E204 @5:11 Boolean",
            "E204 @6:10 nowhere",
        ],
        "on the name, whatever is done with it; a built-in §6 does not list is none"
    );
    assert_eq!(
        messages("a.js", "foo\n"),
        ["`foo` is not declared, and is no global of the five runtimes (SPEC.md §5.2)"]
    );
}

#[test]
fn a_global_or_member_a_runtime_lacks_is_e204_naming_the_runtime() {
    assert_eq!(
        found("Temporal.Now\nwindow\nprocess.env\nUint8Array.fromBase64(\"\")\nUint8Array[\"fromHex\"]\nBun\nself\n"),
        [
            "E204 @1:1 Temporal",
            "E204 @2:1 window",
            "E204 @3:1 process",
            "E204 @4:12 fromBase64",
            "E204 @5:12 \"fromHex\"",
            "E204 @6:1 Bun",
            "E204 @7:1 self",
        ]
    );
    assert_eq!(
        messages("a.js", "Temporal\nwindow\nself\nprocess\nUint8Array.fromBase64\n"),
        [
            "`Temporal` is missing on browsers, Node 24, Deno 2, Bun and Cloudflare Workers: a program runs on all five runtimes (SPEC.md §5.2)",
            "`window` is missing on Node 24, Deno 2, Bun and Cloudflare Workers: a program runs on all five runtimes; use a host file outside the subset (SPEC.md §5.2)",
            "`self` is missing on Node 24: a program runs on all five runtimes; use a host file outside the subset (SPEC.md §5.2)",
            "`process` is missing on browsers and Cloudflare Workers: a program runs on all five runtimes; use a host file outside the subset (SPEC.md §5.2)",
            "`Uint8Array.fromBase64` is missing on Node 24 and Deno 2: a program runs on all five runtimes (SPEC.md §6)",
        ]
    );
}

#[test]
fn a_removed_global_or_member_is_e133_with_its_replacement() {
    assert_eq!(
        found(concat!(
            "globalThis.x\nencodeURI(\"a\")\nnew WeakRef(NaN)\nescape(\"a\")\nparseInt(\"1\")\nnew Proxy(NaN)\n",
            "Object.seal(NaN)\nObject.is(NaN, NaN)\nconsole.trace()\nnavigator.userAgent\nSymbol.for(\"a\")\n",
            "Object[\"create\"]\nError.captureStackTrace\nSymbol()\nconst s = Symbol\nMap.nope\nfetch.name\n",
            "Symbol.iterator.description\n",
        )),
        [
            "E133 @1:1 globalThis",
            "E133 @2:1 encodeURI",
            "E133 @3:5 WeakRef",
            "E133 @4:1 escape",
            "E133 @5:1 parseInt",
            "E133 @6:5 Proxy",
            "E133 @7:8 seal",
            "E133 @8:8 is",
            "E133 @9:9 trace",
            "E133 @10:11 userAgent",
            "E133 @11:8 for",
            "E133 @12:8 \"create\"",
            "E133 @13:7 captureStackTrace",
            "E133 @14:1 Symbol",
            "E133 @15:11 Symbol",
            "E133 @16:5 nope",
            "E133 @17:7 name",
        ],
        "a removed global on its name; a removed or unlisted member on the member; `Symbol` but for its members"
    );
    assert_eq!(
        messages(
            "a.js",
            "globalThis\nWeakRef\nescape\nparseInt\nObject.seal\nObject.is\nconsole.trace\nSymbol()\nSymbol.for\n"
        ),
        [
            "`globalThis` is removed: use a host file outside the subset (SPEC.md §6.4)",
            "`WeakRef` is removed: there is no replacement (SPEC.md §6.4)",
            "`escape` is removed (SPEC.md §6.4)",
            "`parseInt` is removed: use `Number.parseInt` (SPEC.md §6.1)",
            "`Object.seal` is removed: use `Object.freeze` (SPEC.md §6.1)",
            "`Object.is` is removed (SPEC.md §6.1)",
            "`console.trace` is not a member the language keeps (SPEC.md §6.2)",
            "`Symbol` is used through its members only: use `#private`, well-known symbols only (SPEC.md §6.1)",
            "`Symbol.for` is removed: use `#private`, well-known symbols only (SPEC.md §6.1)",
        ]
    );
}

#[test]
fn the_test_globals_are_globals_of_test_files_only() {
    let text = "describe(\"a\", () => {\n  it(\"b\", () => {\n    expect(1).toBe(1)\n  })\n  it.skip(\"c\", () => {})\n  describe.only(\"d\", () => {})\n})\nbeforeEach(() => {})\nafterEach(() => {})\nbeforeAll(() => {})\nafterAll(() => {})\n";
    assert_eq!(found_in("src/a.test.js", text), NONE);
    assert_eq!(
        found_in("src/a.js", text),
        [
            "E204 @1:1 describe",
            "E204 @2:3 it",
            "E204 @3:5 expect",
            "E204 @5:3 it",
            "E204 @6:3 describe",
            "E204 @8:1 beforeEach",
            "E204 @9:1 afterEach",
            "E204 @10:1 beforeAll",
            "E204 @11:1 afterAll",
        ]
    );
    assert_eq!(
        messages("a.test.js.txt", "it\n"),
        ["`it` is a test global: only a `.test.js` file has it (SPEC.md §10.1)"]
    );
    assert_eq!(
        found_in(
            "a.test.js",
            "it.each\nvi\njest.fn()\nwindow\nconsole.trace()\n"
        ),
        [
            "E133 @1:4 each",
            "E204 @2:1 vi",
            "E204 @3:1 jest",
            "E204 @4:1 window",
            "E133 @5:9 trace",
        ],
        "a test file has the test globals and nothing else more"
    );
}

#[test]
fn a_global_is_assigned_in_a_test_file_only() {
    let text = "fetch = 1\nsetTimeout ??= 2\n;[atob] = []\nconsole.log = 3\nMath.random = 4\nfoo = 5\nglobalThis = 6\nTemporal = 7\nSymbol = 8\n";
    assert_eq!(
        found(text),
        [
            "E138 @1:1 fetch",
            "E138 @2:1 setTimeout",
            "E138 @3:3 atob",
            "E204 @6:1 foo",
            "E133 @7:1 globalThis",
            "E204 @8:1 Temporal",
            "E138 @9:1 Symbol",
        ],
        "a member of a global is the checker's; what is no global, or removed, or missing is that first"
    );
    assert_eq!(
        found_in("a.test.js", text),
        [
            "E204 @6:1 foo",
            "E133 @7:1 globalThis",
            "E204 @8:1 Temporal",
            "E133 @9:1 Symbol",
        ],
        "a test file may put a fake in a global's place"
    );
    assert_eq!(
        messages("a.js", "fetch = 1\n"),
        ["`fetch` is a global: only a `.test.js` file may assign one (SPEC.md §10.3)"]
    );
}
