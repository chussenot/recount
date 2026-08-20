//! The keyed tier, the model comparison and `testify`, against the real
//! binary and a tree a reviewer can check by eye.
//!
//! Everything asserted below is readable in `tests/fixtures2/` without running
//! anything. There is one repo, one worktree `wt/scribe`, and exactly two
//! transcripts in the session directory `wt/scribe` encodes to:
//!
//! ```text
//! <encoded wt/scribe>/11111111-2222-3333-4444-555555555555.jsonl
//! <encoded wt/scribe>/11111111-2222-3333-4444-555555555555/subagents/agent-a1ded78ef5b657fa6.jsonl
//! ```
//!
//! `events.jsonl` is five rows, all agent `scribe`, all at `10:00:00Z`, and the
//! only thing that changes between them is the attribution chain. That is the
//! point: one query, five chains, five different tiers and reasons.
//!
//! | line | chain                                | answer                                    |
//! |-----:|--------------------------------------|-------------------------------------------|
//! | 1    | session + subagent `a1ded78ef…`      | keyed -> the **subagent** file            |
//! | 2    | session + subagent `bbbbbbbbb…`      | refused, `no keyed transcript`            |
//! | 3    | session, no subagent                 | keyed -> the **top-level** file           |
//! | 4    | none at all (pact 0.10.7)            | topological -> the ladder, 0.65           |
//! | 5    | as line 1, `PACT_MODEL` dated-undated | keyed, and the model comparison agrees   |
//!
//! # Line 1 and line 4 are the same question and different answers
//!
//! The ladder *prefers the parent transcript*: both files sit at the same cwd,
//! so topology is ambiguous and cannot discriminate (0.2 + 0.2), and only the
//! parent says the word `scribe`, which is the mention tiebreaker (+0.25 =
//! 0.65). The subagent file scores 0.4 and loses.
//!
//! So line 1 answering with the **subagent** file is not the ladder agreeing by
//! luck — it is the keyed tier overruling the ladder's own preference, which is
//! the property that would silently rot if the fixture had been built so both
//! tiers happened to pick the same file. [`the_keyed_tier_overrules_the_ladder`]
//! asserts both halves in one test for exactly that reason.
//!
//! # The refusal is asserted by reason, never by exit code
//!
//! Line 2 exits 1. So does a `no cwd match` and so does a `no overlap`, and
//! sending an auditor to the topology or the clock when the answer is "that
//! file is not on this disk" is the confident-wrong-answer this tool exists to
//! prevent. Every refusal test here pins the reason string and the two causes
//! the detail has to carry.
//!
//! # Where the model transcripts come in
//!
//! `model_check` needs both sides — a declared `PACT_MODEL` on the event and a
//! `.message.model` on the transcript — and a confident match to hang them on.
//! A keyed answer is the cheapest confident match there is, so each model case
//! is the same tree with a different file installed at the keyed path. What the
//! file records is what the verdict must be, and the file is three lines long.

mod common;

use std::path::Path;

use common::{Fixture, Run};
use serde_json::Value;

/// The session uuid every row of `tests/fixtures2/events.jsonl` names.
const SESSION: &str = "11111111-2222-3333-4444-555555555555";
/// The subagent id lines 1 and 5 name, and the file that exists.
const SUB: &str = "a1ded78ef5b657fa6";
/// The subagent id line 2 names. Nothing is ever written at its path.
const ABSENT_SUB: &str = "bbbbbbbbbbbbbbbbb";

/// `<session>/subagents/agent-<id>.jsonl` — the nested layout `claude_code`
/// documents and [`crate::common::Fixture::session2`] creates the parents for.
fn subagent_file(sub: &str) -> String {
    format!("{SESSION}/subagents/agent-{sub}.jsonl")
}

/// The tree of the module doc, with `subagent` installed as the keyed file.
///
/// Every test takes the same tree and changes one thing, so a failure is always
/// about the thing that changed.
fn tree(subagent: &str) -> Fixture {
    let f = Fixture::new();
    let wt = f.worktree("scribe");
    f.events2("events.jsonl");
    f.session2(&wt, &format!("{SESSION}.jsonl"), "keyed-parent.jsonl");
    f.session2(&wt, &subagent_file(SUB), subagent);
    f
}

/// `recount --json explain --event-line <n>`, which is the only command that
/// can reach the keyed tier: `at`'s own flags cannot name a transcript.
fn explain(f: &Fixture, line: u32) -> Run {
    let run = f.run(&[
        "--json",
        "explain",
        "--repo",
        &f.repo().to_string_lossy(),
        "--event-line",
        &line.to_string(),
    ]);
    assert_ne!(
        run.code, 101,
        "explain line {line} PANICKED:\n{}",
        run.stderr
    );
    run
}

