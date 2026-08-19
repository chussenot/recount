//! The vocabulary every other module speaks: transcript turns, session
//! metadata, time windows, and the confidence/evidence pair that the spec
//! demands of any answer.
//!
//! This module is REAL. Everything else in the tree is a declared seam with a
//! `todo!()` body; these types are not, because every other bead compiles
//! against them and a stub here would make the whole plan uncompilable.
//!
//! # Decisions recorded here
//!
//! **Why `SessionMeta::cwd` is an `Option<PathBuf>`.** A session may never
//! state one. `~/.claude/projects/<encoded-path>/` encodes the directory in
//! the *directory name*, but individual transcript records are not obliged to
//! repeat it, and `generic_jsonl` — "any dir of *.jsonl with {ts|timestamp,
//! role, content|text}" — has no notion of a working directory at all. The
//! join's whole key is topological (`invoked_from -> worktree path -> session
//! cwd`), so a missing cwd is not a parse failure, it is a session the join
//! must refuse with "no cwd match" rather than guess about. Making the absence
//! representable is what lets that refusal be a first-class answer.
//!
//! **Why timestamps are `chrono::DateTime<Utc>` and not strings or i64.** The
//! join requires time-window overlap, and pact's own `.pact/events.jsonl`
//! writes RFC 3339 with a numeric offset (`2026-08-19T07:03:12.694912822+00:00`),
//! while transcripts write their own spellings. Normalising to a single
//! absolute instant at the parse boundary means comparison is `<`, not string
//! juggling, and an offset that differs between the ledger and the transcript
//! cannot silently become a false non-overlap. `Utc` specifically, not `Local`:
//! an audit read on a different machine, or after a DST change, must reach the
//! same verdict. chrono is already in the dependency budget.

use std::fmt;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

/// Who spoke a turn.
///
/// `Other` keeps the raw string rather than dropping it: connectors are
/// reverse-engineered ("expect drift"), and an unrecognised role is evidence
/// about the format, not a parse failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum Role {
    User,
    Assistant,
    System,
    Other(String),
}

impl Role {
    pub fn as_str(&self) -> &str {
        match self {
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::System => "system",
            Role::Other(s) => s,
        }
    }
}

impl From<String> for Role {
    fn from(s: String) -> Self {
        match s.as_str() {
            "user" => Role::User,
            "assistant" => Role::Assistant,
            "system" => Role::System,
            _ => Role::Other(s),
        }
    }
}

impl From<Role> for String {
    fn from(r: Role) -> String {
        match r {
            Role::Other(s) => s,
            other => other.as_str().to_string(),
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One utterance in a transcript.
///
/// `line` is the 1-based line number of the record inside its `.jsonl` file —
/// the same base `pact_events` uses — because `grep` must emit "(session,
/// line, ts, role, snippet)" and a reader needs to be able to `sed -n Np` it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Turn {
    pub ts: DateTime<Utc>,
    pub role: Role,
    pub text: String,
    pub line: usize,
}

/// What a connector knows about one session file without you having to read it
/// again.
///
/// `parse_failures` is a count, never an error: the spec's tolerance contract
/// is "one bad line never fails a file", so failures are reported as a tally
/// that `doctor` can show rather than propagated as a `Result`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionMeta {
    pub path: PathBuf,
    /// See the module doc: a session may never state one.
    pub cwd: Option<PathBuf>,
    /// `None` when no turn in the file parsed — an empty or wholly garbage file.
    pub first_ts: Option<DateTime<Utc>>,
    pub last_ts: Option<DateTime<Utc>>,
    pub turn_count: usize,
    pub parse_failures: usize,
    /// Which connector produced this. `String` rather than `&'static str` so
    /// the type round-trips through serde without a lifetime.
    pub connector: String,
}

impl SessionMeta {
    /// The window this session actually covers, for overlap tests.
    pub fn window(&self) -> TimeWindow {
        TimeWindow {
            since: self.first_ts,
            until: self.last_ts,
        }
    }
}

/// A half-open-ish interval with optional ends. `None` means unbounded on that
/// side, which is how `--since`/`--until` arrive: either, both, or neither.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeWindow {
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
}

impl TimeWindow {
    /// `--ts T --window W` : the interval `[T - W, T + W]`.
    ///
    /// Half-width each side, not W forward: `at` asks "which session was
    /// driving this identity *at that moment*", and a clock skew of a few
    /// seconds between the ledger and the transcript can fall either way.
    pub fn around(ts: DateTime<Utc>, half_width: Duration) -> Self {
        TimeWindow {
            since: Some(ts - half_width),
            until: Some(ts + half_width),
        }
    }

