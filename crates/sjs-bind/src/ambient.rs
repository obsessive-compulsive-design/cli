//! The ambient table (SPEC.md §5.2, §6, §10.1): the globals of `ambient/globals.toml`
//! and what a use of a name the file does not declare is found to be. A program is
//! checked against the intersection of the five runtimes, always: a global or member
//! any one of them lacks is E204, one the language removed is E133, and a global is
//! assigned in a `.test.js` file only (E138).

use sjs_diag::Code;
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// The runtimes a program runs on: the word the table uses, and the name findings use.
pub const RUNTIMES: [(&str, &str); 5] = [
    ("browser", "browsers"),
    ("node", "Node 24"),
    ("deno", "Deno 2"),
    ("bun", "Bun"),
    ("workerd", "Cloudflare Workers"),
];

/// One global of the table.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Global {
    /// Its name.
    pub name: String,
    /// The section of SPEC.md the row transcribes.
    pub spec: String,
    /// Where it is specified.
    pub source: String,
    /// The runtimes that have it, in the table's words.
    pub present: Vec<String>,
    /// The members the language keeps.
    pub members: Vec<String>,
    /// Whether every member is kept.
    pub open: bool,
    /// When the global is removed: its replacement, `none`, or nothing.
    pub removed: Option<String>,
    /// When it is used through its members only: the replacement for any other use.
    pub members_only: Option<String>,
    /// Whether only a `.test.js` file has it.
    pub test: bool,
    /// Whether the lexer refuses the name as a word (E000) before it is looked up.
    pub syntax: bool,
    /// The removed members, each with its replacement.
    pub removed_members: BTreeMap<String, String>,
    /// The kept members some runtime lacks, each with the runtimes that have it.
    pub member_present: BTreeMap<String, Vec<String>>,
}

/// The names of the runtimes that are not among `present`.
fn lacking(present: &[String]) -> Vec<&'static str> {
    let has = |word: &str| present.iter().any(|runtime| runtime == word);
    let lacking = RUNTIMES.iter().filter(|(word, _)| !has(word));
    lacking.map(|(_, name)| *name).collect()
}

impl Global {
    /// The runtimes that lack the global, by name.
    #[must_use]
    pub fn missing(&self) -> Vec<&'static str> {
        lacking(&self.present)
    }

    /// The runtimes that lack the kept member `member`, by name.
    #[must_use]
    pub fn member_missing(&self, member: &str) -> Vec<&'static str> {
        let present = self.member_present.get(member);
        present.map_or(Vec::new(), |present| lacking(present))
    }
}

/// The globals, in the order of the file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Table {
    globals: Vec<Global>,
    index: BTreeMap<String, usize>,
}

/// What a `[[global]]` of the file holds, as read.
#[derive(Default)]
struct Row {
    texts: BTreeMap<String, String>,
    flags: BTreeMap<String, bool>,
    lists: BTreeMap<String, Vec<String>>,
    removed: BTreeMap<String, String>,
    present: BTreeMap<String, Vec<String>>,
}

enum Value {
    Text(String),
    Flag(bool),
    List(Vec<String>),
}

#[derive(Clone, Copy)]
enum Section {
    Global,
    Removed,
    Present,
}

/// `line` without its comment: a `#` outside a string begins one.
fn code(line: &str) -> &str {
    let mut quoted = false;
    let mut escaped = false;
    for (at, c) in line.char_indices() {
        match c {
            '\\' if quoted && !escaped => {
                escaped = true;
                continue;
            }
            '"' if !escaped => quoted = !quoted,
            '#' if !quoted => return &line[..at],
            _ => {}
        }
        escaped = false;
    }
    line
}

/// The strings of `text`, which holds nothing else but commas and spaces.
fn strings(text: &str) -> Option<Vec<String>> {
    let mut found = Vec::new();
    let mut chars = text.chars();
    loop {
        match chars.next() {
            None => return Some(found),
            Some(c) if c == ',' || c.is_whitespace() => {}
            Some('"') => {
                let mut string = String::new();
                loop {
                    match chars.next()? {
                        '"' => break,
                        '\\' => string.push(chars.next()?),
                        c => string.push(c),
                    }
                }
                found.push(string);
            }
            Some(_) => return None,
        }
    }
}

fn value(text: &str) -> Option<Value> {
    let list = text
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'));
    match (text, list) {
        ("true", _) => Some(Value::Flag(true)),
        ("false", _) => Some(Value::Flag(false)),
        (_, Some(list)) => strings(list).map(Value::List),
        _ => {
            let mut one = strings(text)?;
            let text = one.pop().filter(|_| one.is_empty());
            text.map(Value::Text)
        }
    }
}