/// The evidence detail for one signal, or a panic naming what was there
/// instead. Every answer carries a `keyed` line — an inferred one says why it
/// is not keyed — so this never has to cope with an absence.
fn evidence(doc: &Value, signal: &str) -> String {
    let list = doc["best"]["evidence"]
        .as_array()
        .unwrap_or_else(|| panic!("no evidence on {doc}"));
    list.iter()
        .find(|e| e["signal"] == signal)
        .unwrap_or_else(|| panic!("no {signal} evidence in {list:?}"))
        .get("detail")
        .and_then(Value::as_str)
        .expect("evidence detail is a string")
        .to_string()
}

fn file_name(path: &str) -> &str {
    Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap()
}

fn best_path(doc: &Value) -> String {
    doc["best"]["session"]["path"]
        .as_str()
        .unwrap_or_else(|| panic!("no winning session in {doc}"))
        .to_string()
}

/// Line 1 answers with the subagent file; line 4 — same agent, same instant,
/// no chain — answers with the parent. One tree, two tiers, two files.
///
/// The second half is what makes the first half mean something. See the module
/// doc.
#[test]
fn the_keyed_tier_overrules_the_ladder() {
    let f = tree("keyed-subagent.jsonl");

    let keyed = explain(&f, 1);
    assert_eq!(keyed.code, 0, "stderr:\n{}", keyed.stderr);
    let doc = keyed.json();
    assert_eq!(doc["outcome"], "match");
    assert_eq!(doc["refusal"], Value::Null);

    // The two fields that say an identity was named rather than inferred.
    assert_eq!(doc["best"]["tier"], "keyed");
    assert_eq!(
        doc["best"]["confidence"],
        Value::Null,
        "a keyed answer has no position on the ladder, and the key is present \
         and null rather than absent"
    );
    assert_eq!(doc["best"]["confident"], true);

    // The file, by name.
    assert_eq!(
        file_name(&best_path(&doc)),
        format!("agent-{SUB}.jsonl"),
        "the keyed tier must answer with the file the event named"
    );
    let why = evidence(&doc, "keyed");
    assert!(
        why.contains(&format!("agent-{SUB}.jsonl")),
        "the evidence must name the filename that proved it: {why}"
    );
    assert!(
        why.contains("one path join and one stat"),
        "the evidence must say how it was reached: {why}"
    );
    assert!(
        why.contains("the topological ladder was not consulted"),
        "{why}"
    );
    // Only the keyed signal: no topology, no overlap, no mention.
    assert_eq!(
        doc["best"]["evidence"].as_array().unwrap().len(),
        1,
        "a keyed answer scores nothing, so it has one line of evidence"
    );

    // ... and the ladder, on the same question, prefers the OTHER file — and
    // says so as a LEAD and exit 1, because the chain could not tell the two
    // candidates apart and the transcript mention is what picked this one.
    // That contrast is the whole argument for the keyed tier: same question,
    // an identity above and a lead below.
    let ladder = explain(&f, 4);
    assert_eq!(
        ladder.code, 1,
        "a lead is not a match: stderr:\n{}",
        ladder.stderr
    );
    let doc = ladder.json();
    assert_eq!(doc["best"]["tier"], "topological");
    assert_eq!(
        file_name(&best_path(&doc)),
        format!("{SESSION}.jsonl"),
        "the ladder prefers the parent — only it says the word `scribe`"
    );
    assert_eq!(
        doc["best"]["confidence"], 0.65,
        "0.2 ambiguous topology + 0.2 overlap + 0.25 mention — the score is still \
         computed and still printed"
    );
    assert_eq!(doc["best"]["lead"], true);
    assert_eq!(doc["best"]["confident"], false);
    assert!(
        evidence(&doc, "topology").contains("THIS IS A LEAD, NOT A CONFIDENT ANSWER"),
        "the reader is told in words, not left to infer it from 0.65: {}",
        evidence(&doc, "topology")
    );
}

