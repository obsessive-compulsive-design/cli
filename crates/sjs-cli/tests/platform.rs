//! STORIES.md US-099: the fixtures under `tests/conformance/06-ambient/platform` are
//! generated from `ambient/platform/*.toml`, so that every member the data files list
//! is used once without a finding, and a member none lists is E133. This test fails
//! when the fixtures differ from the data, and `SJS_UPDATE_SNAPSHOTS=1` writes them
//! anew. The conformance harness then holds the compiler to each of them.

use sjs_bind::ambient::Table;
use sjs_infer::platform::{platform, spec, Interface, Kind, Spec};
use sjs_testkit::fs::TempDir;
use sjs_testkit::snapshot::Mode;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/conformance/06-ambient/platform")
}

/// A member no interface has: what is not listed is not kept.
const UNLISTED: &str = "notKept";

/// The name a fixture gives an instance of an interface.
fn held(interface: &str) -> String {
    let mut letters = interface.chars();
    let first = letters.next().map(|first| first.to_ascii_lowercase());
    format!("{}{}", first.unwrap_or('x'), letters.as_str())
}

/// A fixture being written: the instances it declares, and its lines.
#[derive(Default)]
struct Fixture {
    /// Each instance it needs: the name, and the expression that gives it.
    instances: Vec<(String, String)>,
    /// Each use: the expression, and where in it a finding is expected.
    uses: Vec<(String, Option<usize>)>,
}

impl Fixture {
    /// The name of an instance of `interface`, which the fixture then declares.
    fn instance(&mut self, interface: &str) -> String {
        let name = held(interface);
        let sample = match interface {
            "Uint8Array" => Some("new Uint8Array(1)".to_owned()),
            "ArrayBuffer" => Some("new ArrayBuffer(1)".to_owned()),
            _ => {
                let table = platform().iter().find(|table| table.name == interface);
                table.and_then(|table| table.sample.clone())
            }
        };
        let sample = sample.unwrap_or_else(|| panic!("{interface} has no sample"));
        if !self.instances.iter().any(|(known, _)| *known == name) {
            self.instances.push((name.clone(), sample));
        }
        name
    }

    /// An expression of the type a parameter takes.
    fn sample(&mut self, spec: &Spec) -> String {
        match spec {
            Spec::Name(name) => match name.as_str() {
                "string" => "\"a\"".to_owned(),
                "number" | "int32" | "unknown" => "1".to_owned(),
                "boolean" => "true".to_owned(),
                interface => self.instance(interface),
            },
            Spec::Array(element) => format!("[{}]", self.sample(element)),
            Spec::Union(members) => self.sample(&members[0]),
            Spec::Function(..) => "() => undefined".to_owned(),
            other => panic!("no sample of {other:?}"),
        }
    }

    /// The arguments of a call of what is written `what`: one for each parameter
    /// that is not left out.
    fn arguments(&mut self, what: &str) -> String {
        let signature = spec(what).unwrap_or_else(|| panic!("`{what}` is not read"));
        let Spec::Function(parameters, _) = signature else {
            panic!("`{what}` is no function");
        };
        let required = parameters
            .iter()
            .filter(|(optional, rest, _)| !optional && !rest);
        let passed: Vec<String> = required.map(|(_, _, ty)| self.sample(ty)).collect();
        passed.join(", ")
    }

    /// One use of a member `name` of `on`, written `what`.
    fn member(&mut self, on: &str, name: &str, what: &str) {
        let is_function = matches!(spec(what), Some(Spec::Function(..)));
        let used = match name {
            "[Symbol.iterator]" => format!("[...{on}]"),
            "[Symbol.asyncIterator]" => format!("Array.fromAsync({on})"),
            _ if is_function => format!("{on}.{name}({})", self.arguments(what)),
            _ => format!("{on}.{name}"),
        };
        self.uses.push((used, None));
    }

    /// The text of the fixture.
    fn text(&self) -> String {
        let mut body = Vec::new();
        for (name, sample) in &self.instances {
            body.push(format!("const {name} = {sample}"));
        }
        let mut expected = Vec::new();
        let findings = self.uses.iter().filter(|(_, at)| at.is_some()).count();
        for (at, (used, finding)) in self.uses.iter().enumerate() {
            let declared = format!("export const use{} = ", at + 1);
            // After the `expect` lines and the two of the note, a line and a gap each.
            let row = findings + 3 + 2 * body.len();
            expected.extend(finding.map(|column| {
                let column = declared.len() + column + 1;
                format!("// expect: E133 removed-global-or-member @{row}:{column}\n")
            }));
            body.push(format!("{declared}{used}"));
        }
        format!(
            "{}// Generated from ambient/platform/*.toml, not written by hand:\n// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform\n{}\n",
            expected.concat(),
            body.join("\n\n")
        )
    }
}

