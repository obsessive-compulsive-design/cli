//! Import declarations (SPEC.md §2 `ImportDecl`, §5.1): `import { a, b as c } from
//! "./x.js"`, `import * as ns from "../y.js"` and the one default form `import cfg from
//! "./c.json" with { type: "json" }`. Every other form is E000; the path's shape is
//! checked here (E201 for anything but a relative `.js`/`.json` path, E205 for a
//! mismatch between the extension and the `with` clause); whether the file exists is
//! US-060's.

use sjs_diag::{Code, Span};

use super::Parser;
use crate::token::{Keyword, Punct, TokenKind};
use crate::tree::NodeKind;

/// Which binding form the declaration used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Form {
    Named,
    Namespace,
    Default,
}

const LBRACE: TokenKind = TokenKind::Punct(Punct::LBrace);
const RBRACE: TokenKind = TokenKind::Punct(Punct::RBrace);
const COMMA: TokenKind = TokenKind::Punct(Punct::Comma);
const COLON: TokenKind = TokenKind::Punct(Punct::Colon);

/// Every import declaration at the start of the module (`import(` is a dynamic import,
/// an expression, so the section ends there).
pub(super) fn import_section(parser: &mut Parser<'_>) {
    while parser.peek(false).kind == TokenKind::Keyword(Keyword::Import)
        && parser.peek_second().kind != TokenKind::Punct(Punct::LParen)
    {
        import_decl(parser);
    }
}

/// One `import …` declaration, at the `import` keyword. A syntax error ends the
/// declaration at the offending token, reported once; what follows is left to the
/// module items.
pub(super) fn import_decl(parser: &mut Parser<'_>) {
    parser.start_node(NodeKind::ImportDecl);
    parser.bump(false);
    let form = match parser.peek(false).kind.clone() {
        LBRACE => named_imports(parser),
        TokenKind::Punct(Punct::Star) => namespace_import(parser),
        TokenKind::Ident => {
            parser.start_node(NodeKind::DefaultImport);
            parser.bump(false);
            parser.finish_node();
            Some(Form::Default)
        }
        TokenKind::String(_) => {
            let span = parser.peek(false).span;
            parser.error(
                span,
                "a side-effect import `import \"…\"` is removed: import a binding and use it (SPEC.md §5.1)",
            );
            None
        }
        _ => {
            let span = parser.peek(false).span;
            parser.error(span, "expected `{`, `*` or a name after `import`");
            None
        }
    };
    let Some(form) = form else {
        parser.finish_node();
        return;
    };
    if !parser.at_word("from") {
        let span = parser.peek(false).span;
        parser.error(span, "expected `from` after the imported names");
        parser.finish_node();
        return;
    }
    parser.bump(false);
    let TokenKind::String(path) = parser.peek(false).kind.clone() else {
        let span = parser.peek(false).span;
        parser.error(span, "expected a module path string after `from`");
        parser.finish_node();
        return;
    };
    let path_span = parser.peek(false).span;
    parser.bump(false);
    let attributes = (parser.peek(false).kind == TokenKind::Keyword(Keyword::With))
        .then(|| import_attributes(parser));
    check(parser, form, &path, path_span, attributes);
    parser.finish_node();
}

/// `{ a, b as c, }`: at least one name, each an identifier.
fn named_imports(parser: &mut Parser<'_>) -> Option<Form> {
    parser.start_node(NodeKind::NamedImports);
    let open = parser.peek(false).span;
    parser.bump(false);
    let mut named = false;
    loop {
        match parser.peek(false).kind.clone() {
            RBRACE => break,
            TokenKind::Ident => {
                parser.start_node(NodeKind::ImportSpecifier);
                parser.bump(false);
                if parser.at_word("as") {
                    parser.bump(false);
                    if parser.peek(false).kind != TokenKind::Ident {
                        let span = parser.peek(false).span;
                        parser.error(span, "expected a name after `as`");
                        parser.finish_node();
                        parser.finish_node();
                        return None;
                    }
                    parser.bump(false);
                }
                parser.finish_node();
                named = true;
            }
            TokenKind::Keyword(Keyword::Default) => {
                let span = parser.peek(false).span;
                parser.error(
                    span,
                    "`import { default as x }` is removed: a `.js` module has named exports only (SPEC.md §5.1)",
                );
                parser.finish_node();
                return None;
            }
            _ => {
                let span = parser.peek(false).span;
                parser.error(span, "expected a name in the import list");
                parser.finish_node();
                return None;
            }
        }
        if !parser.eat(&COMMA) && parser.peek(false).kind != RBRACE {
            let span = parser.peek(false).span;
            parser.error(span, "expected `,` or `}` after an imported name");
            parser.finish_node();
            return None;
        }
    }
    let close = parser.peek(false).span;
    parser.bump(false);
    parser.finish_node();
    if !named {
        parser.error(
            Span::new(open.start, close.end, open.byte_start, close.byte_end),
            "an import list needs at least one name: `import {} from …` imports nothing",
        );
    }
    Some(Form::Named)
}

