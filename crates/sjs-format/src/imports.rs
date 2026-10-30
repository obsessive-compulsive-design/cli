//! Import order (US-045, SPEC.md §9.1). A run of imports is sorted by the number of
//! `../` segments in the path, most first, then by the path's bytes; the names of an
//! import list are sorted by the imported name's bytes. Equal keys keep their order, so
//! two imports from one path stay two. The comment at the end of an import's line
//! moves with it, and so do the comments right above it, below any blank line; the
//! lines above the first import of a run are the file's header and stay.
//! `Formatter::import_order()` reports S003 once per run out of order, with the run as
//! written and the run in order.

use std::cmp::Reverse;
use std::ops::Range;

use sjs_diag::{Code, Diagnostic};
use sjs_syntax::{Element, NodeKind, NodeRef, TokenKind};

use crate::doc::WIDTH;
use crate::expr::Formatter;
use crate::stmt::{items_of, joined, Above, Body, Item};

const MESSAGE: &str = "imports out of order: by depth (`../` segments, most first) then by path, \
                       names by the imported name (SPEC.md §9.1)";

impl<'a> Formatter<'a> {
    /// S003 for every run of imports out of order: one per run, the run as written being
    /// the current text and the run in order the replacement.
    #[must_use]
    pub fn import_order(&self) -> Vec<Diagnostic> {
        let items = items_of(self.tree.root(), true);
        import_runs(&items)
            .into_iter()
            .filter_map(|run| self.run_finding(&items[run]))
            .collect()
    }

    /// The S003 of one run of imports, when its imports or their names are out of order.
    /// The text runs from the first import to the end of the last import's line comment.
    fn run_finding(&self, run: &[Item<'_>]) -> Option<Diagnostic> {
        let imports: Vec<NodeRef<'_>> = run.iter().filter_map(import_of).collect();
        let mut sorted = run.to_vec();
        sort_run(&mut sorted);
        let moved = sorted
            .iter()
            .zip(run)
            .any(|(after, before)| after.body != before.body)
            || imports.iter().any(|decl| self.names_moved(*decl));
        if !moved {
            return None;
        }
        let start = imports[0].span().byte_start;
        let end = run[run.len() - 1]
            .trailing
            .last()
            .map_or(imports[imports.len() - 1].span().byte_end, |span| {
                span.byte_end
            });
        let current = self.file.text()[start..end].to_owned();
        let replacement = self.run_text(sorted);
        self.file.span(start, end).and_then(|span| {
            Diagnostic::new(Code::S003, self.file.path(), span, MESSAGE)
                .with_fix(current, replacement)
                .ok()
        })
    }

    /// A sorted run as text, without the header above it.
    fn run_text(&self, mut sorted: Vec<Item<'_>>) -> String {
        sorted[0].above.clear();
        joined(self.lines(&sorted, true), WIDTH)
    }

    /// Whether an import's list of names is not in order.
    fn names_moved(&self, decl: NodeRef<'_>) -> bool {
        decl.children().any(|child| {
            matches!(child, Element::Node(list) if list.kind() == NodeKind::NamedImports
                && self.sorted_specifiers(list) != list.children().collect::<Vec<_>>())
        })
    }

    /// The children of an import list with its names in order by the imported name's
    /// bytes, the braces and commas where they were.
    pub(crate) fn sorted_specifiers<'t>(&self, list: NodeRef<'t>) -> Vec<Element<'t>> {
        let mut children: Vec<Element<'t>> = list.children().collect();
        let mut specifiers = Vec::new();
        let mut positions = Vec::new();
        for (at, child) in children.iter().enumerate() {
            if let Element::Node(specifier) = *child {
                specifiers.push(specifier);
                positions.push(at);
            }
        }
        specifiers.sort_by_key(|specifier| self.imported_name(*specifier));
        for (at, specifier) in positions.into_iter().zip(specifiers) {
            children[at] = Element::Node(specifier);
        }
        children
    }

    /// The name an import specifier imports: its first word (`a` in `a as b`).
    fn imported_name(&self, specifier: NodeRef<'_>) -> &'a str {
        specifier
            .tokens()
            .first()
            .map_or("", |token| self.text(token.span))
    }
}

/// Every run of imports among `items` in order; imports only ever stand among the file's
/// items.
pub(crate) fn sort_imports(items: &mut [Item<'_>]) {
    for run in import_runs(items) {
        sort_run(&mut items[run]);
    }
}

/// The maximal runs of imports among `items`, as ranges of positions.
fn import_runs(items: &[Item<'_>]) -> Vec<Range<usize>> {
    let mut runs: Vec<Range<usize>> = Vec::new();
    for (at, item) in items.iter().enumerate() {
        if import_of(item).is_none() {
            continue;
        }
        if let Some(run) = runs.last_mut().filter(|run| run.end == at) {
            run.end = at + 1;
        } else {
            runs.push(at..at + 1);
        }
    }
    runs
}

/// The import an item is, if it is one.
fn import_of<'t>(item: &Item<'t>) -> Option<NodeRef<'t>> {
    match item.body {
        Body::Node(node) if node.kind() == NodeKind::ImportDecl => Some(node),
        _ => None,
    }
}

