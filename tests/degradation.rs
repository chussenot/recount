//! The spec's integration obligation, discharged against the real binary:
//!
//! > "Integration: run every command against a repo with events but NO
//! > sessions — calm degradation, never a panic (recount will often run where
//! > transcripts don't exist)."
//!
//! That parenthetical is the whole rationale. recount is reached for *after*
//! something has already gone wrong, usually on a machine that is not the one
//! the transcripts were written on: a CI runner checking out a repo, an
//! auditor reading a ledger someone mailed them, a `--repo` typo. Every one of
//! those paths lands in this file.
//!
//! # What "calm" is asserted to mean
//!
//! Three things, for every command in every shape below, and each of them is a
//! bug this file exists to catch:
//!
//! 1. **Never 101.** That is the exit code a Rust panic produces, and it is the
//!    single most important assertion here. The exit code is 0 or 1 — never
//!    101, and never 2 either. 2 is clap's usage-error code, so a command
//!    returning it for a well-formed invocation would mean the parser refused
//!    input these tests believe is valid.
//! 2. **Never half a document.** Under `--json` stdout carries exactly one
//!    JSON value or nothing at all. A tool that emits an unterminated object
//!    when the answer is empty is worse than one that errors, because the
//!    caller piping to `jq` is precisely who meets these paths.
//! 3. **A message a human can act on.** Each case pins the words the output has
//!    to contain. An exit code with no sentence behind it sends the reader
//!    hunting for a problem that may not exist — the arkanoid misdiagnosis in
//!    one command.
//!
//! # The stream split, restated as an invariant
//!
//! Every command prints the same human digest either way; `--json` only moves
//! it. So for anything that produces a report, [`check`] asserts
//! `json.stderr == plain.stdout` byte for byte, which is a stronger statement
//! than "both streams are non-empty" and catches a command that summarises
//! itself differently depending on the flag.
//!
//! # Reports and errors are different contracts, on purpose
//!
//! [`Out`] names which one each case is. A *report* is an answer, even when
//! the answer is "nothing": empty is not an error, so `sessions` and `grep`
//! exit 0 with an empty list, `at` exits 1 with a named reason, and `doctor`
//! exits 0 on every input there is. An *error* is a refusal to answer at all —
//! a `--repo` that is not a repo, a ledger line that is not there — and it
//! prints one sentence to stderr, nothing to stdout, and never a partial
//! document.

mod common;

use std::path::{Path, PathBuf};

use common::{encode, Fixture};

/// Which contract a case is asserted against. See the module doc.
#[derive(Debug, Clone, Copy)]
enum Out {
    /// An answer, with this exit code. `--json` puts one document on stdout
    /// and the digest on stderr; without it the digest is alone on stdout.
    Report(i32),
    /// A refusal to answer: exit 1, one sentence on stderr, stdout untouched
    /// in both modes.
    Error,
}

struct Case {
    args: Vec<String>,
    out: Out,
    /// Substrings the output must contain, wherever it went — the "message a
    /// human can act on" half of the obligation.
    says: Vec<String>,
}

fn case(args: &[&str], out: Out, says: &[&str]) -> Case {
    Case {
        args: args.iter().map(|s| s.to_string()).collect(),
        out,
        says: says.iter().map(|s| s.to_string()).collect(),
    }
}

