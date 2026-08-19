//! The spec's proof obligation, discharged against the real binary:
//!
//! > "Fixture ~/.claude tree + fixture events.jsonl with a KNOWN join: `at`
//! > finds it, and correctly refuses (exit 1 + reason) when the worktree is
//! > pruned. Truncated/garbage fixtures."
//!
//! # The join is known by construction
//!
//! Everything these tests assert is readable in `tests/fixtures/` without
//! running anything. Line 2 of `events.jsonl` is agent `alice`, invoked from
//! worktree `alice`, at `2026-01-15T10:00:00Z` — and the tests take the query
//! from that line rather than restating it, so the ledger is the source of
//! truth and cannot drift from them.
//!
//! Three transcripts are offered against that one query, and exactly one can
//! win:
//!
//! | fixture                 | cwd          | turns at          | why it loses      |
//! |-------------------------|--------------|-------------------|-------------------|
//! | `alice.jsonl`           | `wt/alice`   | 09:58 – 10:02     | **it wins**       |
//! | `bob.jsonl`             | `wt/bob`     | 09:59 – 10:01     | wrong worktree    |
//! | `alice-yesterday.jsonl` | `wt/alice`   | 01-14 09:58–10:02 | wrong day         |
//!
//! `bob.jsonl` says the word "alice" on purpose. The mention signal is a
//! tiebreaker between sessions the topology already admitted, never a way in:
//! a fixture where the loser never names the agent would pass whether or not
//! that rule held.
//!
//! # Both refusals are reasons, and they are not interchangeable
//!
//! `at` exits 1 for "no cwd match" (ask about the topology) and for "no
//! overlap" (ask about the clock). A pruned worktree is the first. Asserting
//! only on the exit code would pass for the wrong reason — which is the exact
//! class of confident-wrong-answer recount was written to catch — so
//! [`at_refuses_with_no_cwd_match_when_the_worktree_is_pruned`] pins the
//! string, after a control run that proves the same inputs answer 0 while the
//! directory is still there.

mod common;

use common::Fixture;

/// The fixture tree of the wave, wired up: two worktrees, three transcripts,
/// one ledger. Shared by the finding test and the refusal test because the
/// refusal has to be produced by removing *one* directory from the tree that
/// answers, and nothing else.
fn known_join() -> Fixture {
    let f = Fixture::new();
    let alice = f.worktree("alice");
    let bob = f.worktree("bob");
    f.events("events.jsonl");
    f.session(&alice, "alice-session.jsonl", "alice.jsonl");
    f.session(&alice, "alice-yesterday.jsonl", "alice-yesterday.jsonl");
    f.session(&bob, "bob-session.jsonl", "bob.jsonl");
    f
}

/// Line 2 of the installed ledger: `(agent, at)`. The query comes from the
/// event, the way `explain` will hand it over.
fn known_event(f: &Fixture) -> (String, String) {
    let ledger = std::fs::read_to_string(f.repo().join(".pact").join("events.jsonl")).unwrap();
    let line = ledger.lines().nth(1).expect("the ledger has a line 2");
    let event: serde_json::Value = serde_json::from_str(line).expect("line 2 parses");
    (
        event["agent"].as_str().unwrap().to_string(),
        event["at"].as_str().unwrap().to_string(),
    )
}

#[test]
fn at_finds_the_known_join() {
    let f = known_join();
    let (agent, at) = known_event(&f);
    assert_eq!(
        (agent.as_str(), at.as_str()),
        ("alice", "2026-01-15T10:00:00Z")
    );

    let run = f.run(&[
        "at",
        "--repo",
        &f.repo().to_string_lossy(),
        "--agent",
        &agent,
        "--ts",
        &at,
        "--json",
    ]);
    assert_eq!(run.code, 0, "stderr: {}", run.stderr);

    let doc = run.json();
    assert_eq!(doc["outcome"], "match");
    assert_eq!(doc["refusal"], serde_json::Value::Null);

    // The winner is the one the fixture says it must be, by file name.
    let best = &doc["best"];
    let path = best["session"]["path"].as_str().unwrap();
    assert!(
        path.ends_with("alice-session.jsonl"),
        "the wrong session won: {path}"
    );
    assert_eq!(best["session"]["turn_count"], 3);

    // 0.5 topology (the cwd IS wt/alice, so the chain discriminates)
    // + 0.2 overlap + 0.25 mention. Anything else means a different regime
    // decided, and the regime is the thing an audit reads.
    assert_eq!(best["confidence"], 0.95);
    assert_eq!(best["confident"], true);
    let evidence = serde_json::to_string(&best["evidence"]).unwrap();
    assert!(
        evidence.contains("topology A (process-per-agent)"),
        "evidence should name the regime: {evidence}"
    );
    assert!(
        evidence.contains("`alice` at line 1"),
        "the mention tiebreaker should be quoted: {evidence}"
    );

    // Exit 0 comes with the transcript: all three turns are inside 09:55–10:05.
    assert_eq!(best["transcript"].as_array().unwrap().len(), 3);

    // The losers have to be real losers. If a decoy's cwd were mistyped it
    // would simply vanish from the scan and "alice wins" would be vacuously
    // true, so check the tree the way `sessions` sees it: all three
    // transcripts discovered, each tied to the worktree its cwd names.
    let seen = f.run(&["sessions", "--repo", &f.repo().to_string_lossy(), "--json"]);
    assert_eq!(seen.code, 0, "stderr: {}", seen.stderr);
    let mut tied: Vec<(String, String)> = seen.json()["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            let path = s["path"].as_str().unwrap();
            let name = path.rsplit('/').next().unwrap().to_string();
            (name, s["worktree"].as_str().unwrap_or("-").to_string())
        })
        .collect();
    tied.sort();
    assert_eq!(
        tied,
        vec![
            ("alice-session.jsonl".into(), "alice".into()),
            ("alice-yesterday.jsonl".into(), "alice".into()),
            ("bob-session.jsonl".into(), "bob".into()),
        ]
    );
}