impl Table {
    /// Reads a table: the subset of TOML `ambient/globals.toml` is written in
    /// (`[[global]]`, `[global.removed]`, `[global.present]`; strings, booleans and
    /// lists of strings; comments).
    ///
    /// # Errors
    ///
    /// What is wrong, with its line.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut rows: Vec<Row> = Vec::new();
        let mut section = Section::Global;
        let mut lines = text.lines().enumerate();
        while let Some((at, line)) = lines.next() {
            let fail = |why: &str| format!("line {}: {why}", at + 1);
            let line = code(line).trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with('[') {
                section = match line {
                    "[[global]]" => {
                        rows.push(Row::default());
                        Section::Global
                    }
                    "[global.removed]" => Section::Removed,
                    "[global.present]" => Section::Present,
                    _ => return Err(fail("an unknown table")),
                };
                continue;
            }
            let Some((key, rest)) = line.split_once('=') else {
                return Err(fail("neither a table nor `key = value`"));
            };
            let (key, mut rest) = (key.trim().to_owned(), rest.trim().to_owned());
            while rest.starts_with('[') && !rest.ends_with(']') {
                let Some((_, more)) = lines.next() else {
                    return Err(fail("a list that does not end"));
                };
                rest.push(' ');
                rest.push_str(code(more).trim());
            }
            let Some(row) = rows.last_mut() else {
                return Err(fail("a key before the first `[[global]]`"));
            };
            match (section, value(&rest)) {
                (Section::Global, Some(Value::Text(text))) => {
                    row.texts.insert(key, text);
                }
                (Section::Global, Some(Value::Flag(flag))) => {
                    row.flags.insert(key, flag);
                }
                (Section::Global, Some(Value::List(list))) => {
                    row.lists.insert(key, list);
                }
                (Section::Removed, Some(Value::Text(text))) => {
                    row.removed.insert(key, text);
                }
                (Section::Present, Some(Value::List(list))) => {
                    row.present.insert(key, list);
                }
                _ => return Err(fail("a value its table does not hold")),
            }
        }
        let mut table = Self::default();
        for mut row in rows {
            let mut text = |key: &str| row.texts.remove(key);
            let (name, spec, source) = (text("name"), text("spec"), text("source"));
            let (removed, members_only) = (text("removed"), text("members_only"));
            let mut list = |key: &str| row.lists.remove(key).unwrap_or_default();
            let (present, members) = (list("present"), list("members"));
            let mut flag = |key: &str| row.flags.remove(key).unwrap_or_default();
            let global = Global {
                name: name.unwrap_or_default(),
                spec: spec.unwrap_or_default(),
                source: source.unwrap_or_default(),
                present,
                members,
                open: flag("open"),
                removed,
                members_only,
                test: flag("test"),
                syntax: flag("syntax"),
                removed_members: row.removed,
                member_present: row.present,
            };
            let unknown = row.texts.keys().chain(row.flags.keys());
            if let Some(key) = unknown.chain(row.lists.keys()).next() {
                return Err(format!("`{}`: an unknown key `{key}`", global.name));
            }
            table.index.insert(global.name.clone(), table.globals.len());
            table.globals.push(global);
        }
        Ok(table)
    }

    /// The table the compiler carries: `ambient/globals.toml`.
    #[must_use]
    pub fn shipped() -> &'static Self {
        static TABLE: OnceLock<Table> = OnceLock::new();
        TABLE.get_or_init(|| {
            Self::parse(include_str!("../../../ambient/globals.toml")).unwrap_or_default()
        })
    }

    /// The global of this name.
    #[must_use]
    pub fn global(&self, name: &str) -> Option<&Global> {
        self.index.get(name).map(|&at| &self.globals[at])
    }

    /// The globals, in the order of the file.
    pub fn globals(&self) -> impl Iterator<Item = &Global> {
        self.globals.iter()
    }
}

/// A use of a name the file does not declare.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Use<'a> {
    /// The name.
    pub name: &'a str,
    /// Whether it is read by a key: `name.key`, `name[…]`.
    pub keyed: bool,
    /// The key, when it is a name or a string literal.
    pub member: Option<&'a str>,
    /// Whether the use assigns the name.
    pub write: bool,
    /// Whether the file is a `.test.js` file.
    pub test_file: bool,
}

/// What is wrong with a use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    /// The code.
    pub code: Code,
    /// Whether the finding is on the key and not on the name.
    pub on_member: bool,
    /// The message.
    pub message: String,
}

/// `a`, `a and b`, `a, b and c`.
fn listed(names: &[&str]) -> String {
    let (last, before) = names.split_last().unwrap_or((&"", &[]));
    if before.is_empty() {
        return (*last).to_owned();
    }
    format!("{} and {last}", before.join(", "))
}

