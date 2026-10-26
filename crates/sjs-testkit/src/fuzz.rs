//! A fuzz loop on `std` alone (STORIES.md §0.2 item 7): random and mutated inputs for a
//! time budget, with a dictionary of interesting tokens and a directory of saved
//! regressions that is replayed first. A target that panics is a crash; its input is
//! saved so the next run replays it.

use std::fmt;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::prng::Rng;

/// How long and how to fuzz.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// The time budget.
    pub budget: Duration,
    /// The seed of the input stream.
    pub seed: u64,
    /// Longest generated input, in bytes.
    pub max_len: usize,
    /// Tokens worth inserting (keywords, operators, markers).
    pub dictionary: Vec<Vec<u8>>,
    /// Where crashing inputs are saved and replayed from.
    pub regressions: Option<PathBuf>,
}

impl Config {
    /// A budget in seconds with the default seed and no dictionary.
    #[must_use]
    pub fn seconds(seconds: f64) -> Self {
        Self {
            budget: Duration::from_secs_f64(seconds.max(0.0)),
            seed: 0xF022_0000_0000_0001,
            max_len: 256,
            dictionary: Vec::new(),
            regressions: None,
        }
    }

    /// Reads `SJS_FUZZ_SECONDS` (default 1).
    #[must_use]
    pub fn from_env() -> Self {
        Self::from_var(std::env::var("SJS_FUZZ_SECONDS").ok().as_deref())
    }

    /// Builds from the variable's value; an unparsable value means one second.
    #[must_use]
    pub fn from_var(value: Option<&str>) -> Self {
        Self::seconds(value.and_then(|v| v.trim().parse().ok()).unwrap_or(1.0))
    }
}

/// A target panicked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Crash {
    /// The input that made it panic.
    pub input: Vec<u8>,
    /// The panic message.
    pub message: String,
    /// Where the input was saved, when a regressions directory was configured.
    pub saved: Option<PathBuf>,
    /// True when the input came from the regressions directory rather than being new.
    pub replayed: bool,
}

impl fmt::Display for Crash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let saved = self.saved.as_ref().map_or_else(String::new, |path| {
            format!(
                "
  saved at {}",
                path.display()
            )
        });
        let replayed = if self.replayed {
            "
  (replayed from the regressions directory)"
        } else {
            ""
        };
        write!(
            f,
            "fuzz target panicked: {}
  input ({} bytes): {:?}{saved}{replayed}",
            self.message,
            self.input.len(),
            self.input
        )
    }
}

impl std::error::Error for Crash {}

/// What a crash-free run did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// Inputs tried, replays included.
    pub runs: u64,
    /// Regressions replayed.
    pub replayed: u64,
    /// Inputs kept as mutation bases at the end (at most 64).
    pub corpus: usize,
    /// Wall time spent.
    pub elapsed: Duration,
}

/// Replays saved regressions, then feeds `target` random and mutated inputs until the
/// budget is spent.
///
/// # Errors
///
/// The first crash, saved to the regressions directory when one is configured.
pub fn run(config: &Config, mut target: impl FnMut(&[u8])) -> Result<Report, Crash> {
    let start = Instant::now();
    let mut runs = 0;
    let mut replayed = 0;
    for input in saved_inputs(config.regressions.as_deref()) {
        runs += 1;
        replayed += 1;
        if let Err(message) = attempt(&mut target, &input) {
            let saved = config
                .regressions
                .as_deref()
                .map(|dir| dir.join(file_name(&input)));
            return Err(Crash {
                input,
                message,
                saved,
                replayed: true,
            });
        }
    }
    let mut rng = Rng::from_seed(config.seed);
    let mut corpus: Vec<Vec<u8>> = Vec::new();
    while start.elapsed() < config.budget {
        let input = next_input(&mut rng, config, &corpus);
        runs += 1;
        if let Err(message) = attempt(&mut target, &input) {
            let saved = config
                .regressions
                .as_deref()
                .and_then(|dir| save(dir, &input));
            return Err(Crash {
                input,
                message,
                saved,
                replayed: false,
            });
        }
        if corpus.len() < 64 {
            corpus.push(input);
        } else {
            let slot = rng.index(corpus.len());
            corpus[slot] = input;
        }
    }
    Ok(Report {
        runs,
        replayed,
        corpus: corpus.len(),
        elapsed: start.elapsed(),
    })
}