/// Run one case both ways and assert the three properties in the module doc.
fn check(f: &Fixture, c: &Case) {
    let argv: Vec<&str> = c.args.iter().map(String::as_str).collect();
    let label = format!("recount {}", argv.join(" "));

    let plain = f.run(&argv);
    let mut json_argv = vec!["--json"];
    json_argv.extend_from_slice(&argv);
    let json = f.run(&json_argv);

    for (mode, r) in [("", &plain), (" --json", &json)] {
        assert_ne!(
            r.code, 101,
            "{label}{mode} PANICKED — the one thing this file exists to catch\n{}",
            r.stderr
        );
        assert!(
            matches!(r.code, 0 | 1),
            "{label}{mode} exited {} — the contract is 0 or 1\n{}",
            r.code,
            r.stderr
        );
    }

    let want = match c.out {
        Out::Report(code) => code,
        Out::Error => 1,
    };
    assert_eq!(
        plain.code, want,
        "{label}\n{}{}",
        plain.stdout, plain.stderr
    );
    assert_eq!(
        json.code, want,
        "{label} --json\n{}{}",
        json.stdout, json.stderr
    );

    match c.out {
        Out::Report(_) => {
            assert!(
                !plain.stdout.trim().is_empty(),
                "{label} answered with an empty stdout"
            );
            assert!(
                plain.stderr.is_empty(),
                "{label} wrote to stderr without --json:\n{}",
                plain.stderr
            );
            // Panics with both streams attached if stdout is not exactly one
            // JSON value — trailing bytes included.
            let doc = json.json();
            assert!(
                doc.get("command").is_some() && doc.get("schema").is_some(),
                "{label} --json lacks the house envelope: {doc}"
            );
            assert_eq!(
                json.stderr, plain.stdout,
                "{label}: the digest must be the same text, only the stream moves"
            );
        }
        Out::Error => {
            assert!(
                plain.stdout.is_empty(),
                "{label} errored but still printed a report:\n{}",
                plain.stdout
            );
            assert!(
                json.stdout.is_empty(),
                "{label} --json errored but still printed part of a document:\n{}",
                json.stdout
            );
            assert!(
                plain.stderr.starts_with("recount: "),
                "{label}: an error must say who is speaking:\n{}",
                plain.stderr
            );
            assert_eq!(
                json.stderr, plain.stderr,
                "{label}: --json must not change the error"
            );
        }
    }

    let said = format!("{}{}", plain.stdout, plain.stderr);
    for needle in &c.says {
        assert!(
            said.contains(needle.as_str()),
            "{label}: nothing actionable — expected {needle:?} in\n{said}"
        );
    }
}

fn check_all(f: &Fixture, cases: &[Case]) {
    for c in cases {
        check(f, c);
    }
}

fn s(p: &Path) -> String {
    p.display().to_string()
}

/// The `~/.claude/projects` directory this repo's sessions would live in.
fn session_dir(f: &Fixture) -> PathBuf {
    f.home()
        .join(".claude")
        .join("projects")
        .join(encode(f.repo()))
}

/// A directory beside the fixture's repo and home, so it is a real path that
/// is emphatically not a git repository.
fn sibling(f: &Fixture, name: &str) -> PathBuf {
    let p = f.repo().parent().expect("fixture base").join(name);
    std::fs::create_dir_all(&p).expect("mkdir sibling");
    p
}

/// A ledger written by hand rather than installed from `tests/fixtures/`, for
/// the shapes that are not valid JSONL and so cannot be committed as one.
fn write_ledger(f: &Fixture, body: &[u8]) {
    let path = f.repo().join(".pact").join("events.jsonl");
    std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir .pact");
    std::fs::write(&path, body).expect("write ledger");
}

/// The worst case there is: a directory with nothing in it, no `~/.claude`, no
/// `.pact`, and not a git repository. Four of the five say so and stop; doctor
/// reports all four absences and exits 0, because doctor is the command you
/// reach for once something else has already refused.
#[test]
fn a_bare_directory_is_named_not_crashed_into() {
    let f = Fixture::new();
    let bare = s(&sibling(&f, "bare"));
    let not_git = "is not a git repository";

    check_all(
        &f,
        &[
            case(
                &["sessions", "--repo", &bare],
                Out::Error,
                &[&bare, not_git],
            ),
            case(
                &[
                    "at",
                    "--repo",
                    &bare,
                    "--agent",
                    "a",
                    "--ts",
                    "2026-01-15T10:00:00Z",
                ],
                Out::Error,
                &[&bare, not_git],
            ),
            case(
                &["grep", "x", "--repo", &bare],
                Out::Error,
                &[&bare, not_git],
            ),
            case(
                &["explain", "--repo", &bare, "--event-line", "1"],
                Out::Error,
                &["no pact ledger at", &bare, "nothing to explain here"],
            ),
            // Doctor's whole argument: a doctor that exits non-zero gets
            // wrapped in `|| true` and then its exit code means nothing. The
            // machine-readable verdict is `join_readiness.ready`.
            case(
                &["doctor", "--repo", &bare],
                Out::Report(0),
                &[not_git, "ABSENT", "NOT READY", "does not exist"],
            ),
        ],
    );

    let ready = f.run(&["--json", "doctor", "--repo", &bare]).json()["join_readiness"].clone();
    assert_eq!(ready["ready"], false);
    assert_eq!(ready["repo_is_git"], false);
    assert_eq!(ready["events_present"], false);
}

