//! Three measurements, not a benchmarking harness.
//!
//! `docs/spec.md`, "Proof obligations", puts a ceiling on exactly two numbers:
//!
//! > Bench: pruned `at` <100ms; unwindowed grep over 500 synthetic sessions
//! > (~200MB) <2s.
//!
//! Those two are [`spec_ceilings_on_a_synthetic_corpus`] and they are the
//! 0.1.0 contract: untouched here, and not to be moved. 0.2.0 added a third,
//! [`keyed_at_beats_the_topological_scan`] — the keyed tier under 20 ms and
//! ahead of the path that scans — because a tier that resolves by name has to
//! be visibly cheaper than one that searches or it is not resolving by name.
//! It generates its own corpus rather than sharing theirs, so a change to it
//! cannot move their numbers.
//!
//! Every test here is `#[ignore]`d so CI stays fast. Run them with:
//!
//! ```text
//! cargo test --release --test bench -- --ignored --nocapture
//! ```
//!
//! # Release is not optional
//!
//! `cargo test` without `--release` is a debug build, and a debug number is not
//! the number the spec means. Measured on the pre-recount-hwk.47 code, same
//! corpus, same machine: the grep ceiling read 2.83 s in release and **35.9 s
//! in debug** — 13x, entirely from the optimiser, nothing to do with the
//! design. (The `at` ceiling survived both: 13 ms release, 42 ms debug.) Both
//! release figures are lower today; the ratio is the point. So the asserts fire only under
//! `--release`; a debug run still prints every measurement, clearly labelled,
//! and asserts nothing. The printed block always names the profile it was taken
//! in. Do not quote a debug number as if it settled anything.
//!
//! # Result, at the time of writing
//!
//! **All three ceilings are met.** Ceiling 1 with room to spare (15-65 ms
//! against 100 ms; the margin moves with machine load, the verdict does not).
//! Ceiling 2 at ~1.0-1.2 s against 2 s. The keyed ceiling at ~10 ms against
//! 20 ms, which is 35x under the topological path that scans and about 20%
//! under the topological path that answers — the whole of that second, smaller
//! margin being the transcript read the mention tiebreaker does and the keyed
//! tier does not.
//!
//! Ceiling 2 was missed when this file was written — ~2.8 s, and the assert
//! fired. That was reported as a finding rather than softened, and
//! recount-hwk.47 then fixed it: an ASCII fast path in `cmd::grep::eq_ci`
//! (~1.75 s of the 2.8 s, see that module's doc) and dropping a whole
//! discovery parse the `generic_jsonl` fallback was doing for nothing
//! (~230 ms, see `connector::discover_all`). The asserts never moved. A bench
//! that reports a miss as a pass is worth nothing, and a ceiling that is
//! lowered to fit the code is not a ceiling; the spec's number is the spec's
//! number until the spec says otherwise, and the code came to it.
//!
//! # What the printed block is for
//!
//! The spec's refusals section defers one thing: a metadata-only sidecar
//! manifest "may be added iff real windowed queries exceed ~2s on a real
//! corpus — never a content index, never a database." That clause is evaluated
//! against evidence, and this file is the evidence. Hence
//! [`real_corpus_reference`]: the synthetic corpus proves the ceiling, the real
//! one proves the ceiling is the one that matters. It measures and prints, and
//! asserts nothing — a machine without `~/.claude/projects`, or with a corpus
//! ten times this one, must not turn into a red test.
//!
//! # Two `at` numbers, not one
//!
//! `at` has two paths and they cost differently, by design (see the retry in
//! [`cmd::at::decide`](../src/cmd/at.rs)): the cheap windowed pass prunes on
//! mtime, and when it refuses on *topology* the whole scan is repeated
//! unwindowed, because the prune deleted exactly the sessions a "no overlap"
//! refusal has to name. So a pruned `at` that finds its answer opens one file;
//! a pruned `at` that is about to explain itself opens all 500. The spec's
//! <100 ms is the answering path — the explaining path is measured and printed
//! beside it, unasserted, because the gap is a real property of the tool and
//! the README should carry it rather than have someone rediscover it.
//!
//! # Cold or warm
//!
//! Warm. Every number here is taken on a page cache the test itself just
//! filled by writing the corpus, and each measurement is run twice with both
//! runs printed. A genuinely cold number needs `drop_caches`, which a test may
//! not do. An honest warm number plus this paragraph beats a fake cold one.
//!
//! # Dependency note
//!
//! `recount` is a binary crate with no `lib` target, so an integration test
//! cannot call into it: everything below drives the real binary through
//! `CARGO_BIN_EXE_recount` with `$HOME` pointed at a tempdir. That also means
//! [`encode`] is a ten-line copy of `worktree::encode` rather than a call to
//! it. If a `lib` target ever appears, delete it and import the original.

