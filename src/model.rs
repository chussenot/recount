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
//!
//! **Why a keyed answer is a [`Tier`] and not a fifth rung on the ladder.**
//! The four scores in `join.rs` are calibrated *inference*: they exist because
//! no signal is certain. pact 0.11.0 changed the question for events that
//! carry `harness_subagent` — the event NAMES the transcript, resolution is a
//! path join and a `stat`, and there is nothing probabilistic left. Giving
//! that 0.99 would put an identity on the same axis as a guess and invite a
//! reader to compare them, which is the confident-inference failure this tool
//! exists to prevent. So the answer says *how it was reached* first, and only
//! a topological answer carries a number.
//!
//! The shape is `enum Tier { Topological(Confidence), Keyed }` — the score is
//! *inside* the inferring variant — rather than a `Tier` beside an
//! `Option<Confidence>`. Two fields can disagree: `(Keyed, Some(0.99))` is
//! representable, so someone eventually writes it, and every reader then needs
//! to know which field wins. With the score inside the variant there is no
//! `f64` on the keyed arm to print, unwrap, or compare — a caller that wants
//! one has to `match`, and the keyed arm has nothing to hand back. Wrong
//! thing unrepresentable beats wrong thing discouraged. [`Confidence`] itself
//! stays a plain clamped number for the same reason: a `Confidence::Keyed`
//! variant would make `value()` fallible and every existing caller would
//! reach for an `unwrap_or`, which is the fake score arriving by the back
//! door.
//!
//! **What that serialises to.** `Tier` writes two fields, meant to be
//! `#[serde(flatten)]`ed where a `confidence: Confidence` field sits today:
//!
//! ```json
//! { "tier": "keyed",       "confidence": null }
//! { "tier": "topological", "confidence": 0.65 }
//! ```
//!
//! `.confidence` stays exactly what it is today — a number produced by the
//! four-rung ladder, against the same `CONFIDENT` threshold — for every answer
//! that was inferred. A keyed answer puts `null` there, on purpose and
//! explicitly rather than by omitting the key: a script that reads
//! `.confidence` and compares it to a threshold gets `null`, which fails the
//! comparison loudly instead of silently ranking an identity against a guess.
//! Such a script is now *incomplete*, not wrong, and the fix is one field
//! away. Deserialising is strict in the same direction: `{"tier":"keyed",
//! "confidence":0.99}` is rejected, not quietly honoured.

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
/// Both line tallies are counts, never errors: the spec's tolerance contract
/// is "one bad line never fails a file", so a line that did not become a turn
/// is reported as a number `doctor` can show rather than propagated as a
/// `Result`.
///
/// # `declined` is not `parse_failures`, and the difference is the whole point
///
/// They were one field until `docs/format-notes.md` §5.1 measured what that
/// cost. `doctor` reported "12 unparseable lines across 6 of 828 sessions —
/// those transcripts are damaged, not merely skipped" on the machine recount
/// was written on, and every word of it was false. There are **zero** JSON
/// errors in that corpus — 333,408 lines, two independent passes. All 12 were
/// the same 9.6-million-character line, twice each in six sibling files: one
/// ordinary prompt that was piped a digest on stdin and re-quoted itself until
/// it was 78.6% backslashes, retried six times in six minutes, and rejected by
/// the API with `400 … the request is ~8391167 tokens`. Claude Code wrote all
/// of it down byte-perfectly. The connector *declined* those lines at its 8 MB
/// cap. It never failed to parse one.
///
/// One `usize` collapsed "recount chose not to read this" into "this file is
/// hurt", and `doctor` rendered the collapsed number as damage — sending a
/// reader to hunt corruption that does not exist. recount exists because of a
/// misdiagnosis; a confident wrong statement in its own output is the one
/// defect it cannot ship. So:
///
/// * **`declined`** — the connector could have read the line and chose not to.
///   A policy it can name: over the line-size cap, or a shape this reader does
///   not take. The file is fine. The number is a fact about recount's
///   configuration, not about the transcript.
/// * **`parse_failures`** — the connector tried and could not. Damage.
///
/// A line kind a connector simply is not interested in — `claude_code`'s
/// 10,309 `attachment` lines, its fifteen non-testimony `type`s — is in
/// neither tally. Those are structure, not loss, and burying the twelve
/// numbers that matter under a hundred thousand that do not would recreate
/// the defect with a nicer name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionMeta {
    pub path: PathBuf,
    /// See the module doc: a session may never state one.
    pub cwd: Option<PathBuf>,
    /// `None` when no turn in the file parsed — an empty or wholly garbage file.
    pub first_ts: Option<DateTime<Utc>>,
    pub last_ts: Option<DateTime<Utc>>,
    pub turn_count: usize,
    /// Lines the connector tried to parse and could not. DAMAGE. See above.
    pub parse_failures: usize,
    /// Lines the connector refused to read on purpose. NOT damage. See above.
    #[serde(default)]
    pub declined: usize,
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

