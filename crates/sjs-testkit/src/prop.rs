//! Property testing: run a property over generated values, replay a failure from its
//! seed, and shrink the counter-example (STORIES.md §0.2 item 6).
//!
//! `SJS_PROP_CASES` sets the case count (default 256; CI uses 4 096). `SJS_PROP_SEED`
//! replays exactly one case, the one whose seed a failure printed.

use std::fmt;
use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::prng::Rng;

/// A generator of values with optional shrinking.
pub trait Gen {
    /// The generated type.
    type Value: Clone + fmt::Debug;

    /// A random value.
    fn generate(&self, rng: &mut Rng) -> Self::Value;

    /// Simpler candidates to try when `value` fails, most aggressive first; a generator
    /// that cannot shrink returns an empty vector.
    fn shrink(&self, value: &Self::Value) -> Vec<Self::Value>;
}

/// How many cases to run, and from which seed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Cases to run when no seed is given.
    pub cases: u32,
    /// A single case to replay.
    pub seed: Option<u64>,
    /// Upper bound on shrink steps.
    pub max_shrinks: u32,
}

/// The seed the stream of case seeds starts from when none is given.
pub const DEFAULT_MASTER_SEED: u64 = 0x5EED_0000_0000_0001;

impl Config {
    /// `cases` cases from the default master seed.
    #[must_use]
    pub const fn new(cases: u32) -> Self {
        Self {
            cases,
            seed: None,
            max_shrinks: 10_000,
        }
    }

    /// Replays the one case with `seed`.
    #[must_use]
    pub const fn replay(seed: u64) -> Self {
        Self {
            cases: 1,
            seed: Some(seed),
            max_shrinks: 10_000,
        }
    }

    /// Reads `SJS_PROP_CASES` and `SJS_PROP_SEED` from the environment.
    #[must_use]
    pub fn from_env() -> Self {
        Self::from_vars(
            std::env::var("SJS_PROP_CASES").ok().as_deref(),
            std::env::var("SJS_PROP_SEED").ok().as_deref(),
        )
    }

    /// Builds from the two variables' values; unparsable values fall back to defaults.
    #[must_use]
    pub fn from_vars(cases: Option<&str>, seed: Option<&str>) -> Self {
        let cases = cases.and_then(|c| c.trim().parse().ok()).unwrap_or(256);
        let seed = seed.and_then(|s| s.trim().parse().ok());
        Self {
            cases,
            seed,
            max_shrinks: 10_000,
        }
    }
}

/// A failed property with its minimal counter-example.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// The seed that generated the failing value (`SJS_PROP_SEED=<seed>` replays it).
    pub seed: u64,
    /// Which case (0-based) failed.
    pub case: u32,
    /// The value as generated.
    pub original: String,
    /// The value after shrinking.
    pub minimal: String,
    /// The property's message for the minimal value.
    pub message: String,
    /// How many shrink steps were taken.
    pub shrinks: u32,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "property failed (case {}, replay with SJS_PROP_SEED={})
  minimal: {}
  message: {}
  original: {} ({} shrink steps)",
            self.case, self.seed, self.minimal, self.message, self.original, self.shrinks
        )
    }
}

impl std::error::Error for Failure {}

/// Runs `property` over values of `gen` with the environment's configuration and panics
/// with the minimal counter-example on failure.
///
/// # Panics
///
/// When the property fails.
pub fn check<G: Gen>(gen: &G, property: impl Fn(&G::Value) -> Result<(), String>) {
    if let Err(failure) = check_with(&Config::from_env(), gen, &property) {
        panic!("{failure}");
    }
}

/// [`check`] with an explicit configuration, returning the failure instead of panicking.
///
/// A property may fail by returning `Err` or by panicking; both count.
///
/// # Errors
///
/// The first failing case, shrunk.
pub fn check_with<G: Gen>(
    config: &Config,
    gen: &G,
    property: &dyn Fn(&G::Value) -> Result<(), String>,
) -> Result<(), Failure> {
    let mut master = Rng::from_seed(config.seed.unwrap_or(DEFAULT_MASTER_SEED));
    let cases = if config.seed.is_some() {
        1
    } else {
        config.cases
    };
    for case in 0..cases {
        let seed = config.seed.unwrap_or_else(|| master.next_u64());
        let value = gen.generate(&mut Rng::from_seed(seed));
        if let Err(message) = run_property(property, &value) {
            return Err(shrink(config, gen, property, case, seed, &value, message));
        }
    }
    Ok(())
}