use std::fs::{self, File, FileTimes};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime};

use chrono::{DateTime, SecondsFormat, TimeZone, Utc};

/// 500 sessions x 40 turns x ~10 KB of body ≈ 200 MB, the spec's corpus.
///
/// The record *size* is calibrated against this machine rather than invented:
/// its real corpus is 863 files / 745 MB / 77,008 records, a mean record near
/// 10 KB. It turns out barely to matter — the same 200 MB cut into 2 KB records
/// measured 2.81 s against 2.83 s, because the case-insensitive scan is
/// per-byte and swamps the per-record parse. What does matter is that a
/// synthetic byte is *more* expensive than a real one: 171 MB of real
/// transcript scans in 2.32 s where 193 MB of this scans in 2.83 s, because a
/// real file spends much of its size on records and blocks that never become
/// searchable turn text. Read the synthetic number as the pessimistic one.
///
/// (Those four figures are pre-recount-hwk.47 and all of them are lower today.
/// They are kept because what they establish is the *ratios* — record size
/// barely matters, a real byte is cheaper than a synthetic one — and the
/// calibration they justify has not changed.)
const SESSIONS: usize = 500;
const TURNS_PER_SESSION: usize = 40;
const BODY_BYTES: usize = 10_000;

/// The spec's two ceilings.
const AT_CEILING: Duration = Duration::from_millis(100);
const GREP_CEILING: Duration = Duration::from_secs(2);

/// recount-1cw.10's ceiling for the keyed tier. Not a spec number — the spec
/// predates pact 0.11.0 — but a contract all the same, and a much tighter one:
/// a keyed answer is a path join and one `stat`, so five times under the
/// topological ceiling is the loose end of what it should cost.
const KEYED_CEILING: Duration = Duration::from_millis(20);

/// The identity the keyed corpus is keyed by. Both are opaque tokens to
/// `claude_code::keyed`, which checks their shape and joins the path.
const KEYED_SESSION: &str = "11111111-2222-3333-4444-555555555555";
const KEYED_SUB: &str = "a1ded78ef5b657fa6";

/// The fleet identity the answering path resolves. Word-boundary matched by
/// the join, so it must not occur in the filler text `generate` writes.
const AGENT: &str = "bench";
/// Present in exactly one session, so `grep`'s cost is the scan and not the
/// printing. Written upper-case and searched lower-case, so the measured path
/// is the case-insensitive one the spec asks for.
const NEEDLE: &str = "QUICKSILVER";