/// One run of imports sorted: each import moves with its attached comments and its
/// line comment, the lines kept at each position staying there.
fn sort_run(run: &mut [Item<'_>]) {
    let imports: Vec<NodeRef<'_>> = run.iter().filter_map(import_of).collect();
    let mut order: Vec<usize> = (0..imports.len()).collect();
    order.sort_by_key(|&at| import_key(imports[at]));
    let moved: Vec<Item<'_>> = order
        .iter()
        .map(|&at| Item {
            body: run[at].body,
            above: run[at].above[kept_len(&run[at].above, at)..].to_vec(),
            trailing: run[at].trailing.clone(),
        })
        .collect();
    for (at, (item, moved)) in run.iter_mut().zip(moved).enumerate() {
        item.above.truncate(kept_len(&item.above, at));
        item.above.extend(moved.above);
        item.body = moved.body;
        item.trailing = moved.trailing;
    }
}

/// What orders an import: the number of `../` segments in its path, most first, then
/// the path's bytes.
fn import_key(decl: NodeRef<'_>) -> (Reverse<usize>, &str) {
    let path = path_of(decl);
    let depth = path.split('/').filter(|segment| *segment == "..").count();
    (Reverse(depth), path)
}

/// The path an import names, as the string's value; empty when the declaration has
/// none (E000).
fn path_of(decl: NodeRef<'_>) -> &str {
    decl.children()
        .find_map(|child| match child {
            Element::Token(token) => match &token.token().kind {
                TokenKind::String(path) => Some(&**path),
                _ => None,
            },
            Element::Node(_) => None,
        })
        .unwrap_or("")
}

/// How many of the lines above the import at position `at` of its run stay where they
/// are when it moves: all of them above the first import, the file's header; those up
/// to and including the last blank line above any other, what follows being the
/// import's own comments.
fn kept_len(above: &[Above], at: usize) -> usize {
    if at == 0 {
        return above.len();
    }
    above
        .iter()
        .rposition(|line| *line == Above::Blank)
        .map_or(0, |blank| blank + 1)
}

#[cfg(test)]
mod tests {
    use super::Formatter;
    use sjs_diag::{Code, SourceFile};
    use sjs_syntax::parse;

    /// A finding: its code, line, column and fix.
    type Found = (Code, u32, u32, Option<(String, String)>);

    fn found(text: &str) -> Vec<Found> {
        let f = SourceFile::new("a.js", text.as_bytes().to_vec()).expect("utf-8");
        let (tree, _) = parse(&f);
        Formatter::new(&f, &tree)
            .import_order()
            .into_iter()
            .map(|d| {
                let fix = d.current.clone().zip(d.replacement.clone());
                (d.code, d.span.start.line, d.span.start.column, fix)
            })
            .collect()
    }

    fn module(text: &str) -> String {
        let f = SourceFile::new("a.js", text.as_bytes().to_vec()).expect("utf-8");
        let (tree, _) = parse(&f);
        Formatter::new(&f, &tree).module()
    }

    fn fix(current: &str, replacement: &str) -> (String, String) {
        (current.to_owned(), replacement.to_owned())
    }

    #[test]
    fn imports_sort_by_depth_then_by_path() {
        assert_eq!(
            module("import { a } from './a.js'\nimport { b } from './a/b.js'\nimport { c } from '../a.js'\nimport { d } from '../../d.js'\nimport { e } from './../e.js'\nuse(a, b, c, d, e)"),
            "import { d } from \"../../d.js\"\nimport { c } from \"../a.js\"\nimport { e } from \"./../e.js\"\nimport { a } from \"./a.js\"\nimport { b } from \"./a/b.js\"\n\nuse(a, b, c, d, e)\n"
        );
        assert_eq!(
            module("import d from \"./d.json\" with { type: \"json\" }\nimport * as c from \"./c.js\"\nimport { e } from \"./e.js\""),
            "import * as c from \"./c.js\"\nimport d from \"./d.json\" with { type: \"json\" }\nimport { e } from \"./e.js\"\n",
            "a JSON import and a namespace import sort by their paths like the others"
        );
        assert_eq!(
            module("import { b } from \"./a.js\"\nimport { a } from \"./a.js\""),
            "import { b } from \"./a.js\"\nimport { a } from \"./a.js\"\n",
            "two imports from one path keep their order"
        );
        assert_eq!(
            module("import { b } from \"./b.js\"\nimport x\nimport { a } from \"./a.js\""),
            "import x\nimport { a } from \"./a.js\"\nimport { b } from \"./b.js\"\n",
            "an import without a path (E000) sorts as the empty path"
        );
        assert_eq!(
            module("const n = 1\nimport { b } from \"./b.js\"\nimport { a } from \"./a.js\""),
            "const n = 1\n\nimport { a } from \"./a.js\"\nimport { b } from \"./b.js\"\n",
            "imports after an item (E000) are a run of their own"
        );
    }

    #[test]
    fn names_sort_by_the_imported_name() {
        assert_eq!(
            module("import { b as a, a as b, c } from \"./x.js\""),
            "import { a as b, b as a, c } from \"./x.js\"\n"
        );
        assert_eq!(
            module("import { c, a, b, } from \"./x.js\""),
            "import { a, b, c } from \"./x.js\"\n"
        );
        assert_eq!(
            module("import { bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb as c, aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa as d } from \"./x.js\""),
            "import {\n  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa as d,\n  bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb as c,\n} from \"./x.js\"\n",
            "a broken list is sorted too"
        );
        assert_eq!(
            module("export { b, a }\nexport { b as a, a as b } from \"./x.js\""),
            "export { b, a }\n\nexport { b as a, a as b } from \"./x.js\"\n",
            "an export list is not an import list"
        );
    }

    #[test]
    fn comments_attached_to_an_import_move_with_it() {
        assert_eq!(
            module("import { c } from \"./c.js\"\n// about b\nimport { b } from \"./b.js\" // b's line\nimport { a } from \"./a.js\""),
            "import { a } from \"./a.js\"\n// about b\nimport { b } from \"./b.js\"  // b's line\nimport { c } from \"./c.js\"\n"
        );
        assert_eq!(
            module("// header\nimport { b } from \"./b.js\"\nimport { a } from \"./a.js\""),
            "// header\nimport { a } from \"./a.js\"\nimport { b } from \"./b.js\"\n",
            "the lines above the first import are the file's header and stay"
        );
        assert_eq!(
            module("// header\n\n// about b\nimport { b } from \"./b.js\"\nimport { a } from \"./a.js\""),
            "// header\n\n// about b\nimport { a } from \"./a.js\"\nimport { b } from \"./b.js\"\n",
            "whatever stands in them"
        );
        assert_eq!(
            module("import { b } from \"./b.js\"\n\n// group two\nimport { a } from \"./a.js\"\nimport { c } from \"./c.js\"\n// end\n"),
            "// group two\nimport { a } from \"./a.js\"\nimport { b } from \"./b.js\"\nimport { c } from \"./c.js\"\n// end\n",
            "a comment under a blank line is attached to the import below it"
        );
        assert_eq!(
            module("import { b } from \"./b.js\"\n// detached\n\nimport { a } from \"./a.js\""),
            "import { a } from \"./a.js\"\n// detached\n\nimport { b } from \"./b.js\"\n",
            "a comment above a blank line stays at its position"
        );
        assert_eq!(
            module("// about a\nimport { a } from \"./a.js\"\nimport { b } from \"./b.js\" // b\n"),
            "// about a\nimport { a } from \"./a.js\"\nimport { b } from \"./b.js\"  // b\n",
            "in order, nothing moves"
        );
    }

    #[test]
    fn s003_names_a_run_out_of_order_with_its_order() {
        assert_eq!(
            found("import { b } from \"./b.js\"\nimport { a } from \"./a.js\"\nuse(a, b)\n"),
            vec![(
                Code::S003,
                1,
                1,
                Some(fix(
                    "import { b } from \"./b.js\"\nimport { a } from \"./a.js\"",
                    "import { a } from \"./a.js\"\nimport { b } from \"./b.js\""
                ))
            )]
        );
        assert_eq!(
            found("import { b, a } from \"./x.js\""),
            vec![(
                Code::S003,
                1,
                1,
                Some(fix(
                    "import { b, a } from \"./x.js\"",
                    "import { a, b } from \"./x.js\""
                ))
            )],
            "names out of order in a run of one"
        );
        assert_eq!(
            found("// header\nimport { c } from './c.js' // c's line\n// about b\nimport { b } from './b.js' // b's line\nimport { a } from './a.js'"),
            vec![(
                Code::S003,
                2,
                1,
                Some(fix(
                    "import { c } from './c.js' // c's line\n// about b\nimport { b } from './b.js' // b's line\nimport { a } from './a.js'",
                    "import { a } from \"./a.js\"\n// about b\nimport { b } from \"./b.js\"  // b's line\nimport { c } from \"./c.js\"  // c's line"
                ))
            )],
            "the text starts under the header and carries the comments that move"
        );
        assert_eq!(
            found("import { a } from \"./a.js\"\nimport { b } from \"./b.js\" // last\nconst n = 1\nimport { d } from \"./d.js\"\nimport { c } from \"./c.js\" // c\n"),
            vec![(
                Code::S003,
                4,
                1,
                Some(fix(
                    "import { d } from \"./d.js\"\nimport { c } from \"./c.js\" // c",
                    "import { c } from \"./c.js\"  // c\nimport { d } from \"./d.js\""
                ))
            )],
            "one per run; the text ends with the last import's line comment"
        );
    }

    #[test]
    fn runs_in_order_produce_nothing() {
        for text in [
            "import { a } from \"../a.js\"\nimport { b } from \"./b.js\"\nimport { c } from \"./b/c.js\"\n",
            "import { a, b } from \"./a.js\"\nimport { a as x } from \"./a.js\"\n",
            "import { a as b, b as a } from \"./a.js\"",
            "const n = 1",
            "",
        ] {
            assert_eq!(found(text), Vec::new(), "{text}");
        }
    }
}