/// How an answer was reached: named, or inferred.
///
/// The ladder, and the whole argument for this shape, is in the module doc.
/// Variant order *is* the ladder — `Topological` below, `Keyed` above — so the
/// derived `PartialOrd` ranks any keyed answer over every score, and two
/// topological answers against each other.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "TierRepr", into = "TierRepr")]
pub enum Tier {
    /// Inferred, by the four-rung ladder in `join.rs`, against `CONFIDENT`.
    /// The number means what it has always meant.
    Topological(Confidence),
    /// The event named the transcript. There is no `f64` here because there is
    /// nothing left to be uncertain about; see [`Signal::Keyed`].
    Keyed,
}

impl Tier {
    /// The score, when there is one. `None` for a keyed answer, and there is
    /// no default to substitute: an identity has no position on the ladder.
    pub fn confidence(self) -> Option<Confidence> {
        match self {
            Tier::Topological(c) => Some(c),
            Tier::Keyed => None,
        }
    }

    /// Does this answer stand? The one place the threshold is applied, so no
    /// caller re-derives it and drops a keyed hit for having no number: keyed
    /// clears every bar, including 1.0.
    pub fn clears(self, threshold: f64) -> bool {
        match self {
            Tier::Keyed => true,
            Tier::Topological(c) => c.value() >= threshold,
        }
    }

    /// The word this tier serialises as, so human output and `--json` cannot
    /// drift apart.
    //
    // The join (recount-1cw.5) uses the type but not this word: the digest
    // headers that print it are `at`, `explain` and `sessions`, next wave. Same
    // mechanism as the four attributes that guarded the whole type before it
    // had a caller — the expectation goes unfulfilled the moment recount-1cw.6
    // prints the word, and the build says so.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "the digest word lands in recount-1cw.6")
    )]
    pub fn name(self) -> &'static str {
        match self {
            Tier::Keyed => "keyed",
            Tier::Topological(_) => "topological",
        }
    }
}

/// `keyed`, or the two decimals a [`Confidence`] prints. Deliberately not a
/// number in both cases: there is no number in one of them.
impl fmt::Display for Tier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Tier::Keyed => f.write_str("keyed"),
            Tier::Topological(c) => c.fmt(f),
        }
    }
}

/// The wire shape of a [`Tier`]: the two flat fields, meant to be
/// `#[serde(flatten)]`ed into whatever carries the answer.
#[derive(Serialize, Deserialize)]
struct TierRepr {
    tier: TierName,
    /// Always written, `null` for keyed — see the module doc. `default` on the
    /// way in so `{"tier": "keyed"}` is also accepted.
    #[serde(default)]
    confidence: Option<Confidence>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TierName {
    Keyed,
    Topological,
}

impl From<Tier> for TierRepr {
    fn from(t: Tier) -> Self {
        TierRepr {
            tier: match t {
                Tier::Keyed => TierName::Keyed,
                Tier::Topological(_) => TierName::Topological,
            },
            confidence: t.confidence(),
        }
    }
}

impl TryFrom<TierRepr> for Tier {
    type Error = String;

    fn try_from(r: TierRepr) -> std::result::Result<Self, String> {
        match (r.tier, r.confidence) {
            (TierName::Keyed, None) => Ok(Tier::Keyed),
            (TierName::Topological, Some(c)) => Ok(Tier::Topological(c)),
            (TierName::Keyed, Some(c)) => Err(format!(
                "tier \"keyed\" carries confidence {c}: a keyed answer is an identity, not a score"
            )),
            (TierName::Topological, None) => {
                Err("tier \"topological\" without a confidence".to_string())
            }
        }
    }
}

/// What a piece of evidence is evidence *of*.
///
/// Three of these are the scored signals the original spec named for the
/// join. The fourth, [`Signal::Keyed`], arrived with pact 0.11.0 and is a
/// change to the spec rather than a new way to score: it is not weighed
/// against the other three, it replaces them. An event that names its
/// transcript is resolved by a path join and a `stat`, so the evidence for it
/// is a filename, and the answer it produces is [`Tier::Keyed`] — no number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Signal {
    /// The event carried `harness_subagent`, and the transcript it names is on
    /// disk: `.../<session>/subagents/agent-<id>.jsonl`. Not scored — the file
    /// is there or it is not.
    Keyed,
    /// `invoked_from -> worktree path -> session cwd`.
    Topology,
    /// Required overlap between the event window and the session window.
    TimeOverlap,
    /// PACT_AGENT name or bead id mentioned in the transcript.
    TranscriptMention,
}