/// [`run`] with the environment's budget, panicking on a crash.
///
/// # Panics
///
/// On a crash.
pub fn run_or_panic(
    regressions: Option<&Path>,
    dictionary: &[&[u8]],
    target: impl FnMut(&[u8]),
) -> Report {
    let mut config = Config::from_env();
    config.regressions = regressions.map(Path::to_path_buf);
    config.dictionary = dictionary.iter().map(|token| token.to_vec()).collect();
    match run(&config, target) {
        Ok(report) => report,
        Err(crash) => panic!("{crash}"),
    }
}

fn attempt(target: &mut impl FnMut(&[u8]), input: &[u8]) -> Result<(), String> {
    catch_unwind(AssertUnwindSafe(|| target(input))).map_err(|payload| {
        payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_owned()))
            .unwrap_or_else(|| "panic with a non-string payload".to_owned())
    })
}

fn next_input(rng: &mut Rng, config: &Config, corpus: &[Vec<u8>]) -> Vec<u8> {
    let fresh_len = rng.index(config.max_len + 1);
    let Some(base) = rng.pick(corpus) else {
        return rng.bytes(fresh_len);
    };
    let strategy = rng.below(8);
    let other = rng.pick(corpus).cloned().unwrap_or_default();
    mutate(rng, strategy, fresh_len, base, &other, &config.dictionary)
}

/// One mutation of `base`, chosen by `strategy` (0–7; anything else truncates).
fn mutate(
    rng: &mut Rng,
    strategy: u64,
    fresh_len: usize,
    base: &[u8],
    other: &[u8],
    dictionary: &[Vec<u8>],
) -> Vec<u8> {
    match strategy {
        0 => rng.bytes(fresh_len),
        1 => flip_bit(rng, base),
        2 => set_byte(rng, base),
        3 => insert_byte(rng, base),
        4 => delete_byte(rng, base),
        5 => splice(rng, base, other),
        6 => insert_token(rng, base, dictionary),
        _ => truncate(rng, base),
    }
}

fn flip_bit(rng: &mut Rng, base: &[u8]) -> Vec<u8> {
    let mut out = base.to_vec();
    if !out.is_empty() {
        let index = rng.index(out.len());
        out[index] ^= 1 << rng.below(8);
    }
    out
}

fn set_byte(rng: &mut Rng, base: &[u8]) -> Vec<u8> {
    let mut out = base.to_vec();
    if !out.is_empty() {
        let index = rng.index(out.len());
        out[index] = u8::try_from(rng.below(256)).unwrap_or(u8::MAX);
    }
    out
}

fn insert_byte(rng: &mut Rng, base: &[u8]) -> Vec<u8> {
    let mut out = base.to_vec();
    let index = rng.index(out.len() + 1);
    out.insert(index, u8::try_from(rng.below(256)).unwrap_or(u8::MAX));
    out
}

fn delete_byte(rng: &mut Rng, base: &[u8]) -> Vec<u8> {
    let mut out = base.to_vec();
    if !out.is_empty() {
        let index = rng.index(out.len());
        out.remove(index);
    }
    out
}

fn splice(rng: &mut Rng, base: &[u8], other: &[u8]) -> Vec<u8> {
    let cut_a = rng.index(base.len() + 1);
    let cut_b = rng.index(other.len() + 1);
    let mut out = base[..cut_a].to_vec();
    out.extend_from_slice(&other[cut_b..]);
    out
}

fn insert_token(rng: &mut Rng, base: &[u8], dictionary: &[Vec<u8>]) -> Vec<u8> {
    let mut out = base.to_vec();
    if let Some(token) = rng.pick(dictionary) {
        let index = rng.index(out.len() + 1);
        out.splice(index..index, token.iter().copied());
    }
    out
}

fn truncate(rng: &mut Rng, base: &[u8]) -> Vec<u8> {
    let mut out = base.to_vec();
    out.truncate(rng.index(out.len() + 1));
    out
}

/// FNV-1a over the input, as a file name.
fn file_name(input: &[u8]) -> String {
    let mut hash: u64 = 0xCBF2_9CE4_8422_2325;
    for byte in input {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01B3);
    }
    format!("{hash:016x}.bin")
}