fn run_property<V>(property: &dyn Fn(&V) -> Result<(), String>, value: &V) -> Result<(), String> {
    match catch_unwind(AssertUnwindSafe(|| property(value))) {
        Ok(result) => result,
        Err(payload) => Err(panic_message(payload.as_ref())),
    }
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_owned()))
        .unwrap_or_else(|| "panic with a non-string payload".to_owned())
}

fn shrink<G: Gen>(
    config: &Config,
    gen: &G,
    property: &dyn Fn(&G::Value) -> Result<(), String>,
    case: u32,
    seed: u64,
    original: &G::Value,
    mut message: String,
) -> Failure {
    let mut current = original.clone();
    let mut shrinks = 0;
    'outer: while shrinks < config.max_shrinks {
        for candidate in gen.shrink(&current) {
            if let Err(candidate_message) = run_property(property, &candidate) {
                current = candidate;
                message = candidate_message;
                shrinks += 1;
                continue 'outer;
            }
        }
        break;
    }
    Failure {
        seed,
        case,
        original: format!("{original:?}"),
        minimal: format!("{current:?}"),
        message,
        shrinks,
    }
}

/// Integers in `lo..=hi`, shrinking toward `lo`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct U64 {
    /// Smallest value.
    pub lo: u64,
    /// Largest value.
    pub hi: u64,
}

impl Gen for U64 {
    type Value = u64;

    fn generate(&self, rng: &mut Rng) -> u64 {
        rng.range(self.lo, self.hi)
    }

    /// `lo`, the midpoint toward `lo`, and the predecessor, each once and only below
    /// `value`.
    fn shrink(&self, value: &u64) -> Vec<u64> {
        let mut out = Vec::new();
        for candidate in [
            self.lo,
            self.lo + (value.saturating_sub(self.lo)) / 2,
            value.saturating_sub(1),
        ] {
            if candidate < *value && candidate >= self.lo && !out.contains(&candidate) {
                out.push(candidate);
            }
        }
        out
    }
}

/// One of a fixed, non-empty set of values, shrinking toward the first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Elements<'a, T> {
    items: &'a [T],
}

impl<'a, T> Elements<'a, T> {
    /// A generator over `items`, or `None` when there is nothing to generate.
    #[must_use]
    pub const fn new(items: &'a [T]) -> Option<Self> {
        if items.is_empty() {
            None
        } else {
            Some(Self { items })
        }
    }
}

impl<T: Clone + fmt::Debug + PartialEq> Gen for Elements<'_, T> {
    type Value = T;

    fn generate(&self, rng: &mut Rng) -> T {
        let index = rng.index(self.items.len());
        self.items[index].clone()
    }

    /// The first item and the item just before `value`, when they differ from it.
    fn shrink(&self, value: &T) -> Vec<T> {
        let index = self
            .items
            .iter()
            .position(|item| item == value)
            .unwrap_or_default();
        let mut out = Vec::new();
        for candidate in [0, index.saturating_sub(1)] {
            if candidate < index && !out.contains(&candidate) {
                out.push(candidate);
            }
        }
        out.into_iter().map(|i| self.items[i].clone()).collect()
    }
}

/// Vectors of up to `max_len` elements, shrinking by halving, dropping one element, and
/// shrinking one element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VecOf<G> {
    /// The element generator.
    pub element: G,
    /// Longest vector generated.
    pub max_len: usize,
}

impl<G: Gen> Gen for VecOf<G> {
    type Value = Vec<G::Value>;

    fn generate(&self, rng: &mut Rng) -> Self::Value {
        let len = rng.index(self.max_len + 1);
        (0..len).map(|_| self.element.generate(rng)).collect()
    }