/// The spec's two ceilings, against a corpus generated here and deleted on the
/// way out.
///
/// One test rather than two because both ceilings need the same 200 MB and
/// generating it twice is 200 MB of pointless writing. Every number is printed
/// before anything is asserted, so a failure reports all of them.
#[test]
#[ignore = "generates ~200MB; run explicitly, in release: cargo test --release --test bench -- --ignored --nocapture"]
fn spec_ceilings_on_a_synthetic_corpus() {
    let home = tempfile::tempdir().expect("tempdir");
    let repo_dir = tempfile::tempdir().expect("tempdir");
    let repo = repo_dir.path().canonicalize().expect("canonicalize");
    git_init(&repo);

    // A fixed instant, not `Utc::now()`: mtimes and turn timestamps are both
    // set explicitly, so nothing here can race the wall clock.
    let target = Utc.with_ymd_and_hms(2026, 1, 15, 12, 0, 0).unwrap();
    let bytes = generate(home.path(), &repo, target);

    // --- measurement 1: the answering path. One file survives the mtime
    // prune, and it is the one that holds the answer.
    let answering = twice(home.path(), &at_args(&repo, target));

    // --- the explaining path, unasserted. A `--ts` a month past every mtime
    // prunes all 500 files, which makes the cheap pass refuse on topology,
    // which triggers the unwindowed re-scan.
    let stale = target + chrono::Duration::days(30);
    let explaining = twice(home.path(), &at_args(&repo, stale));

    // --- measurement 2: unwindowed grep. No --since, so nothing prunes and
    // all 500 sessions are opened.
    let grep_args = [
        "grep".to_string(),
        NEEDLE.to_lowercase(),
        "--repo".to_string(),
        repo.display().to_string(),
        "--json".to_string(),
    ];
    let grep = twice(home.path(), &grep_args);

    // --- not a ceiling, a decomposition. `sessions` is one unwindowed parse of
    // the same corpus and nothing else, so it separates the two halves of
    // grep's bill: `grep` parses the corpus once in `discover` and a second
    // time in `read`, then runs the case-insensitive scan over every turn.
    let one_parse = twice(
        home.path(),
        &[
            "sessions".to_string(),
            "--repo".to_string(),
            repo.display().to_string(),
            "--json".to_string(),
        ],
    );

    let searched = json_field(&grep.1.stdout, "sessions_searched");
    let turns = json_field(&grep.1.stdout, "turns_searched");

    println!("\n{}", header());
    println!("corpus            : {SESSIONS} synthetic sessions, {TURNS_PER_SESSION} turns each, {} total (tempdir, deleted on exit)", mb(bytes));
    println!("page cache        : warm (the corpus was written by this test moments before); run 1 / run 2 both shown");
    println!();
    println!("SPEC CEILING 1 — pruned `at`, answering path            < 100 ms");
    println!(
        "  run 1 {}   run 2 {}   exit {}   [499 of 500 sessions pruned on mtime, unopened]",
        ms(answering.0.took),
        ms(answering.1.took),
        answering.1.code
    );
    println!("  {}", verdict(answering.1.took, AT_CEILING));
    println!();
    println!("(not a ceiling) `at`, explaining path — the unwindowed re-scan");
    println!("  run 1 {}   run 2 {}   exit {}   [all {SESSIONS} sessions re-opened to name the true refusal]",
        ms(explaining.0.took), ms(explaining.1.took), explaining.1.code);
    println!("  This is the price of `no overlap` being a distinct reason from `no cwd match`.");
    println!();
    println!("SPEC CEILING 2 — unwindowed `grep` over ~200 MB          < 2 s");
    println!(
        "  run 1 {}   run 2 {}   exit {}   [{searched} sessions, {turns} turns searched]",
        ms(grep.0.took),
        ms(grep.1.took),
        grep.1.code
    );
    println!("  {}", verdict(grep.1.took, GREP_CEILING));
    println!("  where it goes, for whoever has to make this faster:");
    println!(
        "    {}  `recount sessions`, same corpus — one discovery parse (plus identity work),",
        ms(one_parse.1.took)
    );
    println!("              and no content scan at all.");
    println!(
        "    {}  `at`'s explaining path above — one discovery parse, no scan, no identity work.",
        ms(explaining.1.took)
    );
    println!("    `grep` still parses the corpus TWICE (once in `cmd::grep::search`'s");
    println!("    `discover_all`, once in the `connector.read` loop) and then runs `find_ci`");
    println!("    over every turn. The scan is no longer the bulk of it: `eq_ci` answers an");
    println!("    ASCII pair without building a `char::to_lowercase()` iterator (recount-hwk.47).");
    println!("    The second parse is what is left to take, worth ~540 ms, and taking it needs a");
    println!("    `Connector` trait change. Neither half is a manifest-shaped problem: a metadata");
    println!("    sidecar could remove at most the first parse.");
    println!();

    // Correctness first: a fast wrong answer is not a measurement.
    assert_eq!(
        answering.1.code, 0,
        "the answering path must answer:\n{}",
        answering.1.stderr
    );
    assert_eq!(
        explaining.1.code, 1,
        "the explaining path must refuse:\n{}",
        explaining.1.stderr
    );
    assert!(
        grep.1.stdout.contains(NEEDLE),
        "grep found no hit; it measured the wrong thing"
    );

    if cfg!(debug_assertions) {
        println!("DEBUG BUILD — nothing asserted. Re-run with --release for the numbers the spec means.\n");
        return;
    }
    assert!(
        answering.1.took < AT_CEILING,
        "pruned `at` (answering) took {}, ceiling {}",
        ms(answering.1.took),
        ms(AT_CEILING)
    );
    assert!(
        grep.1.took < GREP_CEILING,
        "unwindowed `grep` took {}, ceiling {}",
        ms(grep.1.took),
        ms(GREP_CEILING)
    );
}

