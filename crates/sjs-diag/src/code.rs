//! Every diagnostic code of SPEC.md with its name, severity and defining section.

use std::fmt;
use std::str::FromStr;

/// How a finding affects the build (SPEC.md §8): errors fail it, style findings are only
/// reported with the formatter's replacement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// An E-code: fails `build`, `lint`, `map`, `docs` and `test` (exit 1).
    Error,
    /// An S-code: reported with before/after text, applied only by `format --write`.
    Style,
}

/// The string was not a code this compiler knows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseCodeError {
    /// The text that failed to parse.
    pub text: String,
}

impl fmt::Display for ParseCodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown diagnostic code {:?}", self.text)
    }
}

impl std::error::Error for ParseCodeError {}

/// One row of the code table: `(code text, name, section)`. Severity follows from the
/// letter: `E` is an error, `S` is style (SPEC.md §8).
type Row = (&'static str, &'static str, &'static str);

macro_rules! codes {
    ($( $variant:ident => ($name:literal, $section:literal) ),+ $(,)?) => {
        /// Every diagnostic code SPEC.md defines (§8.1–§8.5, §5, §13.7). The variant name
        /// is the code text; `name()` is the kebab-case rule name the reporter prints.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum Code {
            $(
                #[doc = $name]
                $variant,
            )+
        }

        /// Every code, in table (and therefore numeric) order.
        const TABLE: &[Row] = &[ $( (stringify!($variant), $name, $section), )+ ];

        impl Code {
            /// Every code, in numeric order.
            pub const ALL: &'static [Code] = &[ $( Code::$variant, )+ ];
        }
    };
}

codes! {
    E000 => ("syntax", "§2"),
    E001 => ("ambiguous-line-start", "§2"),
    E002 => ("regex-flags", "§2"),
    E100 => ("null-produced", "§3.1"),
    E101 => ("excess-property", "§3.3"),
    E102 => ("ambiguous-truthiness", "§3.5"),
    E103 => ("non-exhaustive", "§3.5"),
    E104 => ("bitwise-on-double", "§3.5"),
    E105 => ("no-overlap", "§4.1"),
    E106 => ("unused-let", "§4.2"),
    E107 => ("block-shadow", "§4.2"),
    E108 => ("fallthrough", "§4.3"),
    E109 => ("finally-return", "§4.3"),
    E110 => ("empty-untyped", "§3.2"),
    E111 => ("mixed-return", "§4.3"),
    E112 => ("unconstrained-parameter", "§4.4"),
    E113 => ("floating-promise", "§4.4"),
    E114 => ("uninitialized-field", "§4.5"),
    E115 => ("unused-binding", "§8.1"),
    E116 => ("unused-expression", "§8.1"),
    E117 => ("unreachable-code", "§8.1"),
    E118 => ("async-no-await", "§8.1"),
    E119 => ("await-non-promise", "§4.1"),
    E120 => ("self-assign-or-compare", "§8.1"),
    E121 => ("constant-condition", "§8.1"),
    E122 => ("write-to-frozen", "§3.4"),
    E123 => ("modified-loop-variable", "§8.1"),
    E124 => ("array-index-unchecked", "§4.6"),
    E125 => ("template-non-string", "§4.1"),
    E126 => ("this-outside-class", "§4.4"),
    E127 => ("promise-executor", "§8.1"),
    E128 => ("then-chain", "§8.1"),
    E129 => ("shape-change", "§4.5"),
    E130 => ("throw-non-error", "§3.6"),
    E131 => ("catch-untyped-use", "§3.6"),
    E132 => ("commented-out-code", "§8.1"),
    E133 => ("removed-global-or-member", "§6"),
    E134 => ("duplicate-key", "§4.6"),
    E135 => ("use-before-declaration", "§8.1"),
    E136 => ("arity-mismatch", "§8.1"),
    E137 => ("missing-property", "§3.2"),
    E138 => ("type-mismatch", "§3.3"),
    E139 => ("not-callable", "§8.1"),
    E140 => ("function-length", "§8.2"),
    E141 => ("cyclomatic-complexity", "§8.2"),
    E142 => ("nesting-depth", "§8.2"),
    E143 => ("parameter-count", "§8.2"),
    E144 => ("file-length", "§8.2"),
    E145 => ("class-members", "§8.2"),
    E150 => ("naming-case", "§8.3"),
    E151 => ("boolean-name", "§8.3"),
    E152 => ("file-name", "§8.3"),
    E153 => ("underscore-prefix", "§8.3"),
    E160 => ("mixed-exports", "§8.4"),
    E161 => ("export-star", "§8.4"),
    E162 => ("missing-doc-comment", "§8.4"),
    E163 => ("todo-without-issue", "§8.4"),
    E201 => ("foreign-import", "§5.1"),
    E202 => ("default-export", "§5.1"),
    E203 => ("import-cycle", "§5.1"),
    E204 => ("unknown-global", "§5.2"),
    E205 => ("unresolved-import", "§5.1"),
    E206 => ("missing-export", "§5.1"),
    E207 => ("unreachable-module", "§5.3"),
    E301 => ("test-no-assert", "§8.1"),
    E302 => ("test-focused-committed", "§10.4"),
    E303 => ("test-async-no-await", "§8.1"),
    E304 => ("test-skipped-without-reason", "§10.4"),
    E401 => ("unknown-doc-tag", "§13.7"),
    E402 => ("doc-mismatch", "§13.7"),
    E403 => ("example-terminates-comment", "§13.7"),
    S001 => ("single-quoted-string", "§8.5"),
    S002 => ("semicolon", "§8.5"),
    S003 => ("import-order", "§8.5"),
    S004 => ("trailing-comma", "§8.5"),
    S005 => ("redundant-parentheses", "§8.5"),
    S006 => ("line-too-long", "§8.5"),
    S007 => ("indentation", "§8.5"),
    S008 => ("layout", "§8.5"),
    S009 => ("member-order", "§8.5"),
    S010 => ("method-chain", "§8.5"),
    S011 => ("arrow-block-return", "§8.5"),
    S012 => ("increment-statement", "§8.5"),
    S020 => ("freeze-candidate", "§8.5"),
    S021 => ("narrowable-signature", "§8.5"),
    S030 => ("deprecated-call", "§8.5"),
}