/// Line 2: the id is well-formed, the path is right, and nothing is at it.
///
/// Asserting exit 1 alone would pass for `no cwd match` and for `no overlap`,
/// which is the wrong answer told confidently. The reason string and both of
/// the detail's two causes are the assertion.
#[test]
fn a_keyed_id_whose_file_is_absent_refuses_with_its_own_reason() {
    let f = tree("keyed-subagent.jsonl");

    // Control: the same tree answers line 1, so nothing below is a broken tree.
    assert_eq!(explain(&f, 1).code, 0);

    let run = explain(&f, 2);
    assert_eq!(run.code, 1, "stdout:\n{}", run.stdout);
    let doc = run.json();
    assert_eq!(doc["outcome"], "refused");
    assert_eq!(doc["best"], Value::Null, "a refusal names no candidate");

    let reason = doc["refusal"]["reason"].as_str().unwrap();
    assert_eq!(reason, "no keyed transcript");
    assert_ne!(reason, "no cwd match", "the topology was never consulted");
    assert_ne!(reason, "no overlap", "the clock was never consulted");

    let detail = doc["refusal"]["detail"].as_str().unwrap();
    assert!(
        detail.contains(&format!("agent-{ABSENT_SUB}.jsonl")),
        "the refusal must name the file it wanted: {detail}"
    );
    // BOTH causes. They send the reader to different places — this machine's
    // backups, or the machine the ledger came from — and a refusal that names
    // one of them is a refusal that has guessed.
    assert!(
        detail.contains("PRUNED"),
        "cause 1 (retention or a tidied worktree) is missing: {detail}"
    );
    assert!(
        detail.contains("LEDGER CAME FROM ANOTHER MACHINE"),
        "cause 2 (the transcript was never here) is missing: {detail}"
    );
    assert!(
        detail.contains("looked at"),
        "the refusal must say where it looked: {detail}"
    );

    // The digest says the same thing, so stderr is not narrower than the JSON.
    assert!(
        run.stderr.contains("no keyed transcript"),
        "digest:\n{}",
        run.stderr
    );
}

/// Line 3: `harness_session` and no `harness_subagent` — the row a top-level
/// agent writes. It names `<session>.jsonl`, beside the `subagents/` directory
/// and not inside it, and that is an identity too.
#[test]
fn a_session_with_no_subagent_id_is_keyed_to_the_top_level_file() {
    let f = tree("keyed-subagent.jsonl");

    let run = explain(&f, 3);
    assert_eq!(run.code, 0, "stderr:\n{}", run.stderr);
    let doc = run.json();
    assert_eq!(doc["best"]["tier"], "keyed");
    assert_eq!(doc["best"]["confidence"], Value::Null);

    let path = best_path(&doc);
    assert_eq!(file_name(&path), format!("{SESSION}.jsonl"));
    assert!(
        !path.contains("subagents"),
        "the top-level file sits beside subagents/, not in it: {path}"
    );

    // Same tier as line 1, a different sentence: a reader has to be able to
    // tell "a subagent wrote this" from "the agent that owns the session did".
    let why = evidence(&doc, "keyed");
    assert!(why.contains("TOP-LEVEL"), "{why}");
    assert!(why.contains("is nobody's subagent"), "{why}");
    assert!(
        !why.contains("harness_subagent `"),
        "there is no subagent to name here: {why}"
    );
}

/// Line 4 is a pact 0.10.7 row: no chain at all, so it reaches the ladder.
///
/// The sentence it falls through with has to say the row is **old**, not that
/// an id is missing from it. Those are different findings — one sends the
/// reader to the ledger looking for a defect that is not there, the other says
/// there is nothing to look for — and the distinction was deliberate.
#[test]
fn a_pre_contract_row_falls_through_saying_it_is_old_not_defective() {
    let f = tree("keyed-subagent.jsonl");

    let run = explain(&f, 4);
    // Exit 1, and not for a missing chain: the row reaches the ladder, the
    // ladder answers, and the answer is a lead. The candidate and its evidence
    // are printed either way, which is what the rest of this test reads.
    assert_eq!(run.code, 1, "stderr:\n{}", run.stderr);
    let doc = run.json();
    assert_eq!(doc["best"]["tier"], "topological");
    assert!(
        doc["best"]["confidence"].as_f64().is_some(),
        "an inferred answer carries its number, lead or not: {doc}"
    );

    let why = evidence(&doc, "keyed");
    assert!(why.contains("PRE-CONTRACT row"), "{why}");
    assert!(why.contains("it is OLD, not defective"), "{why}");
    assert!(
        why.contains("no id is missing from it"),
        "the row is not being reported as defective: {why}"
    );
    assert!(
        why.contains("nothing to go and look for"),
        "and the reader is told not to go hunting: {why}"
    );
    assert!(
        why.contains("INFERRED by the topological ladder, not keyed"),
        "every inferred answer says it is not the keyed one: {why}"
    );

    // The chain is on the report, all five links absent, which is what a
    // pre-contract row honestly looks like.
    let chain = &doc["chain"];
    assert_eq!(chain["harness"], Value::Null);
    assert_eq!(chain["harness_session"], Value::Null);
    assert_eq!(chain["harness_subagent"], Value::Null);
    assert_eq!(
        doc["model_check"],
        Value::Null,
        "no declared model, so there is no comparison to make and nothing is said"
    );
}