/// recount-1cw.10's ceiling: a keyed `at` is a path join and one `stat`, and it
/// has to beat the topological path that scans.
///
/// # What is being compared, and what is NOT
///
/// The same corpus, the same repo, the same instant, three commands:
///
/// * **keyed** — `explain` on a ledger row carrying `harness_session` +
///   `harness_subagent`. The join is one path join and one `stat`.
/// * **topological, answering** — `at` on the same instant with no chain. The
///   ladder runs: cwd resolution, scoring, and a transcript read for the
///   mention tiebreaker.
/// * **topological, scanning** — `at` a month later, which refuses on topology
///   and so repeats the whole discovery unwindowed (see [`cmd::at::decide`]).
///   This is *the scan*, and it is what the assert below is against.
///
/// **The keyed tier does not skip discovery, and this bench is honest about
/// it.** `join::resolve` is handed a `&[SessionMeta]` that `at` has already
/// built by parsing every file the mtime window admits, and the keyed answer
/// has to be found among them (`join::keyed_match`). So what the key removes is
/// the *ladder* — the cwd chain, the scoring, the transcript reads — and the
/// unwindowed re-scan a topological refusal triggers. It does not remove the
/// windowed parse, and on a corpus the window does not prune, keyed costs what
/// everything else costs. The number below is therefore a windowed number, on
/// the same 499-of-500-pruned corpus SPEC CEILING 1 uses, and it should be read
/// as "the key costs nothing on top of the window", not as "the key replaced
/// the scan".
///
/// # Why the assert is against the scanning path and not the answering one
///
/// Both keyed and topological-answering are dominated by process spawn plus the
/// same windowed discovery, so they differ by a hair and a strict
/// `keyed < answering` would be a coin flip dressed up as a contract. The
/// scanning path is the one the bead's sentence is about — "a path join and a
/// stat against a scan" — and it is a real margin, printed as a ratio so a
/// regression that quietly turns the key back into a search is visible rather
/// than merely un-asserted.
#[test]
#[ignore = "generates ~200MB; run explicitly, in release: cargo test --release --test bench -- --ignored --nocapture"]
fn keyed_at_beats_the_topological_scan() {
    let home = tempfile::tempdir().expect("tempdir");
    let repo_dir = tempfile::tempdir().expect("tempdir");
    let repo = repo_dir.path().canonicalize().expect("canonicalize");
    git_init(&repo);

    let target = Utc.with_ymd_and_hms(2026, 1, 15, 12, 0, 0).unwrap();
    let bytes = generate(home.path(), &repo, target);
    let keyed_path = write_keyed(home.path(), &repo, target);

    // --- the keyed path: one ledger row naming the transcript above.
    let keyed = twice(home.path(), &explain_args(&repo, 1));
    // --- the ladder, answering, on the same instant.
    let answering = twice(home.path(), &at_args(&repo, target));
    // --- the ladder, scanning: a month past every mtime, so the cheap pass
    // refuses on topology and the whole corpus is re-opened unwindowed.
    let stale = target + chrono::Duration::days(30);
    let scanning = twice(home.path(), &at_args(&repo, stale));

    let ratio = scanning.1.took.as_secs_f64() / keyed.1.took.as_secs_f64().max(f64::EPSILON);

    println!("\n{}", header());
    println!("corpus            : {SESSIONS} synthetic sessions, {TURNS_PER_SESSION} turns each, {} total (tempdir, deleted on exit)", mb(bytes));
    println!(
        "                    plus one keyed subagent transcript, {} bytes, at {}",
        fs::metadata(&keyed_path).map(|m| m.len()).unwrap_or(0),
        keyed_path.display()
    );
    println!("page cache        : warm (the corpus was written by this test moments before); run 1 / run 2 both shown");
    println!("window            : the default ±5m, so 499 of 500 sessions are pruned on mtime and never opened");
    println!();
    println!("recount-1cw.10 CEILING — keyed `at` (via `explain`)      < 20 ms");
    println!(
        "  run 1 {}   run 2 {}   exit {}   [one path join, one stat, no score]",
        ms(keyed.0.took),
        ms(keyed.1.took),
        keyed.1.code
    );
    println!("  {}", verdict(keyed.1.took, KEYED_CEILING));
    println!();
    println!("(not a ceiling) the topological ladder, same corpus, same instant");
    println!(
        "  answering  run 1 {}   run 2 {}   exit {}   [cwd chain, scoring, one transcript read]",
        ms(answering.0.took),
        ms(answering.1.took),
        answering.1.code
    );
    println!(
        "  scanning   run 1 {}   run 2 {}   exit {}   [refuses on topology, re-opens all {SESSIONS} unwindowed]",
        ms(scanning.0.took),
        ms(scanning.1.took),
        scanning.1.code
    );
    println!("  keyed is {ratio:.1}x faster than the scan.");
    println!(
        "  Read the answering figure as a control, not a comparison: it and the keyed run pay"
    );
    println!(
        "  the same spawn and the same windowed discovery, so they cannot differ by much. The"
    );
    println!("  key buys the ladder and the re-scan, not the parse — see this test's doc comment.");
    println!();

    // Correctness first: a fast wrong answer is not a measurement.
    assert_eq!(
        keyed.1.code, 0,
        "the keyed path must answer:\n{}",
        keyed.1.stderr
    );
    let doc: serde_json::Value =
        serde_json::from_str(&keyed.1.stdout).expect("the keyed run printed one JSON document");
    assert_eq!(
        doc["best"]["tier"], "keyed",
        "this did not measure the keyed tier"
    );
    assert_eq!(
        doc["best"]["confidence"],
        serde_json::Value::Null,
        "a keyed answer carries no score"
    );
    assert!(
        doc["best"]["session"]["path"]
            .as_str()
            .is_some_and(|p| p.ends_with(&format!("agent-{KEYED_SUB}.jsonl"))),
        "the keyed tier answered with the wrong file: {}",
        doc["best"]["session"]["path"]
    );
    assert_eq!(
        scanning.1.code, 1,
        "the scanning path must refuse:\n{}",
        scanning.1.stderr
    );

    if cfg!(debug_assertions) {
        println!("DEBUG BUILD — nothing asserted. Re-run with --release for the numbers the ceiling means.\n");
        return;
    }
    assert!(
        keyed.1.took < KEYED_CEILING,
        "keyed `at` took {}, ceiling {}",
        ms(keyed.1.took),
        ms(KEYED_CEILING)
    );
    assert!(
        keyed.1.took < scanning.1.took,
        "keyed `at` ({}) did not beat the topological scan ({}) — a path join and a \
         stat cannot lose to re-opening {SESSIONS} files, so the keyed tier is not \
         keyed any more",
        ms(keyed.1.took),
        ms(scanning.1.took)
    );
}