/// `* as ns`.
fn namespace_import(parser: &mut Parser<'_>) -> Option<Form> {
    parser.start_node(NodeKind::NamespaceImport);
    parser.bump(false);
    if !parser.at_word("as") {
        let span = parser.peek(false).span;
        parser.error(span, "expected `as` after `*`");
        parser.finish_node();
        return None;
    }
    parser.bump(false);
    if parser.peek(false).kind != TokenKind::Ident {
        let span = parser.peek(false).span;
        parser.error(span, "expected a namespace name after `as`");
        parser.finish_node();
        return None;
    }
    parser.bump(false);
    parser.finish_node();
    Some(Form::Namespace)
}

/// `with { type: "json" }`, exactly: one `type` key whose value is the string `json`,
/// an optional trailing comma. True when well formed; a malformed clause is reported
/// here and not again.
fn import_attributes(parser: &mut Parser<'_>) -> bool {
    const ONLY: &str = "the only import attribute is `type: \"json\"` (SPEC.md §5.1)";
    parser.start_node(NodeKind::ImportAttributes);
    parser.bump(false);
    if !parser.eat(&LBRACE) {
        let span = parser.peek(false).span;
        parser.error(span, "expected `{` after `with`");
        parser.finish_node();
        return false;
    }
    let mut keys = 0;
    while parser.peek(false).kind != RBRACE {
        if !parser.at_word("type") {
            let span = parser.peek(false).span;
            parser.error(span, ONLY);
            parser.finish_node();
            return false;
        }
        let key = parser.peek(false).span;
        parser.bump(false);
        if !parser.eat(&COLON) {
            let span = parser.peek(false).span;
            parser.error(span, "expected `:` after `type`");
            parser.finish_node();
            return false;
        }
        let value = parser.peek(false).span;
        if !matches!(&parser.peek(false).kind, TokenKind::String(text) if &**text == "json") {
            parser.error(value, ONLY);
            parser.finish_node();
            return false;
        }
        parser.bump(false);
        keys += 1;
        if keys > 1 {
            parser.error(key, "the `type` attribute is given twice");
            parser.finish_node();
            return false;
        }
        if !parser.eat(&COMMA) && parser.peek(false).kind != RBRACE {
            let span = parser.peek(false).span;
            parser.error(span, "expected `,` or `}` after the attribute");
            parser.finish_node();
            return false;
        }
    }
    let close = parser.peek(false).span;
    parser.bump(false);
    parser.finish_node();
    if keys == 0 {
        parser.error(
            close,
            "`with { }` names no attribute: write `with { type: \"json\" }`",
        );
        return false;
    }
    true
}

/// The path and clause rules of SPEC.md §5.1, once the declaration has parsed.
/// `attributes` is `None` without a `with` clause, `Some(well_formed)` with one.
#[allow(clippy::case_sensitive_file_extension_comparisons)]
fn check(parser: &mut Parser<'_>, form: Form, path: &str, span: Span, attributes: Option<bool>) {
    if let Some(message) = foreign(path) {
        parser.report(Code::E201, span, message);
        return;
    }
    let last = path.rsplit('/').next().unwrap_or("");
    let json = last.ends_with(".json");
    match form {
        Form::Default if !json => parser.error(
            span,
            "a default import of a `.js` module is removed: use `import { name } from …` (SPEC.md §5.1)",
        ),
        Form::Default => {
            if attributes.is_none() {
                parser.report(
                    Code::E205,
                    span,
                    "a `.json` import needs `with { type: \"json\" }` (SPEC.md §5.1)",
                );
            }
        }
        Form::Named | Form::Namespace if json => parser.error(
            span,
            "a JSON module has no named exports: write `import cfg from \"…\" with { type: \"json\" }` (SPEC.md §5.1)",
        ),
        Form::Named | Form::Namespace => {
            if attributes.is_some() {
                parser.report(
                    Code::E205,
                    span,
                    "`with { type: \"json\" }` belongs on a `.json` import only (SPEC.md §5.1)",
                );
            }
        }
    }
}