/// The fixtures of the data files: path beneath the directory, and text.
fn generated(tables: &[Interface]) -> BTreeMap<String, String> {
    let mut files = BTreeMap::new();
    for table in tables {
        let mut pass = Fixture::default();
        let name = table.name.as_str();
        if let Some(call) = &table.call {
            let passed = pass.arguments(call);
            pass.uses.push((format!("{name}({passed})"), None));
        }
        if let Some(parameters) = &table.constructor {
            let passed = pass.arguments(&format!("{parameters} => unknown"));
            pass.uses.push((format!("new {name}({passed})"), None));
        }
        // An interface a program is only handed has no fixture: `sjs-infer` tests it.
        if table.kind == Kind::Interface && table.sample.is_none() {
            continue;
        }
        let on = match table.kind {
            Kind::Interface => pass.instance(name),
            Kind::Namespace | Kind::Function => name.to_owned(),
        };
        for member in &table.members {
            let on = if member.is_static { name } else { on.as_str() };
            pass.member(on, &member.name, &member.what);
        }
        if table.kind == Kind::Interface && pass.uses.is_empty() {
            // An interface with no member is used by being made.
            pass.uses.push((on.clone(), None));
        }
        files.insert(format!("pass/{name}.js"), pass.text());
        if table.kind == Kind::Interface {
            let mut removed = Fixture::default();
            let on = removed.instance(name);
            let unlisted = format!("{on}.{UNLISTED}");
            removed.uses.push((unlisted, Some(on.len() + 1)));
            // An interface that is a global is a class by name too.
            if Table::shipped().global(name).is_some() {
                removed
                    .uses
                    .push((format!("{name}.{UNLISTED}"), Some(name.len() + 1)));
            }
            files.insert(format!("E133/{name}.js"), removed.text());
        }
    }
    files
}

/// The files beneath `directory`, one level down: path and text.
fn on_disk(directory: &Path) -> BTreeMap<String, String> {
    let mut files = BTreeMap::new();
    let entries = |path: &Path| fs::read_dir(path).into_iter().flatten().flatten();
    for group in entries(directory) {
        for file in entries(&group.path()) {
            let path = format!(
                "{}/{}",
                group.file_name().to_string_lossy(),
                file.file_name().to_string_lossy()
            );
            files.insert(path, fs::read_to_string(file.path()).expect("a fixture"));
        }
    }
    files
}

/// Makes the files beneath `directory` exactly `files`.
fn write(directory: &Path, files: &BTreeMap<String, String>) {
    let stored = on_disk(directory);
    for stale in stored.keys().filter(|path| !files.contains_key(*path)) {
        fs::remove_file(directory.join(stale)).expect("a stale fixture is removed");
    }
    for (path, text) in files {
        let path = directory.join(path);
        fs::create_dir_all(path.parent().expect("a directory")).expect("the directory");
        fs::write(path, text).expect("the fixture");
    }
}

#[test]
fn the_fixtures_of_the_platform_are_the_ones_on_disk() {
    let files = generated(platform());
    let scratch = TempDir::new("us099").expect("a temporary directory");
    scratch.write("pass/stale.js", "").expect("a stale file");
    let update = Mode::from_env() == Mode::Update;
    let targets = [scratch.path().to_path_buf(), fixtures()];
    for target in targets.iter().take(1 + usize::from(update)) {
        write(target, &files);
    }
    assert!(on_disk(scratch.path()) == files, "writing is exact");
    let stored = on_disk(&fixtures());
    let differing: Vec<&String> = files
        .iter()
        .filter(|(path, text)| stored.get(*path) != Some(*text))
        .map(|(path, _)| path)
        .chain(stored.keys().filter(|path| !files.contains_key(*path)))
        .collect();
    assert!(
        differing.is_empty(),
        "the fixtures differ from ambient/platform; regenerate them with SJS_UPDATE_SNAPSHOTS=1: {differing:?}"
    );
}

#[test]
fn every_listed_member_is_used_and_an_unlisted_one_is_removed() {
    let files = generated(platform());
    let text = |path: &str| files.get(path).cloned().unwrap_or_default();
    for table in platform() {
        let pass = text(&format!("pass/{}.js", table.name));
        if table.name == "MessageEvent" {
            assert!(
                pass.is_empty() && table.sample.is_none(),
                "handed, not made"
            );
            continue;
        }
        assert!(!pass.is_empty(), "{} has no fixture", table.name);
        for member in &table.members {
            let named = member.name.starts_with('[') || pass.contains(&format!(".{}", member.name));
            assert!(named, "{}.{} is not used", table.name, member.name);
        }
    }
    assert_eq!(
        text("E133/Headers.js"),
        "// expect: E133 removed-global-or-member @7:29\n// expect: E133 removed-global-or-member @9:29\n// Generated from ambient/platform/*.toml, not written by hand:\n// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform\nconst headers = new Headers()\n\nexport const use1 = headers.notKept\n\nexport const use2 = Headers.notKept\n"
    );
    assert!(text("pass/Headers.js").contains("export const use2 = headers.append(\"a\", \"a\")\n"));
    assert!(text("pass/Headers.js").contains(" = [...headers]\n"));
    assert!(text("pass/fetch.js").contains("export const use1 = fetch(\"a\")\n"));
    assert!(text("pass/console.js").contains("console.groupEnd()\n"));
    assert!(text("pass/Response.js").contains(" = Response.json(1)\n"));
    assert!(text("pass/ReadableStream.js").contains(" = Array.fromAsync(readableStream)\n"));
    assert!(text("pass/SubtleCrypto.js").starts_with("// Generated"));
    assert!(
        text("pass/SubtleCrypto.js").contains("const cryptoKey = await crypto.subtle.importKey(")
    );
    assert!(text("pass/Timeout.js").contains("export const use1 = timeout\n"));
    assert!(!files.contains_key("E133/console.js") && !files.contains_key("E133/fetch.js"));
}