/// The same two commands against this machine's real `~/.claude/projects`, for
/// the deferral clause in the spec's refusals section.
///
/// Asserts nothing and modifies nothing: it reads the real corpus through the
/// same read-only binary the user runs. Skips, loudly, where there is no
/// corpus — so this file stays green on a fresh box.
#[test]
#[ignore = "reads the real ~/.claude corpus; run explicitly: cargo test --release --test bench -- --ignored --nocapture"]
fn real_corpus_reference() {
    let Some(home) = std::env::var_os("HOME") else {
        println!("\nreal corpus: no $HOME — skipped.");
        return;
    };
    let home = PathBuf::from(home);
    if !home.join(".claude").join("projects").is_dir() {
        println!("\nreal corpus: no ~/.claude/projects — skipped.");
        return;
    }
    // recount's own history is a few MB — too small to say anything about a
    // ceiling. Measure the biggest real project on this machine instead, and
    // fall back to this repo where there is nothing bigger.
    let projects = home.join(".claude").join("projects");
    let repo =
        biggest_real_repo(&projects).unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    println!("\nreal corpus: measuring against {}", repo.display());

    // No --since, so this is the unwindowed scan the ceiling is about. The
    // needle is one nobody's transcript is likely to hold; the cost is the
    // scan either way.
    let grep = twice(
        &home,
        &[
            "grep".to_string(),
            "zzqx-no-such-needle".to_string(),
            "--repo".to_string(),
            repo.display().to_string(),
            "--json".to_string(),
        ],
    );
    // The clause the deferral is written against says *windowed*, and a
    // windowed query is the one the mtime prune exists for. Measure it beside
    // the unwindowed worst case, because they are not the same question.
    let windowed = twice(
        &home,
        &[
            "grep".to_string(),
            "zzqx-no-such-needle".to_string(),
            "--repo".to_string(),
            repo.display().to_string(),
            "--since".to_string(),
            "24h".to_string(),
            "--json".to_string(),
        ],
    );
    // One unwindowed parse and no scan, so the same decomposition as the
    // synthetic block applies to real data.
    let one_parse = twice(
        &home,
        &[
            "sessions".to_string(),
            "--repo".to_string(),
            repo.display().to_string(),
            "--json".to_string(),
        ],
    );
    // `at` against a live identity: whichever path it takes, the exit code says
    // which, and both are worth the reader's attention.
    let at = twice(&home, &at_args(&repo, Utc::now()));

    let dirs = json_paths(&grep.1.stdout, "session_dirs");
    let bytes: u64 = dirs
        .iter()
        .map(PathBuf::as_path)
        .flat_map(walk)
        .map(|p| fs::metadata(&p).map(|m| m.len()).unwrap_or(0))
        .sum();
    let searched = json_field(&grep.1.stdout, "sessions_searched");
    let turns = json_field(&grep.1.stdout, "turns_searched");

    println!("\n{}", header());
    println!(
        "corpus            : real ~/.claude/projects for {}",
        repo.display()
    );
    println!(
        "                    {} session dir(s), {searched} sessions, {}",
        dirs.len(),
        mb(bytes)
    );
    println!("page cache        : warm-ish (whatever the OS held); run 1 / run 2 both shown. Read-only: nothing was written.");
    println!();
    println!(
        "real unwindowed `grep`  run 1 {}   run 2 {}   [{turns} turns searched]",
        ms(grep.0.took),
        ms(grep.1.took)
    );
    println!(
        "real   windowed `grep --since 24h`  run 1 {}   run 2 {}   [{} of {searched} sessions survived the mtime prune]",
        ms(windowed.0.took),
        ms(windowed.1.took),
        json_field(&windowed.1.stdout, "sessions_searched")
    );
    println!(
        "  for scale: `recount sessions` over the same corpus — one discovery parse, no content",
    );
    println!(
        "  scan — is {}. `grep` parses twice and then scans every turn.",
        ms(one_parse.1.took)
    );
    println!(
        "real `at --agent {AGENT} --ts now`  run 1 {}   run 2 {}   exit {} ({}, {} session(s) survived the prune)",
        ms(at.0.took),
        ms(at.1.took),
        at.1.code,
        if at.1.code == 0 {
            "answering path"
        } else {
            "refused — includes the unwindowed re-scan"
        },
        json_field(&at.1.stdout, "considered")
    );
    println!();
    println!("DEFERRAL CLAUSE (docs/spec.md): a sidecar manifest may be added iff real *windowed*");
    println!("queries exceed ~2s on a real corpus — never a content index, never a database.");
    println!(
        "  real WINDOWED grep:   {}  -> {}",
        ms(windowed.1.took),
        if windowed.1.took < GREP_CEILING {
            "NOT triggered. The mtime prune is doing its job; do not build the manifest."
        } else {
            "TRIGGERED. Read the decomposition above before reaching for a manifest."
        }
    );
    println!(
        "  real UNWINDOWED grep: {}  (not what the clause names, but the worst case)",
        ms(grep.1.took)
    );
    println!("A manifest holds metadata, so it can only ever remove the `discover` parse — the");
    println!("`read` parse and the character scan stay. Compare the two lines above against the");
    println!("`sessions` figure before concluding a manifest is what the cost is made of.");
    println!();
}