/// The three verdicts `at` implements, plus the two hazards that would make it
/// lie.
///
/// Each row installs one transcript at the keyed path and reads the verdict
/// off the same document. What the file records is what the verdict must be,
/// and the file is short enough to check by eye.
#[test]
fn the_model_comparison_agrees_switches_differs_and_forgives_a_date() {
    // (transcript, ledger line, declared, verdict, recorded models)
    let cases: &[(&str, u32, &str, &str, &[&str])] = &[
        // Line 1 declares claude-opus-5.
        (
            "keyed-subagent.jsonl",
            1,
            "claude-opus-5",
            "agrees",
            &["claude-opus-5"],
        ),
        (
            "model-differs.jsonl",
            1,
            "claude-opus-5",
            "differs",
            &["claude-fable-5"],
        ),
        // Two real models with the declared one among them: the declaration
        // held for part of the run. `recorded` is most-frequent-first, so the
        // declared model is deliberately NOT recorded[0] here.
        (
            "model-switched.jsonl",
            1,
            "claude-opus-5",
            "switched",
            &["claude-fable-5", "claude-opus-5"],
        ),
        // Hazard: line 5 declares the undated slug, the API returned the dated
        // one. Same model. An exact compare would call this drift and accuse an
        // agent that did exactly what it was told.
        (
            "model-dated.jsonl",
            5,
            "claude-haiku-4-5",
            "agrees",
            &["claude-haiku-4-5-20251001"],
        ),
    ];

    for (fixture, line, declared, verdict, recorded) in cases {
        let f = tree(fixture);
        let run = explain(&f, *line);
        assert_eq!(run.code, 0, "{fixture}: stderr:\n{}", run.stderr);
        let doc = run.json();
        assert_eq!(doc["best"]["tier"], "keyed", "{fixture}");

        let check = &doc["model_check"];
        assert_eq!(check["declared"], *declared, "{fixture}");
        assert_eq!(check["verdict"], *verdict, "{fixture}: {check}");
        assert_eq!(
            check["recorded"],
            serde_json::json!(recorded),
            "{fixture}: most frequent first"
        );

        // The digest says it in words, on the stream the digest went to.
        let said = format!("{}{}", run.stdout, run.stderr);
        let words = match *verdict {
            "agrees" => "they agree",
            "switched" => "SWITCHED model",
            _ => "ORCHESTRATION DRIFT",
        };
        assert!(said.contains(words), "{fixture}: digest says no {words}");
    }
}

/// `<synthetic>` is the stub Claude Code writes on its own API-error records.
/// The line is still testimony — "the agent got a 429" is what an audit wants
/// to read — but it is not a model that answered, and reporting it as one would
/// turn every rate-limited run into an orchestration-drift finding.
#[test]
fn synthetic_is_testimony_but_never_a_recorded_model() {
    let f = tree("model-synthetic.jsonl");
    let run = explain(&f, 1);
    assert_eq!(run.code, 0, "stderr:\n{}", run.stderr);
    let doc = run.json();

    let check = &doc["model_check"];
    assert_eq!(
        check["recorded"],
        serde_json::json!(["claude-opus-5"]),
        "the synthetic stub must not reach the recorded set"
    );
    assert_eq!(
        check["verdict"], "agrees",
        "the 429 must not turn a clean run into drift"
    );

    // ... and the turn itself is in the transcript, which is the other half.
    let transcript = serde_json::to_string(&doc["best"]["transcript"]).unwrap();
    assert!(
        transcript.contains("API Error: 429"),
        "the error turn is testimony and must still be printed: {transcript}"
    );
}