/// The bead, literally: a real git repo, a real ledger, and not one transcript
/// anywhere. Nothing here is an error except the refusal `at` is specified to
/// make, and that refusal names which of the two reasons applies.
#[test]
fn a_repo_with_events_and_no_sessions_answers_every_command() {
    let f = Fixture::new();
    f.events("events.jsonl");
    let repo = s(f.repo());

    check_all(
        &f,
        &[
            case(
                &["sessions", "--repo", &repo],
                Out::Report(0),
                &["no sessions", "not a failure"],
            ),
            // Line 2 of the fixture ledger: agent `alice` at 10:00Z.
            case(
                &[
                    "at",
                    "--repo",
                    &repo,
                    "--agent",
                    "alice",
                    "--ts",
                    "2026-01-15T10:00:00Z",
                ],
                Out::Report(1),
                &["no confident match", "no cwd match"],
            ),
            case(
                &["grep", "alice", "--repo", &repo],
                Out::Report(0),
                &["no hits", "not a failure"],
            ),
            case(
                &["explain", "--repo", &repo, "--event-line", "2"],
                Out::Report(1),
                &["line 2", "agent `alice`", "no cwd match"],
            ),
            case(
                &["doctor", "--repo", &repo],
                Out::Report(0),
                &["3 events", "there is no testimony to join", "NOT READY"],
            ),
            // The spec writes `recount doctor` bare; cwd is the repo.
            case(&["doctor"], Out::Report(0), &["NOT READY"]),
        ],
    );

    let sessions = f.run(&["--json", "sessions", "--repo", &repo]).json();
    assert_eq!(sessions["sessions"].as_array().map(Vec::len), Some(0));
    let grep = f.run(&["--json", "grep", "alice", "--repo", &repo]).json();
    assert_eq!(grep["hits"].as_array().map(Vec::len), Some(0));
    let at = f
        .run(&[
            "--json",
            "at",
            "--repo",
            &repo,
            "--agent",
            "alice",
            "--ts",
            "2026-01-15T10:00:00Z",
        ])
        .json();
    assert_eq!(at["outcome"], "refused");
    assert_eq!(at["refusal"]["reason"], "no cwd match");
    assert!(
        !at["refusal"]["detail"]
            .as_str()
            .unwrap_or_default()
            .is_empty(),
        "a refusal is never silent"
    );
}

/// `~/.claude/projects` exists and is empty — a machine where the agent has
/// run at least once, but never here. Distinct from the directory being
/// absent, and doctor is the command that has to tell them apart.
#[test]
fn an_empty_projects_root_is_not_a_missing_one() {
    let f = Fixture::new();
    f.events("events.jsonl");
    std::fs::create_dir_all(f.home().join(".claude").join("projects")).expect("mkdir projects");
    let repo = s(f.repo());

    check_all(
        &f,
        &[
            case(
                &["sessions", "--repo", &repo],
                Out::Report(0),
                &["no sessions"],
            ),
            case(
                &[
                    "at",
                    "--repo",
                    &repo,
                    "--agent",
                    "alice",
                    "--ts",
                    "2026-01-15T10:00:00Z",
                ],
                Out::Report(1),
                &["no cwd match"],
            ),
            case(
                &["grep", "alice", "--repo", &repo],
                Out::Report(0),
                &["no hits"],
            ),
            case(
                &["explain", "--repo", &repo, "--event-line", "1"],
                Out::Report(1),
                &["agent `bob`", "no cwd match"],
            ),
            case(&["doctor", "--repo", &repo], Out::Report(0), &["(exists)"]),
        ],
    );

    let roots =
        f.run(&["--json", "doctor", "--repo", &repo]).json()["connectors"][0]["roots"][0].clone();
    assert_eq!(roots["exists"], true, "an empty root still exists");
}