    fn shrink(&self, value: &Self::Value) -> Vec<Self::Value> {
        let mut out = Vec::new();
        if value.len() > 1 {
            out.push(value[..value.len() / 2].to_vec());
            out.push(value[value.len() / 2..].to_vec());
        }
        for index in 0..value.len() {
            let mut shorter = value.clone();
            shorter.remove(index);
            out.push(shorter);
        }
        for index in 0..value.len() {
            for smaller in self.element.shrink(&value[index]) {
                let mut changed = value.clone();
                changed[index] = smaller;
                out.push(changed);
            }
        }
        out
    }
}

/// Strings over an alphabet, up to `max_len` characters, shrinking like [`VecOf`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Text<'a> {
    /// The characters to draw from; the first is the simplest.
    pub alphabet: &'a [char],
    /// Longest string generated.
    pub max_len: usize,
}

impl Gen for Text<'_> {
    type Value = String;

    /// An empty alphabet yields only the empty string.
    fn generate(&self, rng: &mut Rng) -> String {
        let Some(element) = Elements::new(self.alphabet) else {
            return String::new();
        };
        VecOf {
            element,
            max_len: self.max_len,
        }
        .generate(rng)
        .into_iter()
        .collect()
    }

    fn shrink(&self, value: &String) -> Vec<String> {
        let Some(element) = Elements::new(self.alphabet) else {
            return Vec::new();
        };
        let chars: Vec<char> = value.chars().collect();
        VecOf {
            element,
            max_len: self.max_len,
        }
        .shrink(&chars)
        .into_iter()
        .map(|chars| chars.into_iter().collect())
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        check, check_with, Config, Elements, Failure, Gen, Text, VecOf, DEFAULT_MASTER_SEED, U64,
    };
    use crate::prng::Rng;

    #[test]
    fn config_reads_cases_and_seed_with_defaults() {
        assert_eq!(Config::from_vars(None, None), Config::new(256));
        assert_eq!(Config::from_vars(Some("4096"), None).cases, 4096);
        assert_eq!(
            Config::from_vars(Some(" 12 "), Some(" 99 ")),
            Config {
                cases: 12,
                seed: Some(99),
                max_shrinks: 10_000
            }
        );
        assert_eq!(Config::from_vars(Some("many"), Some("x")), Config::new(256));
        assert_eq!(
            Config::replay(5),
            Config {
                cases: 1,
                seed: Some(5),
                max_shrinks: 10_000
            }
        );
        assert_eq!(Config::from_env().max_shrinks, 10_000);
    }

    #[test]
    fn a_passing_property_runs_every_case() {
        let gen = U64 { lo: 0, hi: 9 };
        let cell = std::cell::Cell::new(0u32);
        assert_eq!(
            check_with(&Config::new(37), &gen, &|_| {
                cell.set(cell.get() + 1);
                Ok(())
            }),
            Ok(())
        );
        assert_eq!(cell.get(), 37);
        check(&gen, under_ten);
        assert_eq!(
            check_with(&Config::new(0), &gen, &under_ten),
            Ok(()),
            "zero cases pass trivially"
        );
    }

    #[test]
    fn a_failing_property_is_shrunk_to_the_minimal_counter_example() {
        let gen = U64 { lo: 0, hi: 1000 };
        let failure = check_with(&Config::new(100), &gen, &|v| {
            if *v < 10 {
                Ok(())
            } else {
                Err(format!("{v} is too big"))
            }
        })
        .expect_err("values of 10 or more fail");
        assert_eq!(failure.minimal, "10");
        assert_eq!(failure.message, "10 is too big");
        assert!(failure.shrinks >= 1, "{failure:?}");
        assert!(failure.original.parse::<u64>().expect("a number") >= 10);
        let text = failure.to_string();
        assert!(text.starts_with(&format!(
            "property failed (case {}, replay with SJS_PROP_SEED={})",
            failure.case, failure.seed
        )));
        assert!(
            text.contains("minimal: 10") && text.contains("message: 10 is too big"),
            "{text}"
        );
    }

    #[test]
    fn replaying_the_printed_seed_reproduces_the_failure_exactly() {
        let gen = U64 {
            lo: 0,
            hi: 1_000_000,
        };
        let property = |v: &u64| {
            if *v % 7 == 3 {
                Err("seven".to_owned())
            } else {
                Ok(())
            }
        };
        let first =
            check_with(&Config::new(2000), &gen, &property).expect_err("some value is 3 mod 7");
        let replay =
            check_with(&Config::replay(first.seed), &gen, &property).expect_err("same value");
        assert_eq!(replay.original, first.original);
        assert_eq!(replay.minimal, first.minimal);
        assert_eq!(replay.case, 0);
        assert_eq!(replay.seed, first.seed);
        let other = check_with(&Config::replay(first.seed.wrapping_add(1)), &gen, &|_| {
            Ok(())
        });
        assert_eq!(other, Ok(()));
    }

    #[test]
    fn the_default_master_seed_makes_runs_reproducible() {
        let gen = U64 {
            lo: 0,
            hi: u64::MAX,
        };
        let a = std::cell::RefCell::new(Vec::new());
        let b = std::cell::RefCell::new(Vec::new());
        check_with(&Config::new(5), &gen, &|v| {
            a.borrow_mut().push(*v);
            Ok(())
        })
        .expect("passes");
        check_with(&Config::new(5), &gen, &|v| {
            b.borrow_mut().push(*v);
            Ok(())
        })
        .expect("passes");
        assert_eq!(*a.borrow(), *b.borrow());
        assert_eq!(a.borrow().len(), 5);
        assert_eq!(DEFAULT_MASTER_SEED, 0x5EED_0000_0000_0001);
    }

    #[test]
    fn a_panicking_property_counts_as_a_failure() {
        let gen = U64 { lo: 0, hi: 5 };
        let failure = check_with(&Config::new(10), &gen, &|v| {
            assert!(*v >= 3, "value {v} is small");
            Ok(())
        })
        .expect_err("small values panic");
        assert_eq!(failure.minimal, "0");
        assert_eq!(failure.message, "value 0 is small");
        let str_panic =
            check_with(&Config::new(1), &gen, &|_| panic!("plain")).expect_err("panics");
        assert_eq!(str_panic.message, "plain");
        let odd_payload = check_with(&Config::new(1), &gen, &|_| std::panic::panic_any(42_u8))
            .expect_err("panics");
        assert_eq!(odd_payload.message, "panic with a non-string payload");
    }

    #[test]
    fn shrinking_stops_at_the_step_limit() {
        let gen = U64 { lo: 0, hi: 1000 };
        let config = Config {
            cases: 5,
            seed: None,
            max_shrinks: 1,
        };
        let failure = check_with(&config, &gen, &|v| {
            if *v < 10 {
                Ok(())
            } else {
                Err("big".to_owned())
            }
        })
        .expect_err("fails");
        assert_eq!(failure.shrinks, 1);
    }

    #[test]
    fn u64_shrinks_toward_its_low_end_without_duplicates() {
        let gen = U64 { lo: 10, hi: 100 };
        assert_eq!(gen.shrink(&10), Vec::<u64>::new());
        assert_eq!(gen.shrink(&11), [10]);
        assert_eq!(gen.shrink(&12), [10, 11]);
        assert_eq!(gen.shrink(&50), [10, 30, 49]);
        assert_eq!(U64 { lo: 0, hi: 0 }.shrink(&0), Vec::<u64>::new());
        let mut rng = Rng::from_seed(3);
        for _ in 0..100 {
            let v = gen.generate(&mut rng);
            assert!((10..=100).contains(&v));
        }
    }

    #[test]
    fn elements_shrink_toward_the_first_item() {
        let gen = Elements::new(&["a", "b", "c", "d"]).expect("non-empty");
        assert_eq!(gen.shrink(&"a"), Vec::<&str>::new());
        assert_eq!(gen.shrink(&"b"), ["a"]);
        assert_eq!(gen.shrink(&"d"), ["a", "c"]);
        assert_eq!(
            gen.shrink(&"zzz"),
            Vec::<&str>::new(),
            "an unknown value has nothing simpler"
        );
        let mut rng = Rng::from_seed(3);
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..200 {
            seen.insert(gen.generate(&mut rng));
        }
        assert_eq!(seen.len(), 4);
        let empty: &[u8] = &[];
        assert_eq!(
            Elements::new(empty),
            None,
            "an empty Elements cannot generate"
        );
    }

    #[test]
    fn vec_of_shrinks_to_a_minimal_failing_vector() {
        let gen = VecOf {
            element: U64 { lo: 0, hi: 3 },
            max_len: 8,
        };
        let failure = check_with(&Config::new(200), &gen, &|v: &Vec<u64>| {
            if v.iter().sum::<u64>() < 5 {
                Ok(())
            } else {
                Err("sum".to_owned())
            }
        })
        .expect_err("some vector sums to 5 or more");
        let minimal: Vec<u64> = failure
            .minimal
            .trim_matches(['[', ']'])
            .split(", ")
            .map(|n| n.parse().expect("number"))
            .collect();
        assert_eq!(minimal.len(), 2, "{failure:?}");
        assert_eq!(minimal.iter().sum::<u64>(), 5, "{failure:?}");
        assert_eq!(gen.shrink(&vec![]), Vec::<Vec<u64>>::new());
        assert_eq!(gen.shrink(&vec![0]), vec![vec![]]);
        assert_eq!(
            gen.shrink(&vec![1, 2]),
            vec![
                vec![1],
                vec![2],
                vec![2],
                vec![1],
                vec![0, 2],
                vec![1, 0],
                vec![1, 1]
            ]
        );
        let mut rng = Rng::from_seed(9);
        let lens: std::collections::BTreeSet<usize> =
            (0..300).map(|_| gen.generate(&mut rng).len()).collect();
        assert!(lens.contains(&0) && lens.contains(&8) && !lens.contains(&9));
    }

    #[test]
    fn text_generates_over_its_alphabet_and_shrinks_like_a_vector() {
        let gen = Text {
            alphabet: &['a', 'é', '\n'],
            max_len: 6,
        };
        let mut rng = Rng::from_seed(11);
        for _ in 0..200 {
            let s = gen.generate(&mut rng);
            assert!(s.chars().count() <= 6);
            assert!(s.chars().all(|c| ['a', 'é', '\n'].contains(&c)), "{s:?}");
        }
        assert_eq!(gen.shrink(&String::new()), Vec::<String>::new());
        assert_eq!(
            gen.shrink(&"é".to_owned()),
            vec![String::new(), "a".to_owned()]
        );
        let empty = Text {
            alphabet: &[],
            max_len: 3,
        };
        assert_eq!(empty.generate(&mut rng), "");
        assert_eq!(empty.shrink(&"x".to_owned()), Vec::<String>::new());
        let failure = check_with(&Config::new(300), &gen, &|s: &String| {
            if s.contains('\n') {
                Err("newline".to_owned())
            } else {
                Ok(())
            }
        })
        .expect_err("some string has a newline");
        assert_eq!(failure.minimal, "\"\\n\"");
    }

    #[test]
    fn failure_displays_every_field() {
        let failure = Failure {
            seed: 7,
            case: 2,
            original: "[1, 2]".to_owned(),
            minimal: "[2]".to_owned(),
            message: "nope".to_owned(),
            shrinks: 3,
        };
        assert_eq!(
            failure.to_string(),
            "property failed (case 2, replay with SJS_PROP_SEED=7)\n  minimal: [2]\n  message: nope\n  original: [1, 2] (3 shrink steps)"
        );
    }

    // A property receives `&V` by contract, however small `V` is.
    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn under_ten(v: &u64) -> Result<(), String> {
        if *v < 10 {
            Ok(())
        } else {
            Err(format!("{v} is not under ten"))
        }
    }

    /// `under_ten` is the one property every `check` call uses, so a single instantiation
    /// of `check` sees both its passing and its failing path: the coverage gate counts the
    /// best instantiation of a generic function.
    #[test]
    fn check_panics_with_the_failure_text() {
        let caught = std::panic::catch_unwind(|| check(&U64 { lo: 10, hi: 10 }, under_ten));
        let payload = caught.expect_err("check panics on failure");
        let text = payload.downcast_ref::<String>().expect("a String payload");
        assert!(
            text.contains("minimal: 10") && text.contains("message: 10 is not under ten"),
            "{text}"
        );
    }
}