/// What a finding says of a replacement: the one of SPEC.md §12, that there is none,
/// or nothing.
fn instead(replacement: &str) -> String {
    match replacement {
        "" => String::new(),
        "none" => ": there is no replacement".to_owned(),
        _ => format!(": use {replacement}"),
    }
}

const EVERYWHERE: &str = "a program runs on all five runtimes";

impl Table {
    /// What is wrong with a use of a name the file does not declare, if anything.
    #[must_use]
    pub fn judge(&self, used: &Use<'_>) -> Option<Verdict> {
        let name = used.name;
        let verdict = |code, on_member, message| {
            Some(Verdict {
                code,
                on_member,
                message,
            })
        };
        let Some(global) = self.global(name) else {
            let message = format!(
                "`{name}` is not declared, and is no global of the five runtimes (SPEC.md §5.2)"
            );
            return verdict(Code::E204, false, message);
        };
        let spec = &global.spec;
        if global.test && !used.test_file {
            let message =
                format!("`{name}` is a test global: only a `.test.js` file has it (SPEC.md §10.1)");
            return verdict(Code::E204, false, message);
        }
        let missing = global.missing();
        if !missing.is_empty() {
            let instead = global.removed.as_deref().map_or(String::new(), instead);
            let message = format!(
                "`{name}` is missing on {}: {EVERYWHERE}{} (SPEC.md §5.2)",
                listed(&missing),
                instead.replacen(':', ";", 1)
            );
            return verdict(Code::E204, false, message);
        }
        if let Some(replacement) = &global.removed {
            let message = format!(
                "`{name}` is removed{} (SPEC.md {spec})",
                instead(replacement)
            );
            return verdict(Code::E133, false, message);
        }
        if used.write && !used.test_file {
            let message = format!(
                "`{name}` is a global: only a `.test.js` file may assign one (SPEC.md §10.3)"
            );
            return verdict(Code::E138, false, message);
        }
        let Some(member) = used.member else {
            let replacement = global.members_only.as_ref().filter(|_| !used.keyed)?;
            let message = format!(
                "`{name}` is used through its members only{} (SPEC.md {spec})",
                instead(replacement)
            );
            return verdict(Code::E133, false, message);
        };
        let missing = global.member_missing(member);
        if !missing.is_empty() {
            let message = format!(
                "`{name}.{member}` is missing on {}: {EVERYWHERE} (SPEC.md §6)",
                listed(&missing)
            );
            return verdict(Code::E204, true, message);
        }
        if let Some(replacement) = global.removed_members.get(member) {
            let message = format!(
                "`{name}.{member}` is removed{} (SPEC.md {spec})",
                instead(replacement)
            );
            return verdict(Code::E133, true, message);
        }
        if global.open || global.members.iter().any(|kept| kept == member) {
            return None;
        }
        let message =
            format!("`{name}.{member}` is not a member the language keeps (SPEC.md {spec})");
        verdict(Code::E133, true, message)
    }
}

#[cfg(test)]
mod tests {
    use super::{Table, RUNTIMES};

    #[test]
    fn a_table_is_read_as_written() {
        let table = Table::parse(concat!(
            "# a comment\n",
            "[[global]] # the first\n",
            "name = \"A\"\n",
            "spec = \"§6.1\"\n",
            "source = \"a \\\"quoted\\\" \\\\ # source\" # and a comment\n",
            "present = [\"browser\", \"node\"]\n",
            "members = [\n    \"one\", # the first\n    \"two\",\n]\n",
            "open = false\n",
            "members_only = \"x = y\"\n",
            "\n",
            "[global.removed]\n",
            "gone = \"another\"\n",
            "bare = \"\"\n",
            "open_quote = \"a \\\" # b\"\n",
            "backslash = \"a \\\\\" # \"\n",
            "[global.present]\n",
            "two = [\"bun\"]\n",
            "[[global]]\n",
            "name = \"b\"\n",
            "removed = \"none\"\n",
            "test = true\n",
            "syntax = true\n",
            "open = true\n",
            "present = []\n",
        ))
        .expect("a table");
        let names: Vec<&str> = table.globals().map(|g| g.name.as_str()).collect();
        assert_eq!(names, ["A", "b"], "in the order of the file");
        let a = table.global("A").expect("A");
        assert_eq!(a.spec, "§6.1");
        assert_eq!(a.source, "a \"quoted\" \\ # source");
        assert_eq!(a.present, ["browser", "node"]);
        assert_eq!(a.members, ["one", "two"]);
        assert_eq!(a.members_only.as_deref(), Some("x = y"));
        assert_eq!((a.open, a.test, a.removed.as_deref()), (false, false, None));
        let removed: Vec<_> = a.removed_members.iter().collect();
        assert_eq!(removed.len(), 4);
        assert_eq!(a.removed_members["open_quote"], "a \" # b");
        assert_eq!(a.removed_members["backslash"], "a \\");
        assert_eq!(a.removed_members["gone"], "another");
        assert_eq!(a.removed_members["bare"], "");
        assert_eq!(a.missing(), ["Deno 2", "Bun", "Cloudflare Workers"]);
        assert_eq!(
            a.member_missing("two"),
            ["browsers", "Node 24", "Deno 2", "Cloudflare Workers"]
        );
        assert_eq!(a.member_missing("one"), Vec::<&str>::new());
        let b = table.global("b").expect("b");
        assert_eq!(
            (b.open, b.test, b.removed.as_deref()),
            (true, true, Some("none"))
        );
        assert_eq!(b.missing().len(), 5);
        assert!(b.syntax && !a.syntax);
        assert_eq!(b.members, Vec::<String>::new());
        assert_eq!(table.global("c"), None);
        assert_eq!(Table::parse("").expect("empty"), Table::default());
    }