/// Why `path` is not a path into the project (E201), if it is not.
#[allow(clippy::case_sensitive_file_extension_comparisons)]
pub(super) fn foreign(path: &str) -> Option<String> {
    if !path.starts_with("./") && !path.starts_with("../") {
        return Some(format!(
            "`{path}` is not a relative path: imports name a file in the project as `./…` or `../…` (SPEC.md §5.1)"
        ));
    }
    if path.contains('\\') {
        return Some(format!("`{path}` uses `\\`: paths are written with `/`"));
    }
    if path.contains("//") {
        return Some(format!("`{path}` has an empty path segment"));
    }
    let last = path.rsplit('/').next().unwrap_or("");
    if last.is_empty() || last.ends_with(".js") || last.ends_with(".json") {
        return None;
    }
    Some(format!(
        "`{path}` does not end in `.js` or `.json`: imports name the file as it is on disk (SPEC.md §5.1)"
    ))
}

#[cfg(test)]
mod tests {
    use super::foreign;
    use crate::parser::parse;
    use sjs_diag::{Code, Diagnostic, SourceFile};

    fn file(text: &str) -> SourceFile {
        SourceFile::new("a.js", text.as_bytes().to_vec()).expect("valid UTF-8")
    }

    fn sexp(text: &str) -> String {
        let f = file(text);
        parse(&f).0.to_sexp(&f)
    }

    /// `(code, line, column, message)` of every diagnostic.
    fn diagnostics(text: &str) -> Vec<(Code, u32, u32, String)> {
        parse(&file(text))
            .1
            .iter()
            .map(|d: &Diagnostic| {
                (
                    d.code,
                    d.span.start.line,
                    d.span.start.column,
                    d.message.clone(),
                )
            })
            .collect()
    }

    fn clean(text: &str) -> String {
        let found = diagnostics(text);
        assert_eq!(found, Vec::new(), "{text}");
        sexp(text)
    }

    #[test]
    fn the_three_forms_parse_into_their_nodes() {
        assert_eq!(
            clean("import { a, b as c } from \"./x.js\"\n"),
            "(Module (ImportDecl import (NamedImports { (ImportSpecifier a) , (ImportSpecifier b as c) }) from \"./x.js\") <eof>)"
        );
        assert_eq!(
            clean("import * as ns from \"../y.js\""),
            "(Module (ImportDecl import (NamespaceImport * as ns) from \"../y.js\") <eof>)"
        );
        assert_eq!(
            clean("import cfg from \"./c.json\" with { type: \"json\" }"),
            "(Module (ImportDecl import (DefaultImport cfg) from \"./c.json\" (ImportAttributes with { type : \"json\" })) <eof>)"
        );
        assert_eq!(
            clean("import { a, } from \"./x.js\""),
            "(Module (ImportDecl import (NamedImports { (ImportSpecifier a) , }) from \"./x.js\") <eof>)",
            "a trailing comma"
        );
        assert_eq!(
            clean("import cfg from \"./c.json\" with { type: \"json\", }"),
            "(Module (ImportDecl import (DefaultImport cfg) from \"./c.json\" (ImportAttributes with { type : \"json\" , })) <eof>)"
        );
        assert_eq!(
            clean("import { a } from \"./x.js\"\nimport * as b from \"./y.js\"\nconst c = a + b"),
            "(Module (ImportDecl import (NamedImports { (ImportSpecifier a) }) from \"./x.js\") (ImportDecl import (NamespaceImport * as b) from \"./y.js\") const c = a + b <eof>)",
            "several imports, then the items"
        );
        assert_eq!(
            clean("import { as as as } from \"./x.js\""),
            "(Module (ImportDecl import (NamedImports { (ImportSpecifier as as as) }) from \"./x.js\") <eof>)",
            "`as` and `from` are contextual"
        );
        assert_eq!(
            clean("import { from } from \"./x.js\""),
            "(Module (ImportDecl import (NamedImports { (ImportSpecifier from) }) from \"./x.js\") <eof>)"
        );
    }

    #[test]
    #[allow(clippy::case_sensitive_file_extension_comparisons)]
    fn edge_paths_are_accepted_and_left_to_resolution() {
        for path in [
            "./a.js/",
            "./",
            "././a.js",
            "../../a.json",
            "./.js",
            "./a.b.js",
            "./dir.js/x.js",
        ] {
            assert_eq!(foreign(path), None, "{path}");
            let text = if path.ends_with(".json") {
                format!("import c from \"{path}\" with {{ type: \"json\" }}")
            } else {
                format!("import {{ a }} from \"{path}\"")
            };
            assert_eq!(diagnostics(&text), Vec::new(), "{text}");
        }
    }

