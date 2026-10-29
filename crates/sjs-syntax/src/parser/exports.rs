//! Export declarations (SPEC.md §2 `ExportDecl`, §5.1, §8.4): `export` before a
//! declaration, `export { a, b as c }` and `export { a } from "./x.js"`. Named exports
//! only: every `export default` form is E202, `export *` is E161, and a name exported
//! twice is E000 (an ES2025 early error). The declaration after `export` is parsed by
//! its own module, inside the `ExportDecl` node.

use std::collections::BTreeMap;

use sjs_diag::{Code, Span};

use super::imports::foreign;
use super::{bindings, classes, functions, Parser};
use crate::token::{Keyword, Punct, TokenKind};
use crate::tree::NodeKind;

const LBRACE: TokenKind = TokenKind::Punct(Punct::LBrace);
const RBRACE: TokenKind = TokenKind::Punct(Punct::RBrace);
const COMMA: TokenKind = TokenKind::Punct(Punct::Comma);

/// The names a module has exported so far, each with where it was first exported.
pub(super) type Exported = BTreeMap<String, Span>;

/// One `export …` declaration, at the `export` keyword. `exported` collects the names
/// so a second export of one is E000 at the repeat.
pub(super) fn export_decl(parser: &mut Parser<'_>, exported: &mut Exported) {
    parser.start_node(NodeKind::ExportDecl);
    parser.bump(false);
    match parser.peek(false).kind.clone() {
        LBRACE => {
            named_exports(parser, exported);
            parser.terminate();
        }
        TokenKind::Keyword(Keyword::Default) => {
            let span = parser.peek(false).span;
            parser.report(
                Code::E202,
                span,
                "`export default` is removed: a module has named exports only, so every importer uses the same name (SPEC.md §5.1)",
            );
            // Recovery: a default-exported function parses as the expression it is in
            // ES2025, without a second report; anything else is left to the items.
            parser.bump(false);
            if functions::at_function(parser) {
                functions::function(parser, true);
            }
        }
        TokenKind::Punct(Punct::Star) => {
            let span = parser.peek(false).span;
            parser.report(
                Code::E161,
                span,
                "`export *` re-exports an open set of names: name each export (SPEC.md §8.4)",
            );
        }
        TokenKind::Keyword(Keyword::Const | Keyword::Let) => {
            for (name, span) in bindings::declaration(parser) {
                note_export(parser, exported, name, span);
            }
        }
        TokenKind::Keyword(Keyword::Function) => {
            if let Some((name, span)) = functions::function(parser, false) {
                note_export(parser, exported, name, span);
            }
        }
        TokenKind::Keyword(Keyword::Class) => {
            if let Some((name, span)) = classes::class_decl(parser, false) {
                note_export(parser, exported, name, span);
            }
        }
        TokenKind::Ident if parser.at_word("async") => {
            if parser.peek_second().kind == TokenKind::Keyword(Keyword::Function) {
                if let Some((name, span)) = functions::function(parser, false) {
                    note_export(parser, exported, name, span);
                }
            } else {
                let span = parser.peek_second().span;
                parser.error(span, "expected `function` after `async`");
            }
        }
        _ => {
            let span = parser.peek(false).span;
            parser.error(
                span,
                "expected a declaration (`const`, `let`, `function`, `class`) or `{` after `export`",
            );
        }
    }
    parser.finish_node();
}