    #[test]
    fn what_is_no_table_is_refused_with_its_line() {
        let cases = [
            (
                "name = \"a\"\n",
                "line 1: a key before the first `[[global]]`",
            ),
            (
                "[global.removed]\na = \"b\"\n",
                "line 2: a key before the first `[[global]]`",
            ),
            ("\n[[globals]]\n", "line 2: an unknown table"),
            (
                "[[global]]\nname\n",
                "line 2: neither a table nor `key = value`",
            ),
            (
                "[[global]]\nmembers = [\n\"a\",\n",
                "line 2: a list that does not end",
            ),
            (
                "[[global]]\nname = a\n",
                "line 2: a value its table does not hold",
            ),
            (
                "[[global]]\nname = \"a\n",
                "line 2: a value its table does not hold",
            ),
            (
                "[[global]]\nname = \"a\\",
                "line 2: a value its table does not hold",
            ),
            (
                "[[global]]\nname = \"a\" \"b\"\n",
                "line 2: a value its table does not hold",
            ),
            (
                "[[global]]\nname = \n",
                "line 2: a value its table does not hold",
            ),
            (
                "[[global]]\nmembers = [a]\n",
                "line 2: a value its table does not hold",
            ),
            (
                "[[global]]\nopen = yes\n",
                "line 2: a value its table does not hold",
            ),
            (
                "[[global]]\n[global.removed]\na = [\"b\"]\n",
                "line 3: a value its table does not hold",
            ),
            (
                "[[global]]\n[global.removed]\na = true\n",
                "line 3: a value its table does not hold",
            ),
            (
                "[[global]]\n[global.present]\na = \"b\"\n",
                "line 3: a value its table does not hold",
            ),
            (
                "[[global]]\nname = \"a\"\nmember = \"b\"\n",
                "`a`: an unknown key `member`",
            ),
            (
                "[[global]]\nname = \"a\"\ntests = true\n",
                "`a`: an unknown key `tests`",
            ),
            (
                "[[global]]\nname = \"a\"\nmember = [\"b\"]\n",
                "`a`: an unknown key `member`",
            ),
        ];
        for (text, why) in cases {
            assert_eq!(Table::parse(text), Err(why.to_owned()), "{text:?}");
        }
    }

    #[test]
    fn the_shipped_table_is_whole() {
        let text = include_str!("../../../ambient/globals.toml");
        let table = Table::parse(text).expect("ambient/globals.toml is a table");
        assert_eq!(&table, Table::shipped());
        let words: Vec<&str> = RUNTIMES.iter().map(|(word, _)| *word).collect();
        let known = |present: &[String]| present.iter().all(|p| words.contains(&p.as_str()));
        let mut count = 0;
        for global in table.globals() {
            let name = &global.name;
            count += 1;
            assert!(
                !name.is_empty() && global.spec.starts_with('§'),
                "{name}: spec"
            );
            assert!(!global.source.is_empty(), "{name}: the source is cited");
            assert!(known(&global.present), "{name}: runtimes");
            assert!(global.member_present.values().all(|p| known(p)), "{name}");
            for member in global.member_present.keys() {
                assert!(global.members.contains(member), "{name}.{member} is kept");
            }
            for member in global.removed_members.keys() {
                assert!(
                    !global.members.contains(member),
                    "{name}.{member}: kept or removed"
                );
            }
            assert!(
                !global.open || global.members.is_empty(),
                "{name}: open or listed"
            );
            assert_eq!(
                text.matches(&format!("\nname = \"{name}\"\n")).count(),
                1,
                "{name}"
            );
        }
        assert_eq!(count, text.matches("[[global]]\n").count());
        assert!(table.global("Temporal").expect("Temporal").missing().len() > 1);
    }
}