#[test]
fn at_refuses_with_no_cwd_match_when_the_worktree_is_pruned() {
    let f = known_join();
    let (agent, at) = known_event(&f);
    let args = [
        "at",
        "--repo",
        &f.repo().to_string_lossy() as &str,
        "--agent",
        &agent,
        "--ts",
        &at,
        "--json",
    ];

    // Control: with wt/alice on disk these exact inputs answer.
    let found = f.run(&args);
    assert_eq!(found.code, 0, "control run: {}", found.stderr);
    assert_eq!(found.json()["outcome"], "match");

    // The only change: the worktree directory is gone. git still lists it, the
    // transcripts it produced are still in ~/.claude, and the ledger still
    // names it.
    f.prune_worktree("alice");

    let run = f.run(&args);
    assert_eq!(run.code, 1, "a pruned worktree is a refusal, not an answer");

    let doc = run.json();
    assert_eq!(doc["outcome"], "refused");
    assert_eq!(doc["best"], serde_json::Value::Null);

    // THE assertion. Exit 1 alone would pass for "no overlap" too, and that
    // would send an auditor to the clock when the answer is the topology.
    let reason = doc["refusal"]["reason"].as_str().unwrap();
    assert_eq!(reason, "no cwd match");
    assert_ne!(reason, "no overlap");

    let detail = doc["refusal"]["detail"].as_str().unwrap();
    assert!(
        detail.contains("gone from disk") && detail.contains("alice"),
        "the refusal must say which worktree and why: {detail}"
    );
    // The digest carries the same reason, so a human reading stderr is not
    // told something narrower than the JSON.
    assert!(
        run.stderr.contains("no cwd match"),
        "stderr digest: {}",
        run.stderr
    );
}

/// "One bad line never fails a file" (spec), against the two counters wave 4
/// split apart: `parse_failures` is DAMAGE, `declined` is a line this reader
/// chose not to take. `damaged.jsonl` has six lines — two whole turns, three
/// kinds of garbage and a truncated tail — and every one of its four bad lines
/// is damage. Asserting `declined == 0` is the half that catches the
/// conflation: a reader that counted garbage as "declined" would still show
/// four somewhere.
#[test]
fn truncated_and_garbage_lines_are_counted_not_fatal() {
    let f = Fixture::new();
    let alice = f.worktree("alice");
    f.session(&alice, "alice-session.jsonl", "alice.jsonl");
    f.session(&alice, "damaged-session.jsonl", "damaged.jsonl");

    let run = f.run(&["sessions", "--repo", &f.repo().to_string_lossy(), "--json"]);
    assert_eq!(run.code, 0, "a damaged transcript is not an error");

    let doc = run.json();
    let sessions = doc["sessions"].as_array().unwrap();
    let find = |suffix: &str| {
        sessions
            .iter()
            .find(|s| s["path"].as_str().unwrap().ends_with(suffix))
            .unwrap_or_else(|| panic!("{suffix} missing from {sessions:?}"))
    };

    let damaged = find("damaged-session.jsonl");
    assert_eq!(
        damaged["turn_count"], 2,
        "the whole lines still became turns"
    );
    assert_eq!(
        damaged["parse_failures"], 4,
        "not JSON, a bare array, a turn with no timestamp, and a truncated tail"
    );
    assert_eq!(
        damaged["declined"], 0,
        "garbage is damage, never a line recount declined to read"
    );

    // And the file next to it is untouched: one damaged transcript does not
    // cost the scan.
    let clean = find("alice-session.jsonl");
    assert_eq!(clean["turn_count"], 3);
    assert_eq!(clean["parse_failures"], 0);
    assert_eq!(clean["declined"], 0);
}