/// `{ a, b as c, }` with an optional `from "…"`.
fn named_exports(parser: &mut Parser<'_>, exported: &mut Exported) {
    parser.start_node(NodeKind::NamedExports);
    let open = parser.peek(false).span;
    parser.bump(false);
    let mut names: Vec<(String, Span)> = Vec::new();
    loop {
        match parser.peek(false).kind.clone() {
            RBRACE => break,
            TokenKind::Ident => {
                parser.start_node(NodeKind::ExportSpecifier);
                let local = parser.bump(false).clone();
                let mut exported_as = (parser.text(&local).to_owned(), local.span);
                if parser.at_word("as") {
                    parser.bump(false);
                    match parser.peek(false).kind.clone() {
                        TokenKind::Ident => {
                            let target = parser.bump(false).clone();
                            exported_as = (parser.text(&target).to_owned(), target.span);
                        }
                        TokenKind::Keyword(Keyword::Default) => {
                            let span = parser.peek(false).span;
                            parser.report(
                                Code::E202,
                                span,
                                "`export { x as default }` is a default export, which is removed: a module has named exports only (SPEC.md §5.1)",
                            );
                            parser.bump(false);
                        }
                        _ => {
                            let span = parser.peek(false).span;
                            parser.error(span, "expected a name after `as`");
                            parser.finish_node();
                            parser.finish_node();
                            return;
                        }
                    }
                }
                parser.finish_node();
                names.push(exported_as);
            }
            _ => {
                let span = parser.peek(false).span;
                parser.error(span, "expected a name in the export list");
                parser.finish_node();
                return;
            }
        }
        if !parser.eat(&COMMA) && parser.peek(false).kind != RBRACE {
            let span = parser.peek(false).span;
            parser.error(span, "expected `,` or `}` after an exported name");
            parser.finish_node();
            return;
        }
    }
    let close = parser.peek(false).span;
    parser.bump(false);
    parser.finish_node();
    if names.is_empty() {
        parser.error(
            Span::new(open.start, close.end, open.byte_start, close.byte_end),
            "an export list needs at least one name: `export {}` exports nothing",
        );
    }
    for (name, span) in names {
        note_export(parser, exported, name, span);
    }
    if parser.at_word("from") {
        parser.bump(false);
        re_export_path(parser);
    }
}

/// The `"…"` after `from` of a re-export, checked like an import path.
#[allow(clippy::case_sensitive_file_extension_comparisons)]
fn re_export_path(parser: &mut Parser<'_>) {
    let TokenKind::String(path) = parser.peek(false).kind.clone() else {
        let span = parser.peek(false).span;
        parser.error(span, "expected a module path string after `from`");
        return;
    };
    let span = parser.peek(false).span;
    parser.bump(false);
    if let Some(message) = foreign(&path) {
        parser.report(Code::E201, span, message);
    } else if path.rsplit('/').next().unwrap_or("").ends_with(".json") {
        parser.error(
            span,
            "a JSON module has no named exports to re-export (SPEC.md §5.1)",
        );
    }
}

/// Records `name` as exported at `span`; a second export of the same name is E000.
fn note_export(parser: &mut Parser<'_>, exported: &mut Exported, name: String, span: Span) {
    if let Some(first) = exported.get(&name) {
        let message = format!(
            "`{name}` is exported twice; the first export is at {}:{}",
            first.start.line, first.start.column
        );
        parser.error(span, message);
        return;
    }
    exported.insert(name, span);
}

#[cfg(test)]
mod tests {
    use crate::parser::parse;
    use sjs_diag::{Code, Diagnostic, SourceFile};

    fn file(text: &str) -> SourceFile {
        SourceFile::new("a.js", text.as_bytes().to_vec()).expect("valid UTF-8")
    }

    fn sexp(text: &str) -> String {
        let f = file(text);
        parse(&f).0.to_sexp(&f)
    }

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
    fn exported_declarations_wrap_the_keyword_and_leave_the_declaration_to_its_story() {
        assert_eq!(
            clean("export const a = 1"),
            "(Module (ExportDecl export (ConstDecl const a (Initializer = 1))) <eof>)"
        );
        assert_eq!(
            clean("export let a = 1"),
            "(Module (ExportDecl export (LetDecl let a (Initializer = 1))) <eof>)"
        );
        assert_eq!(
            clean("export function f() {}"),
            "(Module (ExportDecl export (FunctionDecl function f (Params ( )) (Block { }))) <eof>)"
        );
        assert_eq!(
            clean("export class C {}"),
            "(Module (ExportDecl export (ClassDecl class C (ClassBody { }))) <eof>)"
        );
        assert_eq!(
            clean("export async function f() {}"),
            "(Module (ExportDecl export (FunctionDecl async function f (Params ( )) (Block { }))) <eof>)"
        );
        assert_eq!(
            clean("export function* g() {}"),
            "(Module (ExportDecl export (FunctionDecl function * g (Params ( )) (Block { }))) <eof>)"
        );
        assert_eq!(
            clean("export const { a, b } = c"),
            "(Module (ExportDecl export (ConstDecl const (ObjectPattern { (PatternProperty a) , (PatternProperty b) }) (Initializer = c))) <eof>)",
            "a pattern's names are tracked too"
        );
    }

