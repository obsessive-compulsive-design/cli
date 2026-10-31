//! STORIES.md US-048: for every conformance fixture the grammar accepts, each change the
//! layout makes is claimed by a finding; a change no pass claims is a fault in a pass.

use sjs_diag::SourceFile;
use sjs_format::Formatter;
use sjs_syntax::parse;
use sjs_testkit::conformance::discover;
use std::path::Path;

#[test]
fn no_fixture_has_a_change_no_pass_claims() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/conformance");
    let fixtures = discover(&root).expect("the fixtures");
    assert_ne!(fixtures.len(), 0, "fixtures discovered");
    let mut checked = 0;
    for fixture in &fixtures {
        for file in &fixture.files {
            let Ok(source) = SourceFile::new(file.relative.clone(), file.bytes.clone()) else {
                continue;
            };
            let (tree, parsed) = parse(&source);
            if parsed.has_errors() {
                continue;
            }
            let report = Formatter::new(&source, &tree).report(&parsed);
            assert!(
                report.unclassified.is_empty(),
                "{}: a change no pass claims: {:?}",
                fixture.name,
                report.unclassified
            );
            checked += 1;
        }
    }
    assert!(checked > 50, "{checked} clean fixtures checked");
}