/// The encoded session directory is there, but holds no `*.jsonl` — a checkout
/// whose transcripts were deleted, or a directory some other tool made. The
/// scan must count the directory and find nothing in it, not skip it and not
/// trip over the file that is there.
#[test]
fn a_session_directory_with_no_transcripts_is_scanned_and_empty() {
    let f = Fixture::new();
    f.events("events.jsonl");
    let dir = session_dir(&f);
    std::fs::create_dir_all(&dir).expect("mkdir session dir");
    std::fs::write(dir.join("notes.txt"), "not a transcript\n").expect("write notes");
    let repo = s(f.repo());

    check_all(
        &f,
        &[
            case(
                &["sessions", "--repo", &repo],
                Out::Report(0),
                &["1 session dir(s)", "no sessions"],
            ),
            case(
                &[
                    "at",
                    "--repo",
                    &repo,
                    "--agent",
                    "alice",
                    "--ts",
                    "2026-01-15T10:00:00Z",
                ],
                Out::Report(1),
                &["1 session dir(s)", "no cwd match"],
            ),
            case(
                &["grep", "alice", "--repo", &repo],
                Out::Report(0),
                &["1 session dir(s)", "no hits"],
            ),
            case(
                &["explain", "--repo", &repo, "--event-line", "3"],
                Out::Report(1),
                &["line 3", "no cwd match"],
            ),
            case(&["doctor", "--repo", &repo], Out::Report(0), &["NOT READY"]),
        ],
    );
}

/// A zero-byte ledger. Not the same thing as no ledger at all, and doctor is
/// specified to report them differently: `events_present` is true, `events` is
/// 0. `explain` has to say the ledger is empty rather than that the file is
/// missing, or the reader goes looking for a file that is right there.
#[test]
fn an_empty_ledger_is_present_and_says_so() {
    let f = Fixture::new();
    write_ledger(&f, b"");
    let repo = s(f.repo());

    check_all(
        &f,
        &[
            case(
                &["explain", "--repo", &repo, "--event-line", "1"],
                Out::Error,
                &[
                    "no event on line 1",
                    "lines are 1-based",
                    "the ledger has 0",
                ],
            ),
            case(
                &["doctor", "--repo", &repo],
                Out::Report(0),
                &["present, 0 events"],
            ),
            case(
                &["sessions", "--repo", &repo],
                Out::Report(0),
                &["no sessions"],
            ),
            case(
                &["grep", "alice", "--repo", &repo],
                Out::Report(0),
                &["no hits"],
            ),
            case(
                &[
                    "at",
                    "--repo",
                    &repo,
                    "--agent",
                    "alice",
                    "--ts",
                    "2026-01-15T10:00:00Z",
                ],
                Out::Report(1),
                &["no cwd match"],
            ),
        ],
    );

    let readiness = f.run(&["--json", "doctor", "--repo", &repo]).json()["join_readiness"].clone();
    assert_eq!(readiness["events_present"], true, "the file is there");
    assert_eq!(
        readiness["events"], 0,
        "and it is empty — a different finding"
    );
}

/// A ledger of pure garbage, including bytes that are not UTF-8. Every line is
/// counted and none parses; the tally is the answer, and no command dies on
/// the way to printing it.
#[test]
fn a_garbage_ledger_costs_lines_not_the_run() {
    let f = Fixture::new();
    write_ledger(&f, b"not json at all\n{{{\n\x00\x01\xff\xfe\n");
    let repo = s(f.repo());

    check_all(
        &f,
        &[
            case(
                &["explain", "--repo", &repo, "--event-line", "1"],
                Out::Error,
                &["line 1 of 3 did not parse as an event"],
            ),
            case(
                &["doctor", "--repo", &repo],
                Out::Report(0),
                &["3 of 3 lines did not parse as events"],
            ),
            case(
                &["sessions", "--repo", &repo],
                Out::Report(0),
                &["no sessions"],
            ),
            case(
                &["grep", "alice", "--repo", &repo],
                Out::Report(0),
                &["no hits"],
            ),
            case(
                &[
                    "at",
                    "--repo",
                    &repo,
                    "--agent",
                    "alice",
                    "--ts",
                    "2026-01-15T10:00:00Z",
                ],
                Out::Report(1),
                &["no cwd match"],
            ),
        ],
    );

    let readiness = f.run(&["--json", "doctor", "--repo", &repo]).json()["join_readiness"].clone();
    assert_eq!(readiness["events"], 0);
    assert_eq!(readiness["event_parse_failures"], 3);
}