fn save(dir: &Path, input: &[u8]) -> Option<PathBuf> {
    let path = dir.join(file_name(input));
    std::fs::create_dir_all(dir).ok()?;
    std::fs::write(&path, input).ok()?;
    Some(path)
}

/// Every `*.bin` in `dir`, sorted by name; nothing when there is no directory.
fn saved_inputs(dir: Option<&Path>) -> Vec<Vec<u8>> {
    let Some(dir) = dir else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "bin"))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .filter_map(|path| std::fs::read(path).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{file_name, mutate, next_input, run, run_or_panic, Config, Crash};
    use crate::fs::TempDir;
    use crate::prng::Rng;
    use std::time::Duration;

    /// The one target every `run` test uses, so a single instantiation of `run` (and of
    /// `attempt`) sees every path: the coverage gate counts the best instantiation of a
    /// generic function. Random bytes never spell the token; the dictionary does.
    fn crash_on_token(input: &[u8]) {
        assert!(!input.windows(5).any(|w| w == b"CRASH"), "found the token");
    }

    #[test]
    fn config_reads_the_budget_from_the_variable() {
        assert_eq!(Config::from_var(None).budget, Duration::from_secs(1));
        assert_eq!(
            Config::from_var(Some("0.25")).budget,
            Duration::from_secs_f64(0.25)
        );
        assert_eq!(Config::from_var(Some(" 2 ")).budget, Duration::from_secs(2));
        assert_eq!(
            Config::from_var(Some("soon")).budget,
            Duration::from_secs(1)
        );
        assert_eq!(
            Config::seconds(-5.0).budget,
            Duration::ZERO,
            "a negative budget is zero"
        );
        assert!(Config::from_env().budget >= Duration::ZERO);
    }

    #[test]
    fn the_time_budget_is_respected_within_ten_percent() {
        let config = Config::seconds(0.5);
        let report = run(&config, crash_on_token).expect("no crash");
        assert!(report.elapsed >= Duration::from_millis(500), "{report:?}");
        assert!(report.elapsed <= Duration::from_millis(550), "{report:?}");
        assert!(report.runs > 1000, "{report:?}");
        assert_eq!((report.replayed, report.corpus), (0, 64));
        assert!(
            run(&Config::seconds(0.02), crash_on_token)
                .expect("no crash")
                .runs
                > 0
        );
        let none = run(&Config::seconds(0.0), crash_on_token).expect("no crash");
        assert_eq!((none.runs, none.corpus), (0, 0));
    }

    #[test]
    fn inputs_vary_in_length_and_content_and_stay_within_max_len() {
        let mut config = Config::seconds(0.2);
        config.max_len = 16;
        config.dictionary = vec![b"TOKEN".to_vec()];
        let lens = std::cell::RefCell::new(std::collections::BTreeSet::new());
        let saw_token = std::cell::Cell::new(false);
        run(&config, |input| {
            lens.borrow_mut().insert(input.len());
            if input.windows(5).any(|w| w == b"TOKEN") {
                saw_token.set(true);
            }
        })
        .expect("no crash");
        let lens = lens.borrow();
        assert!(lens.contains(&0) && lens.contains(&16), "{lens:?}");
        assert!(
            lens.iter().any(|&l| l > 16),
            "insertion and tokens grow inputs past max_len"
        );
        assert!(saw_token.get(), "dictionary tokens are inserted");
    }

    /// Each strategy has an observable signature; a deleted match arm falls through to
    /// `truncate`, which every signature below rules out.
    #[test]
    fn each_mutation_strategy_has_its_signature() {
        let base: Vec<u8> = (10..26).collect();
        let other: Vec<u8> = (100..110).collect();
        let dictionary = vec![b"TOKEN".to_vec()];
        // Over many seeds every position must be reachable, including the ends: a
        // mutation that never inserts at the end, never keeps the whole base or never
        // takes the whole other input would be a weaker fuzzer.
        let mut saw = [false; 9];
        for seed in 0..60u64 {
            let mut rng = Rng::from_seed(seed);
            let fresh = mutate(&mut rng, 0, 7, &base, &other, &dictionary);
            assert_eq!(fresh.len(), 7, "0: fresh bytes of the requested length");
            let flipped = mutate(&mut rng, 1, 7, &base, &other, &dictionary);
            assert_eq!(flipped.len(), base.len(), "1: flip keeps the length");
            let differing_bits: u32 = flipped
                .iter()
                .zip(&base)
                .map(|(a, b)| (a ^ b).count_ones())
                .sum();
            assert_eq!(differing_bits, 1, "1: exactly one bit flips");
            let set = mutate(&mut rng, 2, 7, &base, &other, &dictionary);
            assert_eq!(set.len(), base.len(), "2: set keeps the length");
            assert!(
                set.iter().zip(&base).filter(|(a, b)| a != b).count() <= 1,
                "2: at most one byte changes"
            );
            let inserted = mutate(&mut rng, 3, 7, &base, &other, &dictionary);
            assert_eq!(inserted.len(), base.len() + 1, "3: one byte inserted");
            assert!(is_subsequence(&base, &inserted), "3: the base survives");
            saw[0] |= inserted.starts_with(&base);
            saw[1] |= inserted.ends_with(&base);
            let deleted = mutate(&mut rng, 4, 7, &base, &other, &dictionary);
            assert_eq!(deleted.len(), base.len() - 1, "4: one byte deleted");
            assert!(
                is_subsequence(&deleted, &base),
                "4: a subsequence of the base"
            );
            let spliced = mutate(&mut rng, 5, 7, &base, &other, &dictionary);
            let cut = spliced.iter().take_while(|b| **b < 100).count();
            assert_eq!(&spliced[..cut], &base[..cut], "5: a prefix of the base");
            assert!(
                other.ends_with(&spliced[cut..]),
                "5: then a suffix of the other"
            );
            saw[2] |= cut == base.len();
            saw[3] |= spliced.len() - cut == other.len();
            saw[4] |= cut < base.len() && spliced.len() > cut;
            saw[8] |= spliced.len() == cut && cut < base.len();
            let with_token = mutate(&mut rng, 6, 7, &base, &other, &dictionary);
            assert_eq!(with_token.len(), base.len() + 5, "6: the token is inserted");
            assert!(
                with_token.windows(5).any(|w| w == b"TOKEN"),
                "6: the token is present"
            );
            saw[5] |= with_token.ends_with(b"TOKEN");
            let truncated = mutate(&mut rng, 7, 7, &base, &other, &dictionary);
            assert!(
                truncated.len() <= base.len() && base.starts_with(&truncated),
                "7: a prefix"
            );
            saw[6] |= truncated == base;
            saw[7] |= truncated.len() < base.len();
            assert!(
                base.starts_with(&mutate(&mut rng, 99, 7, &base, &other, &dictionary)),
                "unknown: truncate"
            );
        }
        assert_eq!(
            saw,
            [true; 9],
            "insert at both ends; splice keeping all of base, all of other, a real mix, and none of other; token at the end; truncate to full and to shorter"
        );
        let mut rng = Rng::from_seed(1);
        let empty: Vec<u8> = Vec::new();
        for strategy in 0..8 {
            let out = mutate(&mut rng, strategy, 0, &empty, &empty, &[]);
            assert!(
                out.len() <= 1,
                "{strategy}: an empty base yields at most one byte: {out:?}"
            );
        }
        assert_eq!(
            mutate(&mut rng, 6, 0, &base, &other, &[]),
            base,
            "no dictionary: unchanged"
        );
    }

    fn is_subsequence(needle: &[u8], haystack: &[u8]) -> bool {
        let mut it = haystack.iter();
        needle.iter().all(|n| it.any(|h| h == n))
    }

    #[test]
    fn fresh_inputs_span_zero_to_max_len_exactly() {
        let mut config = Config::seconds(0.0);
        config.max_len = 3;
        let mut rng = Rng::from_seed(2);
        let lens: std::collections::BTreeSet<usize> = (0..300)
            .map(|_| next_input(&mut rng, &config, &[]).len())
            .collect();
        assert_eq!(lens.into_iter().collect::<Vec<_>>(), [0, 1, 2, 3]);
        let corpus = vec![vec![1, 2, 3]];
        for _ in 0..200 {
            assert!(
                next_input(&mut rng, &config, &corpus).len() <= 3 + 1 + 1,
                "mutations stay small"
            );
        }
    }

    #[test]
    fn a_crash_is_reported_saved_and_replayed() {
        let dir = TempDir::new("fuzz").expect("temp dir");
        let regressions = dir.path().join("regressions");
        let mut config = Config::seconds(5.0);
        config.dictionary = vec![b"CRASH".to_vec()];
        config.regressions = Some(regressions.clone());
        let crash =
            run(&config, crash_on_token).expect_err("the dictionary makes the crash reachable");
        assert!(crash.input.windows(5).any(|w| w == b"CRASH"), "{crash:?}");
        assert_eq!(crash.message, "found the token");
        assert!(!crash.replayed);
        let saved = crash.saved.clone().expect("saved");
        assert_eq!(saved, regressions.join(file_name(&crash.input)));
        assert_eq!(std::fs::read(&saved).expect("file"), crash.input);
        let text = crash.to_string();
        assert!(
            text.starts_with("fuzz target panicked: found the token\n  input (")
                && text.contains("saved at"),
            "{text}"
        );

        let replay = run(&config, crash_on_token).expect_err("the saved input is replayed first");
        assert_eq!(replay.input, crash.input);
        assert!(replay.replayed);
        assert_eq!(replay.saved, Some(saved.clone()));
        assert!(replay
            .to_string()
            .ends_with("(replayed from the regressions directory)"));

        // A regression that no longer crashes is replayed and passes.
        std::fs::remove_file(&saved).expect("remove");
        std::fs::write(regressions.join("harmless.bin"), b"nothing here").expect("write");
        std::fs::write(regressions.join("notes.txt"), "ignored").expect("write");
        let mut quiet = Config::seconds(0.0);
        quiet.regressions = Some(regressions.clone());
        let report = run(&quiet, crash_on_token).expect("a harmless regression passes");
        assert_eq!(
            (report.runs, report.replayed),
            (1, 1),
            "only .bin files replay"
        );
    }

    #[test]
    fn a_crash_without_a_regressions_directory_is_still_reported() {
        let mut config = Config::seconds(5.0);
        config.dictionary = vec![b"CRASH".to_vec()];
        let crash = run(&config, crash_on_token).expect_err("crash");
        assert_eq!(crash.saved, None);
        assert!(!crash.to_string().contains("saved at"));
        let dir = TempDir::new("fuzz").expect("temp dir");
        let mut unwritable = config.clone();
        unwritable.regressions = Some(dir.write("file", "").expect("write"));
        let crash = run(&unwritable, crash_on_token).expect_err("crash");
        assert_eq!(crash.saved, None, "saving under a file fails quietly");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let locked_dir = dir.mkdir("locked").expect("mkdir");
            std::fs::set_permissions(&locked_dir, std::fs::Permissions::from_mode(0o555))
                .expect("chmod");
            let mut read_only = config.clone();
            read_only.regressions = Some(locked_dir.clone());
            let crash = run(&read_only, crash_on_token).expect_err("crash");
            std::fs::set_permissions(&locked_dir, std::fs::Permissions::from_mode(0o755))
                .expect("chmod back");
            assert_eq!(
                crash.saved, None,
                "a directory that refuses the write is reported without a path"
            );
        }
        let str_payload = run(&config, |_| panic!("plain")).expect_err("crash");
        assert_eq!(str_payload.message, "plain");
        let odd = run(&config, |_| std::panic::panic_any(7_u8)).expect_err("crash");
        assert_eq!(odd.message, "panic with a non-string payload");
    }

    #[test]
    fn run_or_panic_reports_and_panics() {
        let report = run_or_panic(None, &[], crash_on_token);
        assert!(report.elapsed >= Duration::ZERO);
        let caught = std::panic::catch_unwind(|| run_or_panic(None, &[b"CRASH"], crash_on_token));
        let payload = caught.expect_err("panics");
        assert!(payload
            .downcast_ref::<String>()
            .expect("String")
            .starts_with("fuzz target panicked"));
    }

    #[test]
    fn file_names_are_fnv1a_hashes() {
        assert_eq!(file_name(b""), "cbf29ce484222325.bin");
        assert_eq!(file_name(b"a"), "af63dc4c8601ec8c.bin");
        assert_ne!(file_name(b"ab"), file_name(b"ba"));
        let crash = Crash {
            input: vec![1],
            message: "m".to_owned(),
            saved: None,
            replayed: false,
        };
        assert_eq!(
            crash.to_string(),
            "fuzz target panicked: m\n  input (1 bytes): [1]"
        );
    }
}