    #[test]
    fn export_lists_parse_with_renames_trailing_commas_and_re_exports() {
        assert_eq!(
            clean("export { a, b as c }"),
            "(Module (ExportDecl export (NamedExports { (ExportSpecifier a) , (ExportSpecifier b as c) })) <eof>)"
        );
        assert_eq!(
            clean("export { a, }"),
            "(Module (ExportDecl export (NamedExports { (ExportSpecifier a) , })) <eof>)"
        );
        assert_eq!(
            clean("export { a } from \"./x.js\""),
            "(Module (ExportDecl export (NamedExports { (ExportSpecifier a) }) from \"./x.js\") <eof>)"
        );
        assert_eq!(
            clean("export { a as b } from \"../x.js\"\nexport { a }"),
            "(Module (ExportDecl export (NamedExports { (ExportSpecifier a as b) }) from \"../x.js\") (ExportDecl export (NamedExports { (ExportSpecifier a) })) <eof>)",
            "a re-exported `a as b` leaves `a` free"
        );
        assert_eq!(
            clean("import { a } from \"./a.js\"\nexport { a as default_ }"),
            "(Module (ImportDecl import (NamedImports { (ImportSpecifier a) }) from \"./a.js\") (ExportDecl export (NamedExports { (ExportSpecifier a as default_) })) <eof>)"
        );
    }

    #[test]
    fn every_default_export_form_is_e202() {
        for (text, column) in [
            ("export default 1", 8),
            ("export default function f() {}", 8),
            ("export default class C {}", 8),
            ("export default async function () {}", 8),
            ("export { x as default }", 15),
            ("const x = 1\nexport default x", 8),
        ] {
            let found = diagnostics(text);
            assert_eq!(found.len(), 1, "{text}: {found:?}");
            let line = text.lines().count();
            assert_eq!(found[0].0, Code::E202, "{text}");
            assert_eq!(
                (found[0].1, found[0].2),
                (u32::try_from(line).expect("small"), column),
                "{text}"
            );
            assert!(found[0].3.contains("named exports only"), "{text}");
        }
        assert_eq!(
            sexp("export default 1"),
            "(Module (ExportDecl export default) 1 <eof>)",
            "the rest of the line is left to the items"
        );
        assert_eq!(
            sexp("export { x as default }"),
            "(Module (ExportDecl export (NamedExports { (ExportSpecifier x as default) })) <eof>)"
        );
    }

    #[test]
    fn an_unnamed_class_export_is_e000_once() {
        assert_eq!(diagnostics("export class {}").len(), 1);
    }

    #[test]
    fn export_star_is_e161() {
        for text in ["export * from \"./x.js\"", "export * as ns from \"./x.js\""] {
            let found = diagnostics(text);
            assert_eq!(found.len(), 1, "{text}: {found:?}");
            assert_eq!(
                (found[0].0, found[0].1, found[0].2),
                (Code::E161, 1, 8),
                "{text}"
            );
            assert!(found[0].3.contains("name each export"), "{text}");
        }
        assert_eq!(
            sexp("export * from \"./x.js\""),
            "(Module (ExportDecl export) * from \"./x.js\" <eof>)"
        );
    }