// --- corpus -----------------------------------------------------------------

/// Write the synthetic corpus under `home` and return its total size.
///
/// 499 sessions are a month old in both their turns and their mtime, so the
/// mtime prune rules them out unopened. One — the last — sits at `target` and
/// names [`AGENT`], so the answering path has exactly one file to open.
fn generate(home: &Path, repo: &Path, target: DateTime<Utc>) -> u64 {
    let dir = home.join(".claude").join("projects").join(encode(repo));
    fs::create_dir_all(&dir).expect("session dir");
    let body =
        "the agent read the file and then wrote a patch that compiles. ".repeat(BODY_BYTES / 62);
    let cwd = serde_json::to_string(&repo.display().to_string()).expect("cwd json");
    let old = target - chrono::Duration::days(30);

    let mut total = 0;
    for s in 0..SESSIONS {
        let last = s + 1 == SESSIONS;
        let base = if last {
            target
        } else {
            old + chrono::Duration::minutes(s as i64)
        };
        let path = dir.join(format!("session-{s:04}.jsonl"));
        let file = File::create(&path).expect("session file");
        let mut w = BufWriter::new(&file);
        for t in 0..TURNS_PER_SESSION {
            // The mention and the needle ride on the target session's first
            // turn; every other line is filler of the same shape and size.
            let text = if last && t == 0 {
                format!("PACT_AGENT={AGENT} started. {NEEDLE}. {body}")
            } else {
                body.clone()
            };
            let ts = (base + chrono::Duration::seconds(t as i64))
                .to_rfc3339_opts(SecondsFormat::Millis, true);
            writeln!(w, r#"{{"type":"user","sessionId":"s{s:04}","cwd":{cwd},"timestamp":"{ts}","message":{{"role":"user","content":{}}}}}"#,
                serde_json::to_string(&text).expect("body json")).expect("write");
        }
        w.flush().expect("flush");
        drop(w);
        total += file.metadata().expect("metadata").len();
        // mtime is the whole point of the prune, so it is set, never inherited
        // from the order the test happened to write files in.
        let mtime = base + chrono::Duration::seconds(TURNS_PER_SESSION as i64);
        file.set_times(FileTimes::new().set_modified(SystemTime::from(mtime)))
            .expect("set mtime");
    }
    total
}