/// A ledger cut off mid-line, which is what a ledger being appended to while
/// it is copied looks like. The truncated line costs itself and nothing else,
/// and asking for the line after it is an out-of-range error, not a panic.
#[test]
fn a_truncated_ledger_costs_only_its_last_line() {
    let f = Fixture::new();
    let whole = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("events.jsonl"),
    )
    .expect("read fixture ledger");
    // Two whole lines, then a third cut in half.
    let cut = whole
        .iter()
        .enumerate()
        .filter(|(_, b)| **b == b'\n')
        .map(|(i, _)| i)
        .nth(1)
        .expect("fixture has 3 lines")
        + 40;
    write_ledger(&f, &whole[..cut]);
    let repo = s(f.repo());

    check_all(
        &f,
        &[
            case(
                &["explain", "--repo", &repo, "--event-line", "2"],
                Out::Report(1),
                &["line 2", "agent `alice`"],
            ),
            case(
                &["explain", "--repo", &repo, "--event-line", "3"],
                Out::Error,
                &["line 3 of 3 did not parse as an event"],
            ),
            case(
                &["explain", "--repo", &repo, "--event-line", "4"],
                Out::Error,
                &["no event on line 4", "the ledger has 3"],
            ),
            case(
                &["doctor", "--repo", &repo],
                Out::Report(0),
                &["present, 2 events, 1 unparseable"],
            ),
        ],
    );
}

/// `--event-line` off either end. The error names the line count, so the next
/// thing the reader types is right.
#[test]
fn an_event_line_out_of_range_names_the_line_count() {
    let f = Fixture::new();
    f.events("events.jsonl");
    let repo = s(f.repo());

    check_all(
        &f,
        &[
            case(
                &["explain", "--repo", &repo, "--event-line", "0"],
                Out::Error,
                &[
                    "no event on line 0",
                    "lines are 1-based",
                    "the ledger has 3",
                ],
            ),
            case(
                &["explain", "--repo", &repo, "--event-line", "4"],
                Out::Error,
                &["no event on line 4", "the ledger has 3"],
            ),
            case(
                &["explain", "--repo", &repo, "--event-line", "999999"],
                Out::Error,
                &["no event on line 999999", "the ledger has 3"],
            ),
        ],
    );
}

/// A `--repo` that is not there at all, and one that is a file. Both are
/// typos, both are common, and neither may be reported as "no sessions found"
/// — which is the misdiagnosis recount was written to prevent.
#[test]
fn a_repo_that_is_absent_or_is_a_file_is_named_as_such() {
    let f = Fixture::new();
    f.events("events.jsonl");
    let gone = s(&f.repo().parent().expect("base").join("nope"));
    let file = s(&f.repo().join("README.md"));

    for (repo, why) in [
        (&gone, "cannot resolve path"),
        (&file, "is not a git repository"),
    ] {
        check_all(
            &f,
            &[
                case(&["sessions", "--repo", repo], Out::Error, &[repo, why]),
                case(
                    &[
                        "at",
                        "--repo",
                        repo,
                        "--agent",
                        "a",
                        "--ts",
                        "2026-01-15T10:00:00Z",
                    ],
                    Out::Error,
                    &[repo, why],
                ),
                case(&["grep", "x", "--repo", repo], Out::Error, &[repo, why]),
                case(
                    &["explain", "--repo", repo, "--event-line", "1"],
                    Out::Error,
                    &["no pact ledger at", repo],
                ),
                // Doctor still runs. It is the only way to find out why.
                case(
                    &["doctor", "--repo", repo],
                    Out::Report(0),
                    &["NOT READY", repo],
                ),
            ],
        );
    }
}

/// The one hard error `grep` has. Everything else it can be handed is a
/// question with the answer "no hits".
#[test]
fn an_empty_needle_is_the_one_grep_refuses() {
    let f = Fixture::new();
    f.events("events.jsonl");
    let repo = s(f.repo());

    check_all(
        &f,
        &[
            case(
                &["grep", "", "--repo", &repo],
                Out::Error,
                &["empty needle", "that is not a search"],
            ),
            // A needle nothing can contain is still a normal answer.
            case(
                &["grep", "\u{fffd}no-such-needle", "--repo", &repo],
                Out::Report(0),
                &["no hits"],
            ),
        ],
    );
}