    pub fn is_unbounded(&self) -> bool {
        self.since.is_none() && self.until.is_none()
    }

    pub fn contains(&self, ts: DateTime<Utc>) -> bool {
        self.since.is_none_or(|s| ts >= s) && self.until.is_none_or(|u| ts <= u)
    }

    /// Do the two intervals share at least one instant? Unbounded ends are
    /// treated as -inf / +inf.
    pub fn overlaps(&self, other: &TimeWindow) -> bool {
        let before = |a: Option<DateTime<Utc>>, b: Option<DateTime<Utc>>| match (a, b) {
            (Some(a), Some(b)) => a <= b,
            _ => true,
        };
        before(self.since, other.until) && before(other.since, self.until)
    }
}

/// `0.0 ..= 1.0`. Constructed clamped so no scorer can leak a value the digest
/// would print as "137% confident".
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Confidence(f64);

impl Confidence {
    pub fn new(v: f64) -> Self {
        Confidence(if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) })
    }

    pub fn value(self) -> f64 {
        self.0
    }
}

impl fmt::Display for Confidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2}", self.0)
    }
}

/// The three signals the spec names for the join, and nothing else. A fourth
/// would be a change to the spec, not a new variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Signal {
    /// `invoked_from -> worktree path -> session cwd`.
    Topology,
    /// Required overlap between the event window and the session window.
    TimeOverlap,
    /// PACT_AGENT name or bead id mentioned in the transcript.
    TranscriptMention,
}

/// Why a score is what it is. The spec: "Scored match that always reports
/// confidence and evidence, never a bare answer" — so every scored result
/// carries a non-empty `Vec<Evidence>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    pub signal: Signal,
    /// Human-readable, e.g. "cwd /repo/wt/skeleton is worktree `skeleton`".
    pub detail: String,
}

impl Evidence {
    pub fn new(signal: Signal, detail: impl Into<String>) -> Self {
        Evidence {
            signal,
            detail: detail.into(),
        }
    }
}

/// `45m`, `2h`, `1d`, `2w`, `30s` — or a bare number, which is SECONDS.
///
/// Deliberately the same spellings `pact --ttl`/`--since` accept: `at
/// --window` is compared against a pact ledger, and two duration dialects in
/// one workflow is a bug generator.
pub fn parse_duration(s: &str) -> Result<Duration> {
    let s = s.trim();
    if s.is_empty() {
        bail!("empty duration");
    }
    let (num, unit) = match s.as_bytes()[s.len() - 1] {
        b'0'..=b'9' => (s, 's'),
        _ => (&s[..s.len() - 1], s.chars().next_back().unwrap()),
    };
    let n: i64 = num
        .parse()
        .with_context(|| format!("bad duration {s:?}: {num:?} is not a whole number"))?;
    if n < 0 {
        bail!("bad duration {s:?}: durations are not negative");
    }
    let d = match unit {
        's' => Duration::seconds(n),
        'm' => Duration::minutes(n),
        'h' => Duration::hours(n),
        'd' => Duration::days(n),
        'w' => Duration::weeks(n),
        u => bail!("bad duration {s:?}: unknown unit {u:?} (want s, m, h, d, w)"),
    };
    Ok(d)
}

/// An RFC 3339 instant. Strict: `--ts` is documented as rfc3339 and a silent
/// reinterpretation of an audit timestamp is the last thing this tool should do.
pub fn parse_ts(s: &str) -> Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(s.trim())
        .with_context(|| format!("bad timestamp {s:?}: want RFC 3339, e.g. 2026-08-19T07:03:12Z"))?
        .with_timezone(&Utc))
}