impl Code {
    fn row(self) -> &'static Row {
        &TABLE[self as usize]
    }

    /// The code text as the reporter prints it, e.g. `"E103"`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        self.row().0
    }

    /// The kebab-case rule name, e.g. `"non-exhaustive"`.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.row().1
    }

    /// The SPEC.md section that defines the rule, e.g. `"§3.5"`.
    #[must_use]
    pub fn section(self) -> &'static str {
        self.row().2
    }

    /// Errors fail the build; style findings never do (SPEC.md §8).
    #[must_use]
    pub fn severity(self) -> Severity {
        if self.as_str().starts_with('E') {
            Severity::Error
        } else {
            Severity::Style
        }
    }

    /// True for the style findings that would change behaviour if applied: S020 (freeze),
    /// S021 (narrow a signature) and S030 (deprecated call). `format --write` never
    /// applies them (SPEC.md §8.5, §13.4).
    #[must_use]
    pub fn is_hand_edit(self) -> bool {
        matches!(self, Code::S020 | Code::S021 | Code::S030)
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Code {
    type Err = ParseCodeError;

    /// Parses the exact code text (`"E103"`); names, lower case and padding are not codes.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Code::ALL
            .iter()
            .copied()
            .find(|code| code.as_str() == text)
            .ok_or_else(|| ParseCodeError {
                text: text.to_owned(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::{Code, ParseCodeError, Severity};

    #[test]
    fn the_table_lists_every_code_exactly_once_in_numeric_order() {
        assert_eq!(Code::ALL.len(), 3 + 40 + 6 + 4 + 4 + 7 + 4 + 3 + 12 + 2 + 1);
        for pair in Code::ALL.windows(2) {
            let (earlier, later) = (pair[0], pair[1]);
            assert!(earlier < later, "{earlier} must precede {later}");
            assert!(earlier.as_str() < later.as_str(), "{earlier} vs {later}");
        }
    }

    #[test]
    fn code_text_matches_the_variant_name_so_the_table_cannot_drift() {
        for code in Code::ALL {
            assert_eq!(format!("{code:?}"), code.as_str());
            assert_eq!(code.to_string(), code.as_str());
        }
    }

    #[test]
    fn every_code_round_trips_through_its_text() {
        for code in Code::ALL {
            assert_eq!(code.as_str().parse::<Code>(), Ok(*code));
        }
    }

    #[test]
    fn names_are_kebab_case_and_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for code in Code::ALL {
            let name = code.name();
            assert!(
                name.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "{name}"
            );
            assert!(
                !name.starts_with('-') && !name.ends_with('-') && !name.contains("--"),
                "{name}"
            );
            assert!(seen.insert(name), "duplicate name {name}");
        }
    }

    #[test]
    fn sections_point_into_the_spec() {
        for code in Code::ALL {
            let section = code.section();
            assert!(section.starts_with('§'), "{code}: {section}");
            assert!(
                section
                    .trim_start_matches('§')
                    .chars()
                    .all(|c| c.is_ascii_digit() || c == '.'),
                "{code}: {section}"
            );
        }
        assert_eq!(Code::E000.section(), "§2");
        assert_eq!(Code::E103.section(), "§3.5");
        assert_eq!(Code::E140.section(), "§8.2");
        assert_eq!(Code::E207.section(), "§5.3");
        assert_eq!(Code::E403.section(), "§13.7");
        assert_eq!(Code::S030.section(), "§8.5");
    }

    #[test]
    fn spot_checked_names_match_the_spec() {
        assert_eq!(Code::E000.name(), "syntax");
        assert_eq!(Code::E001.name(), "ambiguous-line-start");
        assert_eq!(Code::E103.name(), "non-exhaustive");
        assert_eq!(Code::E110.name(), "empty-untyped");
        assert_eq!(Code::E207.name(), "unreachable-module");
        assert_eq!(Code::E304.name(), "test-skipped-without-reason");
        assert_eq!(Code::E403.name(), "example-terminates-comment");
        assert_eq!(Code::S001.name(), "single-quoted-string");
        assert_eq!(Code::S030.name(), "deprecated-call");
    }

    #[test]
    fn severity_follows_the_letter() {
        for code in Code::ALL {
            let expected = if code.as_str().starts_with('E') {
                Severity::Error
            } else {
                Severity::Style
            };
            assert_eq!(code.severity(), expected, "{code}");
        }
        assert_eq!(Code::E000.severity(), Severity::Error);
        assert_eq!(Code::E403.severity(), Severity::Error);
        assert_eq!(Code::S001.severity(), Severity::Style);
        assert_eq!(Code::S030.severity(), Severity::Style);
        assert_eq!(
            Code::ALL
                .iter()
                .filter(|c| c.severity() == Severity::Style)
                .count(),
            15
        );
    }

    #[test]
    fn exactly_s020_s021_and_s030_are_hand_edits() {
        let hand: Vec<Code> = Code::ALL
            .iter()
            .copied()
            .filter(|c| c.is_hand_edit())
            .collect();
        assert_eq!(hand, [Code::S020, Code::S021, Code::S030]);
    }

    #[test]
    fn parsing_is_exact() {
        for bad in [
            "",
            "e103",
            "E103 ",
            " E103",
            "E0103",
            "E10",
            "non-exhaustive",
            "E999",
            "S013",
            "X001",
        ] {
            assert_eq!(
                bad.parse::<Code>(),
                Err(ParseCodeError {
                    text: bad.to_owned()
                }),
                "{bad:?} must not parse"
            );
        }
        let err = "E999".parse::<Code>().expect_err("E999 does not exist");
        assert_eq!(err.to_string(), "unknown diagnostic code \"E999\"");
    }
}