    #[test]
    fn foreign_paths_are_e201_on_the_string() {
        let cases = [
            ("lodash", "is not a relative path"),
            ("node:fs", "is not a relative path"),
            ("/abs/x.js", "is not a relative path"),
            ("https://x/y.js", "is not a relative path"),
            (".", "is not a relative path"),
            ("..", "is not a relative path"),
            (".a.js", "is not a relative path"),
            ("", "is not a relative path"),
            ("./x.mjs", "does not end in `.js` or `.json`"),
            ("./x.ts", "does not end in `.js` or `.json`"),
            ("./x", "does not end in `.js` or `.json`"),
            ("../x.jsx", "does not end in `.js` or `.json`"),
            (".//a.js", "has an empty path segment"),
            ("./a//b.js", "has an empty path segment"),
            ("./a\\b.js", "uses `\\`"),
            ("..\\a.js", "is not a relative path"),
        ];
        for (path, expected) in cases {
            let text = format!("import {{ a }} from \"{}\"", path.replace('\\', "\\\\"));
            let found = diagnostics(&text);
            assert_eq!(found.len(), 1, "{text}: {found:?}");
            assert_eq!(
                (found[0].0, found[0].1, found[0].2),
                (Code::E201, 1, 19),
                "{text}"
            );
            let message = &found[0].3;
            assert!(message.contains(expected), "{text}: {message}");
        }
        assert_eq!(
            diagnostics("import * as x from \"lodash\" with { type: \"json\" }").len(),
            1,
            "a foreign path reports nothing else"
        );
        assert_eq!(
            diagnostics("import x from \"lodash\"").len(),
            1,
            "a foreign default import reports the path only"
        );
    }