/// The keyed half of the corpus: one subagent transcript at the nested path
/// `claude_code::keyed` joins to, and a one-row ledger that names it.
///
/// Small on purpose. The keyed tier does one `stat` and then reads the file it
/// found, so a 10 MB transcript would measure a read the key did not cause. The
/// 200 MB beside it is what the *window* has to prune, which is the cost this
/// number is about.
///
/// Returns the transcript's path.
fn write_keyed(home: &Path, repo: &Path, target: DateTime<Utc>) -> PathBuf {
    let path = home
        .join(".claude")
        .join("projects")
        .join(encode(repo))
        .join(KEYED_SESSION)
        .join("subagents")
        .join(format!("agent-{KEYED_SUB}.jsonl"));
    fs::create_dir_all(path.parent().expect("subagents dir")).expect("mkdir subagents");
    let cwd = serde_json::to_string(&repo.display().to_string()).expect("cwd json");
    let file = File::create(&path).expect("keyed transcript");
    let mut w = BufWriter::new(&file);
    for t in 0..3 {
        let ts =
            (target + chrono::Duration::seconds(t)).to_rfc3339_opts(SecondsFormat::Millis, true);
        writeln!(
            w,
            r#"{{"type":"assistant","sessionId":"{KEYED_SESSION}","agentId":"{KEYED_SUB}","cwd":{cwd},"timestamp":"{ts}","message":{{"role":"assistant","model":"claude-opus-5","content":[{{"type":"text","text":"keyed turn {t}"}}]}}}}"#
        )
        .expect("write keyed turn");
    }
    w.flush().expect("flush");
    drop(w);
    // Inside the window, like the corpus's own live session: the keyed tier
    // still has to find its answer among the sessions discovery produced.
    file.set_times(
        FileTimes::new().set_modified(SystemTime::from(target + chrono::Duration::seconds(3))),
    )
    .expect("set mtime");

    let ledger = repo.join(".pact").join("events.jsonl");
    fs::create_dir_all(ledger.parent().expect(".pact")).expect("mkdir .pact");
    fs::write(
        &ledger,
        format!(
            r#"{{"at":"{}","agent":"{AGENT}","kind":"acquired","path":"src/join.rs","detail":"recount-1cw.10: the keyed measurement","chain_hash":"1111","pact_version":"0.11.0","harness":"claude-code","model":"claude-opus-5","harness_session":"{KEYED_SESSION}","harness_subagent":"{KEYED_SUB}"}}
"#,
            target.to_rfc3339_opts(SecondsFormat::Millis, true)
        ),
    )
    .expect("write ledger");
    path
}