/// An RFC 3339 instant, or a duration meaning "that long ago".
///
/// `--since`/`--until` are window edges, not audit coordinates, and `--since
/// 2h` is how the ledger is read in practice.
pub fn parse_when(s: &str) -> Result<DateTime<Utc>> {
    match parse_ts(s) {
        Ok(ts) => Ok(ts),
        Err(ts_err) => match parse_duration(s) {
            Ok(d) => Ok(Utc::now() - d),
            Err(_) => Err(ts_err),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(s: &str) -> DateTime<Utc> {
        parse_ts(s).unwrap()
    }

    /// Every type in this module survives a serde_json round trip: the other
    /// beads emit these straight out of `--json`.
    #[test]
    fn round_trips_through_serde_json() {
        fn rt<T>(v: T)
        where
            T: Serialize + for<'de> Deserialize<'de> + PartialEq + fmt::Debug,
        {
            let s = serde_json::to_string(&v).unwrap();
            let back: T = serde_json::from_str(&s).unwrap();
            assert_eq!(v, back, "round trip changed the value (json was {s})");
        }

        for r in [
            Role::User,
            Role::Assistant,
            Role::System,
            Role::Other("tool_result".into()),
        ] {
            rt(r);
        }
        rt(Turn {
            ts: ts("2026-08-19T07:03:12.694912822+00:00"),
            role: Role::Assistant,
            text: "the arkanoid-v1 misdiagnosis".into(),
            line: 42,
        });
        rt(SessionMeta {
            path: PathBuf::from("/home/u/.claude/projects/-home-u-recount/a.jsonl"),
            cwd: Some(PathBuf::from("/home/u/recount")),
            first_ts: Some(ts("2026-08-19T07:00:00Z")),
            last_ts: Some(ts("2026-08-19T08:00:00Z")),
            turn_count: 12,
            parse_failures: 1,
            connector: "claude_code".into(),
        });
        rt(SessionMeta {
            path: PathBuf::from("empty.jsonl"),
            cwd: None,
            first_ts: None,
            last_ts: None,
            turn_count: 0,
            parse_failures: 0,
            connector: "generic_jsonl".into(),
        });
        rt(TimeWindow::default());
        rt(TimeWindow::around(
            ts("2026-08-19T07:00:00Z"),
            Duration::minutes(5),
        ));
        rt(Signal::TranscriptMention);
        rt(Evidence::new(Signal::Topology, "invoked_from=skeleton"));
        // Confidence is not Eq (it holds an f64), so it is checked by hand.
        let c = Confidence::new(0.75);
        let back: Confidence = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn role_keeps_unknown_spellings() {
        assert_eq!(
            Role::from("tool_use".to_string()),
            Role::Other("tool_use".into())
        );
        assert_eq!(Role::Other("tool_use".into()).as_str(), "tool_use");
    }

    #[test]
    fn confidence_is_clamped() {
        assert_eq!(Confidence::new(1.5).value(), 1.0);
        assert_eq!(Confidence::new(-1.0).value(), 0.0);
        assert_eq!(Confidence::new(f64::NAN).value(), 0.0);
    }

    #[test]
    fn durations_take_pacts_spellings() {
        assert_eq!(parse_duration("30").unwrap(), Duration::seconds(30));
        assert_eq!(parse_duration("45m").unwrap(), Duration::minutes(45));
        assert_eq!(parse_duration("2h").unwrap(), Duration::hours(2));
        assert_eq!(parse_duration("1d").unwrap(), Duration::days(1));
        assert_eq!(parse_duration("2w").unwrap(), Duration::weeks(2));
        for bad in ["", "m", "-5m", "5y", "five"] {
            assert!(parse_duration(bad).is_err(), "{bad:?} should not parse");
        }
    }

    #[test]
    fn windows_overlap_on_a_shared_instant() {
        let a = TimeWindow {
            since: Some(ts("2026-08-19T07:00:00Z")),
            until: Some(ts("2026-08-19T08:00:00Z")),
        };
        let touching = TimeWindow {
            since: Some(ts("2026-08-19T08:00:00Z")),
            until: Some(ts("2026-08-19T09:00:00Z")),
        };
        let after = TimeWindow {
            since: Some(ts("2026-08-19T08:00:01Z")),
            until: None,
        };
        assert!(a.overlaps(&touching));
        assert!(!a.overlaps(&after));
        assert!(a.overlaps(&TimeWindow::default()));
        assert!(TimeWindow::default().is_unbounded());
        assert!(a.contains(ts("2026-08-19T07:30:00Z")));
        assert!(!a.contains(ts("2026-08-19T09:30:00Z")));
    }

    #[test]
    fn parse_when_takes_both_dialects() {
        assert_eq!(
            parse_when("2026-08-19T07:00:00Z").unwrap(),
            ts("2026-08-19T07:00:00Z")
        );
        let two_hours_ago = parse_when("2h").unwrap();
        let expected = Utc::now() - Duration::hours(2);
        assert!((two_hours_ago - expected).num_seconds().abs() < 5);
        assert!(parse_when("not a time").is_err());
    }
}