/// Why an answer is what it is. The spec: "Scored match that always reports
/// confidence and evidence, never a bare answer" — so every result carries a
/// non-empty `Vec<Evidence>`, a keyed one included: it has no score to
/// explain, but it still has to say which file it was named by.
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
            declined: 2,
            connector: "claude_code".into(),
        });
        rt(SessionMeta {
            path: PathBuf::from("empty.jsonl"),
            cwd: None,
            first_ts: None,
            last_ts: None,
            turn_count: 0,
            parse_failures: 0,
            declined: 0,
            connector: "generic_jsonl".into(),
        });
        rt(TimeWindow::default());
        rt(TimeWindow::around(
            ts("2026-08-19T07:00:00Z"),
            Duration::minutes(5),
        ));
        rt(Signal::TranscriptMention);
        rt(Signal::Keyed);
        rt(Evidence::new(Signal::Topology, "invoked_from=skeleton"));
        rt(Evidence::new(
            Signal::Keyed,
            "harness_subagent=acf8cbc94118744f1 -> .../subagents/agent-acf8cbc94118744f1.jsonl",
        ));
        rt(Tier::Keyed);
        rt(Tier::Topological(Confidence::new(0.65)));
        // Confidence is not Eq (it holds an f64), so it is checked by hand.
        let c = Confidence::new(0.75);
        let back: Confidence = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert_eq!(c, back);
    }

    /// The two lines of JSON the whole bead is about. `at`, `explain`,
    /// `sessions` and `testify` all render this, so it is pinned literally.
    #[test]
    fn a_keyed_answer_serialises_a_null_confidence() {
        assert_eq!(
            serde_json::to_string(&Tier::Keyed).unwrap(),
            r#"{"tier":"keyed","confidence":null}"#
        );
        assert_eq!(
            serde_json::to_string(&Tier::Topological(Confidence::new(0.65))).unwrap(),
            r#"{"tier":"topological","confidence":0.65}"#
        );
    }

    /// How `join::Match` will carry it: two flat fields where `confidence`
    /// sits today, not a nested object.
    #[test]
    fn tier_flattens_into_the_answer() {
        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        struct Answer {
            agent: String,
            #[serde(flatten)]
            tier: Tier,
        }

        let a = Answer {
            agent: "tier".into(),
            tier: Tier::Keyed,
        };
        let json = serde_json::to_string(&a).unwrap();
        assert_eq!(json, r#"{"agent":"tier","tier":"keyed","confidence":null}"#);
        assert_eq!(a, serde_json::from_str::<Answer>(&json).unwrap());
    }

    /// A score on a keyed answer is the failure this type exists to prevent,
    /// so it is not merely unconstructible in Rust — it is rejected on the way
    /// in as well.
    #[test]
    fn a_keyed_tier_cannot_carry_a_score() {
        let err = serde_json::from_str::<Tier>(r#"{"tier":"keyed","confidence":0.99}"#)
            .expect_err("keyed + a score must not deserialise");
        assert!(err.to_string().contains("identity, not a score"), "{err}");
        assert!(serde_json::from_str::<Tier>(r#"{"tier":"topological"}"#).is_err());
        // Tolerant the other way: an omitted null is still keyed.
        assert_eq!(
            serde_json::from_str::<Tier>(r#"{"tier":"keyed"}"#).unwrap(),
            Tier::Keyed
        );
    }

    #[test]
    fn keyed_outranks_every_score() {
        assert!(Tier::Keyed > Tier::Topological(Confidence::new(1.0)));
        assert!(Tier::Topological(Confidence::new(0.7)) > Tier::Topological(Confidence::new(0.65)));
        // The threshold is applied in one place, and keyed clears every bar.
        assert!(Tier::Keyed.clears(1.0));
        assert!(Tier::Topological(Confidence::new(0.65)).clears(0.5));
        assert!(!Tier::Topological(Confidence::new(0.4)).clears(0.5));
        // No number to print for a keyed answer, and no default standing in.
        assert_eq!(Tier::Keyed.confidence(), None);
        assert_eq!(Tier::Keyed.to_string(), "keyed");
        assert_eq!(Tier::Topological(Confidence::new(0.65)).to_string(), "0.65");
        assert_eq!(Tier::Keyed.name(), "keyed");
        assert_eq!(
            Tier::Topological(Confidence::new(0.4)).name(),
            "topological"
        );
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
