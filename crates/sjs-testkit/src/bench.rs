//! Benchmark timing on `std::time::Instant` for `harness = false` benches
//! (STORIES.md §0.3): the median of repeated runs, with min and max for context.

use std::fmt;
use std::time::{Duration, Instant};

/// Timings of one benchmark.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Measurement {
    /// What was measured.
    pub name: String,
    /// Runs timed (at least one).
    pub iterations: u32,
    /// Fastest run.
    pub min: Duration,
    /// Median run: the figure to compare between commits.
    pub median: Duration,
    /// Slowest run.
    pub max: Duration,
}

impl fmt::Display for Measurement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: median {:?} (min {:?}, max {:?}) over {} runs",
            self.name, self.median, self.min, self.max, self.iterations
        )
    }
}

/// Runs `f` `iterations` times (at least once) and reports the timings.
pub fn time(name: &str, iterations: u32, mut f: impl FnMut()) -> Measurement {
    let iterations = iterations.max(1);
    let mut samples: Vec<Duration> = (0..iterations)
        .map(|_| {
            let start = Instant::now();
            f();
            start.elapsed()
        })
        .collect();
    samples.sort();
    let median = samples[samples.len() / 2];
    Measurement {
        name: name.to_owned(),
        iterations,
        min: samples[0],
        median,
        max: samples[samples.len() - 1],
    }
}

/// One line per measurement.
#[must_use]
pub fn report(measurements: &[Measurement]) -> String {
    let mut out = String::new();
    for measurement in measurements {
        out.push_str(&measurement.to_string());
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{report, time, Measurement};
    use std::time::Duration;

    #[test]
    fn times_every_iteration_and_orders_the_statistics() {
        let mut calls = 0;
        let m = time("spin", 5, || {
            calls += 1;
            std::thread::sleep(Duration::from_millis(1));
        });
        assert_eq!(calls, 5);
        assert_eq!((m.name.as_str(), m.iterations), ("spin", 5));
        assert!(m.min >= Duration::from_millis(1), "{m}");
        assert!(m.min <= m.median && m.median <= m.max, "{m}");
    }

    #[test]
    fn zero_iterations_still_runs_once() {
        let mut calls = 0;
        let m = time("once", 0, || calls += 1);
        assert_eq!((calls, m.iterations), (1, 1));
        assert_eq!((m.min, m.median, m.max), (m.min, m.min, m.min));
    }

    #[test]
    fn display_and_report() {
        let m = Measurement {
            name: "lex".to_owned(),
            iterations: 3,
            min: Duration::from_millis(1),
            median: Duration::from_millis(2),
            max: Duration::from_millis(3),
        };
        assert_eq!(
            m.to_string(),
            "lex: median 2ms (min 1ms, max 3ms) over 3 runs"
        );
        assert_eq!(report(&[m.clone(), m]), "lex: median 2ms (min 1ms, max 3ms) over 3 runs\nlex: median 2ms (min 1ms, max 3ms) over 3 runs\n");
        assert_eq!(report(&[]), "");
    }
}