    #[test]
    fn the_extension_and_the_with_clause_must_agree() {
        let found = diagnostics("import { a } from \"./x.js\" with { type: \"json\" }");
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].0, found[0].2), (Code::E205, 19));
        assert!(found[0].3.contains("belongs on a `.json` import only"));
        let found = diagnostics("import * as a from \"./x.js\" with { type: \"json\" }");
        assert_eq!((found[0].0, found[0].2), (Code::E205, 20));
        let found = diagnostics("import cfg from \"./c.json\"");
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].0, found[0].2), (Code::E205, 17));
        assert!(found[0].3.contains("needs `with { type: \"json\" }`"));
        let found = diagnostics("import cfg from \"./c.json\" with { type: \"JSON\" }");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!((found[0].0, found[0].2), (Code::E000, 41));
        assert!(found[0].3.contains("the only import attribute"));
    }

    #[test]
    fn every_removed_import_form_is_e000_once() {
        let cases = [
            (
                "import x from \"./x.js\"",
                15,
                "a default import of a `.js` module is removed",
            ),
            ("import \"./side.js\"", 8, "a side-effect import"),
            ("import x, { y } from \"./x.js\"", 9, "expected `from`"),
            (
                "import { default as x } from \"./x.js\"",
                10,
                "`import { default as x }` is removed",
            ),
            ("import {} from \"./x.js\"", 8, "at least one name"),
            (
                "import { k } from \"./c.json\" with { type: \"json\" }",
                19,
                "a JSON module has no named exports",
            ),
            (
                "import * as k from \"./c.json\" with { type: \"json\" }",
                20,
                "a JSON module has no named exports",
            ),
        ];
        check_single_e000(&cases);
    }

    #[test]
    fn every_malformed_attribute_clause_is_e000_once() {
        let cases = [
            (
                "import cfg from \"./c.json\" with { type: \"json\", type: \"json\" }",
                49,
                "given twice",
            ),
            (
                "import cfg from \"./c.json\" with { kind: \"json\" }",
                35,
                "the only import attribute",
            ),
            (
                "import cfg from \"./c.json\" with { type: json }",
                41,
                "the only import attribute",
            ),
            (
                "import cfg from \"./c.json\" with { }",
                35,
                "names no attribute",
            ),
            (
                "import cfg from \"./c.json\" with type",
                33,
                "expected `{` after `with`",
            ),
            (
                "import cfg from \"./c.json\" with { type \"json\" }",
                40,
                "expected `:` after `type`",
            ),
            (
                "import cfg from \"./c.json\" with { type: \"json\" type: \"json\" }",
                48,
                "expected `,` or `}` after the attribute",
            ),
            (
                "import { a b } from \"./x.js\"",
                12,
                "expected `,` or `}` after an imported name",
            ),
            (
                "import { a as } from \"./x.js\"",
                15,
                "expected a name after `as`",
            ),
            (
                "import { 1 } from \"./x.js\"",
                10,
                "expected a name in the import list",
            ),
            (
                "import { class } from \"./x.js\"",
                10,
                "expected a name in the import list",
            ),
            ("import * from \"./x.js\"", 10, "expected `as` after `*`"),
            ("import * as from \"./x.js\"", 18, "expected `from`"),
            (
                "import * as 1 from \"./x.js\"",
                13,
                "expected a namespace name after `as`",
            ),
            ("import { a } \"./x.js\"", 14, "expected `from`"),
            ("import { a } from x", 19, "expected a module path string"),
            (
                "import = 1",
                8,
                "expected `{`, `*` or a name after `import`",
            ),
            ("import", 7, "expected `{`, `*` or a name after `import`"),
        ];
        check_single_e000(&cases);
    }

    /// Each text yields exactly one diagnostic: E000 on line 1 at `column`, whose
    /// message contains `expected`.
    fn check_single_e000(cases: &[(&str, u32, &str)]) {
        for (text, column, expected) in cases {
            let found = diagnostics(text);
            assert_eq!(found.len(), 1, "{text}: {found:?}");
            let message = &found[0].3;
            assert_eq!(
                (found[0].0, found[0].1, found[0].2),
                (Code::E000, 1, *column),
                "{text}: {message}"
            );
            assert!(message.contains(expected), "{text}: {message}");
        }
    }

    #[test]
    fn a_single_quoted_attribute_value_is_still_json() {
        let found = diagnostics("import cfg from './c.json' with { type: 'json' }");
        assert_eq!(found.len(), 2, "two S001s and nothing else: {found:?}");
        assert!(found.iter().all(|d| d.0 == Code::S001));
    }

    #[test]
    fn an_import_after_another_item_is_e000_and_still_parsed() {
        let text = "const a = 1\nimport { b } from \"./b.js\"\nimport * as c from \"lodash\"";
        let found = diagnostics(text);
        assert_eq!(found.len(), 3, "{found:?}");
        assert_eq!((found[0].0, found[0].1, found[0].2), (Code::E000, 2, 1));
        assert!(found[0]
            .3
            .contains("an import must precede every other module item"));
        assert_eq!((found[1].0, found[1].1, found[1].2), (Code::E000, 3, 1));
        assert_eq!(
            (found[2].0, found[2].1, found[2].2),
            (Code::E201, 3, 20),
            "the late import is still checked"
        );
        assert_eq!(
            sexp(text),
            "(Module const a = 1 (ImportDecl import (NamedImports { (ImportSpecifier b) }) from \"./b.js\") (ImportDecl import (NamespaceImport * as c) from \"lodash\") <eof>)"
        );
    }

    #[test]
    fn dynamic_imports_and_property_names_are_not_declarations() {
        assert_eq!(
            clean("const m = import(\"./x.js\")"),
            "(Module const m = import ( \"./x.js\" ) <eof>)"
        );
        assert_eq!(
            clean("import(\"./x.js\")"),
            "(Module import ( \"./x.js\" ) <eof>)",
            "even first"
        );
        assert_eq!(
            clean("a.import\na?.import"),
            "(Module a . import a ?. import <eof>)"
        );
        assert_eq!(
            clean("import { a } from \"./x.js\"\nx = a / 2 / 1"),
            "(Module (ImportDecl import (NamedImports { (ImportSpecifier a) }) from \"./x.js\") x = a / 2 / 1 <eof>)",
            "the items keep the regex guess"
        );
    }

    #[test]
    fn a_malformed_import_leaves_the_rest_of_the_line_to_the_items() {
        assert_eq!(
            sexp("import { a } \"./x.js\"\nconst b = 1"),
            "(Module (ImportDecl import (NamedImports { (ImportSpecifier a) })) \"./x.js\" const b = 1 <eof>)"
        );
        assert_eq!(
            sexp("import \"./x.js\"\nimport { a } from \"./y.js\""),
            "(Module (ImportDecl import) \"./x.js\" (ImportDecl import (NamedImports { (ImportSpecifier a) }) from \"./y.js\") <eof>)",
            "a later import is still an import, reported as out of order"
        );
        assert_eq!(
            diagnostics("import \"./x.js\"\nimport { a } from \"./y.js\"").len(),
            2
        );
    }

    #[test]
    fn imports_print_back_to_their_source() {
        let text = "import   { a ,b as   c, } from \"./x.js\" // c\n\nimport cfg from './c.json' with { type: \"json\" }\n";
        let f = file(text);
        let (tree, _) = parse(&f);
        assert_eq!(tree.print(&f), text);
        assert_eq!(tree.root().children().count(), 3, "two imports and Eof");
    }
}