fn git_init(repo: &Path) {
    let out = Command::new("git")
        .args(["init", "-q"])
        .arg(repo)
        .output()
        .expect("git init");
    assert!(
        out.status.success(),
        "git init: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// `worktree::encode`, copied. See the module doc's dependency note.
fn encode(path: &Path) -> String {
    let s = path.to_string_lossy();
    let trimmed = s.trim_end_matches('/');
    let trimmed = if trimmed.is_empty() { &s } else { trimmed };
    trimmed
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

// --- running and timing ------------------------------------------------------

struct Run {
    took: Duration,
    code: i32,
    stdout: String,
    stderr: String,
}

fn at_args(repo: &Path, ts: DateTime<Utc>) -> [String; 8] {
    [
        "at".into(),
        "--repo".into(),
        repo.display().to_string(),
        "--agent".into(),
        AGENT.into(),
        "--ts".into(),
        ts.to_rfc3339_opts(SecondsFormat::Millis, true),
        "--json".into(),
    ]
}

/// `explain` is the only command that reaches the keyed tier: `at`'s own flags
/// cannot name a transcript, so the chain has to come off a ledger row.
fn explain_args(repo: &Path, line: usize) -> [String; 6] {
    [
        "explain".into(),
        "--repo".into(),
        repo.display().to_string(),
        "--event-line".into(),
        line.to_string(),
        "--json".into(),
    ]
}

/// Wall time of the real binary, spawn included — which is what a user pays.
fn once(home: &Path, args: &[String]) -> Run {
    let start = Instant::now();
    let out = Command::new(env!("CARGO_BIN_EXE_recount"))
        .args(args)
        .env("HOME", home)
        .output()
        .expect("running recount");
    Run {
        took: start.elapsed(),
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

/// Twice, because the second run of anything is faster and hiding that would
/// be the dishonest half of a benchmark.
fn twice(home: &Path, args: &[String]) -> (Run, Run) {
    (once(home, args), once(home, args))
}

// --- reporting ---------------------------------------------------------------

fn header() -> String {
    let cpu = fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("model name"))
                .map(|l| l.split(':').nth(1).unwrap_or("").trim().to_string())
        })
        .unwrap_or_else(|| "unknown".into());
    format!(
        "=== recount bench ===\nbuild profile     : {}\nhardware          : {cpu} ({} cores visible)\nos                : {}",
        if cfg!(debug_assertions) { "DEBUG — numbers are not comparable to the spec's ceilings" } else { "release" },
        std::thread::available_parallelism().map_or(0, |n| n.get()),
        std::env::consts::OS,
    )
}

fn ms(d: Duration) -> String {
    format!("{:>8.1} ms", d.as_secs_f64() * 1000.0)
}

fn mb(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_048_576.0)
}

fn verdict(took: Duration, ceiling: Duration) -> String {
    let pct = took.as_secs_f64() / ceiling.as_secs_f64() * 100.0;
    format!(
        "{} — {} of the ceiling",
        if took < ceiling { "MET" } else { "MISSED" },
        format_args!("{pct:.0}%")
    )
}

/// One integer out of the command's own `--json`, so the printed corpus size is
/// what recount saw and not what the test believes it wrote.
fn json_field(stdout: &str, key: &str) -> u64 {
    serde_json::from_str::<serde_json::Value>(stdout)
        .ok()
        .and_then(|v| v.get(key)?.as_u64())
        .unwrap_or(0)
}

fn json_paths(stdout: &str, key: &str) -> Vec<PathBuf> {
    serde_json::from_str::<serde_json::Value>(stdout)
        .ok()
        .and_then(|v| {
            Some(
                v.get(key)?
                    .as_array()?
                    .iter()
                    .filter_map(|p| Some(PathBuf::from(p.as_str()?)))
                    .collect(),
            )
        })
        .unwrap_or_default()
}

/// The real project with the most session bytes on this machine.
///
/// [`encode`] is lossy, so a directory name cannot be turned back into a path —
/// but the transcripts inside state their own `cwd`, which is how this reads
/// one back. Candidates are tried biggest-first and the first one that is still
/// a git checkout wins; `None` where nothing qualifies.
fn biggest_real_repo(projects: &Path) -> Option<PathBuf> {
    let mut dirs: Vec<(u64, PathBuf)> = fs::read_dir(projects)
        .ok()?
        .filter_map(|e| {
            let p = e.ok()?.path();
            p.is_dir().then(|| {
                (
                    walk(&p)
                        .iter()
                        .filter_map(|f| fs::metadata(f).ok())
                        .map(|m| m.len())
                        .sum(),
                    p,
                )
            })
        })
        .collect();
    dirs.sort_by_key(|(bytes, _)| std::cmp::Reverse(*bytes));
    dirs.iter().find_map(|(_, dir)| {
        // A prefix of one file, not the file: a real transcript can be 100 MB,
        // and `cwd` is absent from the leading record kinds (`summary`,
        // `queue-operation`) but present on every `user`/`assistant` line.
        let file = walk(dir).into_iter().next()?;
        let cwd = BufReader::new(File::open(file).ok()?)
            .lines()
            .take(200)
            .map_while(Result::ok)
            .find_map(|l| {
                let v = serde_json::from_str::<serde_json::Value>(&l).ok()?;
                Some(PathBuf::from(v.get("cwd")?.as_str()?))
            })?;
        cwd.join(".git").exists().then_some(cwd)
    })
}

/// Every `*.jsonl` under `dir`, recursively.
///
/// Recursively because the connector is: a real project directory keeps its
/// subagent transcripts in `subagents/`, and counting only the top level
/// reported 62 MB of a 171 MB corpus while `recount` scanned all of it.
fn walk(dir: &Path) -> Vec<PathBuf> {
    let (mut out, mut todo) = (Vec::new(), vec![dir.to_path_buf()]);
    while let Some(d) = todo.pop() {
        for entry in fs::read_dir(d).into_iter().flatten().filter_map(Result::ok) {
            let p = entry.path();
            if p.is_dir() {
                todo.push(p);
            } else if p.extension().is_some_and(|x| x == "jsonl") {
                out.push(p);
            }
        }
    }
    out
}