    #[test]
    fn a_name_exported_twice_is_e000_at_the_repeat() {
        let cases = [
            ("export { a, a }", 1, 13),
            ("export { a }\nexport { a }", 2, 10),
            ("export const a = 1\nexport { a }", 2, 10),
            ("export { b as a }\nexport function a() {}", 2, 17),
            ("export class a {}\nexport let a = 1", 2, 12),
            ("export const { a } = x\nexport { a }", 2, 10),
            ("export const [b, { c: a }] = x\nexport { a }", 2, 10),
            ("export async function a() {}\nexport { a }", 2, 10),
            ("export { a } from \"./x.js\"\nexport { a }", 2, 10),
        ];
        for (text, line, column) in cases {
            let found = diagnostics(text);
            assert_eq!(found.len(), 1, "{text}: {found:?}");
            assert_eq!(
                (found[0].0, found[0].1, found[0].2),
                (Code::E000, line, column),
                "{text}"
            );
            let message = &found[0].3;
            assert!(
                message.contains("`a` is exported twice; the first export is at 1:"),
                "{text}: {message}"
            );
        }
        assert_eq!(
            diagnostics("export { a as b }\nexport { b as a }"),
            Vec::new(),
            "the exported names differ"
        );
        assert_eq!(
            diagnostics(
                "export function* a() {}
export { a }"
            )
            .len(),
            1,
            "a generator's name is tracked too"
        );
    }

    #[test]
    fn malformed_exports_are_e000_once() {
        let cases = [
            ("export {}", 8, "an export list needs at least one name"),
            ("export { }", 8, "an export list needs at least one name"),
            (
                "export { a b }",
                12,
                "expected `,` or `}` after an exported name",
            ),
            ("export { a as }", 15, "expected a name after `as`"),
            ("export { 1 }", 10, "expected a name in the export list"),
            (
                "export { a } from x",
                19,
                "expected a module path string after `from`",
            ),
            (
                "export { a } from \"./c.json\"",
                19,
                "a JSON module has no named exports to re-export",
            ),
            ("export 1", 8, "expected a declaration"),
            ("export x = 1", 8, "expected a declaration"),
            ("export asyncx = 1", 8, "expected a declaration"),
            (
                "export function () {}",
                17,
                "a `function` declaration needs a name",
            ),
            (
                "export async function () {}",
                23,
                "a `function` declaration needs a name",
            ),
            ("export", 7, "expected a declaration"),
            ("export async x", 14, "expected `function` after `async`"),
        ];
        for (text, column, expected) in cases {
            let found = diagnostics(text);
            assert_eq!(found.len(), 1, "{text}: {found:?}");
            let message = &found[0].3;
            assert_eq!(
                (found[0].0, found[0].1, found[0].2),
                (Code::E000, 1, column),
                "{text}: {message}"
            );
            assert!(message.contains(expected), "{text}: {message}");
        }
        let found = diagnostics("export { a } from \"lodash\"");
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].0, found[0].2), (Code::E201, 19));
        assert_eq!(
            sexp("export { a b }\nexport { c }"),
            "(Module (ExportDecl export (NamedExports { (ExportSpecifier a))) b } (ExportDecl export (NamedExports { (ExportSpecifier c) })) <eof>)",
            "the list ends at the error and the rest is left to the items"
        );
    }

    #[test]
    fn an_export_inside_a_block_is_e000_and_still_parsed() {
        let text = "function f() {\n  export const a = 1\n}\nexport { b }";
        let found = diagnostics(text);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!((found[0].0, found[0].1, found[0].2), (Code::E000, 2, 3));
        let message = &found[0].3;
        assert!(message.contains("top level"), "{message}");
        assert_eq!(
            sexp(text),
            "(Module (FunctionDecl function f (Params ( )) (Block { (ExportDecl export (ConstDecl const a (Initializer = 1))) })) (ExportDecl export (NamedExports { (ExportSpecifier b) })) <eof>)"
        );
        assert_eq!(
            diagnostics(
                "const o = { export: 1 }\nconst p = o.export\nconst q = `${ {}.x }`\nexport { p }"
            ),
            Vec::new(),
            "`export` as a property name and braces inside templates do not count"
        );
        assert_eq!(diagnostics("{ export { a } }").len(), 1, "a bare block");
        assert_eq!(
            diagnostics("} export { a }"),
            vec![(Code::E000, 1, 1, "unmatched `}`".to_owned())],
            "a stray `}}` is reported and never puts the module inside a block"
        );
    }

    #[test]
    fn exports_print_back_to_their_source() {
        let text = "export   { a ,b as   c, } from \"./x.js\" // c\n\nexport const d = 1\n";
        let f = file(text);
        let (tree, found) = parse(&f);
        assert!(found.is_empty(), "{found:?}");
        assert_eq!(tree.print(&f), text);
    }
}