/// `pact audit --json | recount testify`, from a canned document.
///
/// `tests/fixtures2/audit-findings.json` is shaped like the real thing, key by
/// key: pact's audit document has **no `findings` key**, each check writes its
/// own array, and four top-level arrays are statistics rather than findings.
/// This one carries all four (`agents`, `annotations`, `top_contended`,
/// `per_agent`) populated beside two real findings, so a reader that lost the
/// denylist would report 5 findings instead of 2.
///
/// The two real ones are chosen to be different in kind:
///
/// * `silent_contentions[0]` names `refused_agent` and `refused_at` and points
///   at ledger `line: 1` — which is the keyed row, so this finding is testified
///   through the keyed tier and gets a transcript.
/// * `topology_mismatches[0]` is `{invoked_from, events}`: a fact about a
///   fleet, about no agent and no moment. It passes through untouched, and that
///   is not a failure.
#[test]
fn testify_annotates_a_canned_audit_and_ignores_the_statistics_arrays() {
    let f = tree("keyed-subagent.jsonl");
    let doc_path = f.file2("audit.json", "audit-findings.json");

    let run = f.run(&[
        "--json",
        "testify",
        "--repo",
        &f.repo().to_string_lossy(),
        "--file",
        &doc_path.to_string_lossy(),
    ]);
    assert_eq!(run.code, 0, "stderr:\n{}", run.stderr);
    let out = run.json();

    assert_eq!(out["check"], "silent-contention", "the input's own check");
    assert_eq!(
        out["totals"],
        serde_json::json!({
            "findings": 2,
            "testified": 1,
            "untestified": 0,
            "passed_through": 1
        }),
        "3 statistics arrays and an empty double_wins contribute nothing"
    );

    let findings = out["findings"].as_array().unwrap();
    let by_check = |name: &str| {
        findings
            .iter()
            .find(|t| t["check"] == name)
            .unwrap_or_else(|| panic!("no {name} in {findings:?}"))
    };

    let contention = by_check("silent_contentions");
    assert_eq!(contention["agent"], "scribe", "picked from `refused_agent`");
    assert_eq!(
        contention["ts"], "2026-01-15T10:00:00Z",
        "picked from `refused_at`, not from `holder_released_at`"
    );
    assert_eq!(contention["event_line"], 1);
    assert_eq!(contention["outcome"], "match");
    assert_eq!(contention["testimony"]["tier"], "keyed");
    assert_eq!(contention["testimony"]["confidence"], Value::Null);
    assert_eq!(
        file_name(contention["testimony"]["session"]["path"].as_str().unwrap()),
        format!("agent-{SUB}.jsonl"),
        "ledger line 1 is the keyed row, so the finding reaches the keyed tier"
    );
    assert!(
        !contention["testimony"]["transcript"]
            .as_array()
            .unwrap()
            .is_empty(),
        "a confident match comes with its window of the transcript"
    );
    // Verbatim, every key: this is a pipeline stage, not a report.
    assert_eq!(
        contention["finding"]["refused_agent_had_a_channel"], false,
        "the finding comes back out unchanged"
    );

    let mismatch = by_check("topology_mismatches");
    assert_eq!(mismatch["outcome"], "passed_through");
    assert_eq!(mismatch["agent"], Value::Null);
    assert_eq!(mismatch["ts"], Value::Null);
    assert_eq!(mismatch["finding"]["invoked_from"], "main");

    // The four statistics arrays are not findings, and `agents` is the one that
    // would be loudest about it: two strings, neither of them an object.
    for stat in ["agents", "annotations", "top_contended", "per_agent"] {
        assert!(
            !findings.iter().any(|t| t["check"] == stat),
            "{stat} is a statistic, not a finding: {findings:?}"
        );
    }
}

/// A clean audit. Exit 0, no findings, and a sentence that says so — an empty
/// result is an answer, and testifying nothing is not a failure.
#[test]
fn an_empty_audit_is_answered_not_failed() {
    let f = tree("keyed-subagent.jsonl");
    let doc_path = f.file2("audit.json", "audit-empty.json");
    let args = [
        "testify",
        "--repo",
        &f.repo().to_string_lossy() as &str,
        "--file",
        &doc_path.to_string_lossy() as &str,
    ];

    let plain = f.run(&args);
    assert_eq!(
        plain.code, 0,
        "a clean audit is not a failure:\n{}",
        plain.stderr
    );
    assert!(
        plain
            .stdout
            .contains("nothing to testify: the audit found nothing"),
        "digest:\n{}",
        plain.stdout
    );

    let mut json_args = vec!["--json"];
    json_args.extend_from_slice(&args);
    let run = f.run(&json_args);
    assert_eq!(run.code, 0);
    let out = run.json();
    assert_eq!(out["check"], "double-win");
    assert_eq!(out["totals"]["findings"], 0);
    assert_eq!(out["findings"].as_array().unwrap().len(), 0);
    assert_eq!(out["untestified"].as_array().unwrap().len(), 0);
    // `agents` holds two strings here. If the denylist ever lapsed, this
    // document would stop being empty.
    assert_eq!(out["totals"]["passed_through"], 0);
}
