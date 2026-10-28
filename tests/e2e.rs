//! textpen in a terminal pane, end to end: the view is opened, the page's calls reach the program
//! through the tty, and the ways out (⌃C, the page closing, a terminal that shows no views) leave
//! the files and the drafts as they should. See `common` for how the stand-in terminal is found.

// A test that cannot go on fails where it stands, helpers included.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{Pane, Script, answers, count, host, program, run};
use serde_json::json;
use std::time::Duration;

/// How many answers the page heard, and how many of them were errors.
fn tally(log: &str) -> (usize, usize) {
    let heard = answers(log);
    let failed = heard
        .iter()
        .filter(|answer| matches!(answer, common::Answer::Error { .. }))
        .count();
    (heard.len(), failed)
}

fn on_a_txt() -> Script {
    Script::new().args(&["a.txt"])
}

fn pane() -> Pane {
    let pane = Pane::new();
    pane.write("a.txt", "world\n");
    pane
}

#[test]
fn the_page_is_shown_the_view_and_hears_the_program() {
    let pane = pane();
    let log = pane.record(&on_a_txt().call("boot", json!({})));
    assert!(
        log.lines()
            .any(|line| line.starts_with("program: open id=") && line.contains(" title=\"Editor\"")),
        "{log}"
    );
    assert!(
        log.contains("view: title=\"Editor\" program=\"textpen\""),
        "{log}"
    );
    assert!(log.contains("view: icons png 32x32 dark"), "{log}");
    assert_eq!(tally(&log), (1, 0));
}

#[test]
fn an_edit_sent_through_the_tty_is_saved_to_the_file() {
    let pane = pane();
    let log = pane.record(
        &on_a_txt()
            .call("open", json!({ "path": "a.txt" }))
            .call(
                "edit",
                json!({ "path": "a.txt", "base": 1, "edits": [[0, 0, "hello "]] }),
            )
            .call("save", json!({ "path": "a.txt", "version": 2 })),
    );
    assert_eq!(tally(&log), (3, 0));
    assert!(
        log.contains("reason=signal") && log.contains("program: bye id="),
        "{log}"
    );
    assert!(log.contains("alternate screen off"), "{log}");
    assert_eq!(pane.read("a.txt"), "hello world\n");
}

#[test]
fn an_edit_on_a_version_the_program_no_longer_holds_is_refused() {
    let pane = pane();
    let log = pane.record(&on_a_txt().call("open", json!({ "path": "a.txt" })).call(
        "edit",
        json!({ "path": "a.txt", "base": 7, "edits": [[0, 0, "x"]] }),
    ));
    assert!(answers(&log)[1].failed("out-of-sync"), "{log}");
    assert_eq!(pane.read("a.txt"), "world\n");
}

#[test]
fn the_program_tells_the_page_what_changed_and_the_page_hears_it() {
    let pane = pane();
    let log = pane.record(
        &on_a_txt()
            .call("open", json!({ "path": "a.txt" }))
            .host_args(&["--subscribe", "changes"]),
    );
    assert!(log.contains("view: -> subscribe id="), "{log}");
    let pushes = common::pushes(&log);
    assert_eq!(
        pushes.first(),
        Some(&json!({ "buffers": [], "dirs": [], "git": null }))
    );
}

#[test]
fn ctrl_c_keeps_what_is_not_saved_as_a_draft_and_the_next_run_offers_it() {
    let pane = pane();
    pane.record(&on_a_txt().call("open", json!({ "path": "a.txt" })).call(
        "edit",
        json!({ "path": "a.txt", "base": 1, "edits": [[5, 5, "!"]] }),
    ));
    assert_eq!(pane.read("a.txt"), "world\n");
    assert_eq!(pane.drafts().len(), 1);
    let log = pane.record(
        &on_a_txt()
            .call("open", json!({ "path": "a.txt" }))
            .call("buffer", json!({ "path": "a.txt", "action": "restore" }))
            .call("save", json!({ "path": "a.txt", "version": 2 })),
    );
    assert_eq!(tally(&log), (3, 0));
    assert_eq!(pane.read("a.txt"), "world!\n");
}

#[test]
fn the_pane_is_not_handed_to_a_shell_until_pencere_can_take_the_view_back() {
    // (The shell, its directory, its environment and the view shown again come with the next
    // pencere, which hands a view over and takes it back.)
    let pane = pane();
    let log = pane.record(
        &on_a_txt()
            .call("boot", json!({}))
            .call("handoff", json!({ "kind": "shell" })),
    );
    let heard = answers(&log);
    assert_eq!(heard[0].result()["handoff"], json!(false));
    assert_eq!(heard[1].result(), &json!({ "state": "unavailable" }));
    assert_eq!(count(&log, "view: started from the bundle"), 1);
    assert!(!log.contains("program: dismiss"), "{log}");
}

// The view is closed by the person (`--then close`): the stand-in of this pencere passes a page of
// the kit's own `close` to the program as a message (host/fake/src/host.rs, `react`), so `--then
// page-close` does not close it. That is pencere's to mend (handed to its set 009); the program's
// way out is the same for both.
#[test]
fn closing_the_view_ends_textpen_with_success() {
    let pane = pane();
    let log = pane.record(&on_a_txt().then(&["--then", "close"]));
    assert!(log.contains("reason=user"), "{log}");
    assert!(log.contains("program: exited with status 0"), "{log}");
}

#[test]
fn closing_the_view_keeps_what_is_not_saved_as_a_draft() {
    let pane = pane();
    let log = pane.record(
        &on_a_txt()
            .call("open", json!({ "path": "a.txt" }))
            .call(
                "edit",
                json!({ "path": "a.txt", "base": 1, "edits": [[5, 5, "!"]] }),
            )
            .then(&["--then", "close"]),
    );
    assert!(log.contains("program: exited with status 0"), "{log}");
    assert_eq!(pane.read("a.txt"), "world\n");
    assert_eq!(pane.drafts().len(), 1);
}

#[test]
fn quitting_without_saving_leaves_no_draft() {
    let pane = pane();
    pane.record(
        &on_a_txt()
            .call("open", json!({ "path": "a.txt" }))
            .call(
                "edit",
                json!({ "path": "a.txt", "base": 1, "edits": [[5, 5, "!"]] }),
            )
            .call("discard", json!({}))
            .then(&["--then", "close"]),
    );
    assert_eq!(pane.read("a.txt"), "world\n");
    assert!(!pane.exists("state/textpen/drafts"));
}

#[test]
fn a_terminal_without_pencere_runs_the_persons_own_editor_with_textpen_passed_over() {
    let pane = pane();
    let mut command = pane.command(host());
    command
        .args(["--no-host", "--passthrough", "--limit", "15", "--"])
        .arg(program())
        .arg("a.txt")
        .env("VISUAL", "textpen")
        .env("EDITOR", "cat -n");
    let ran = run(command, None, Duration::from_secs(30));
    assert!(ran.stdout.contains("1\tworld"), "{}", ran.stdout);
    assert!(
        ran.stderr.contains("exited with status 0"),
        "{}",
        ran.stderr
    );
}

#[test]
fn a_text_too_long_for_one_message_is_read_in_pieces_of_whole_characters() {
    // A message is 1 MiB at most: 30,000 lines of 99 ş are 5.97 MB and 3,000,000 characters.
    let pane = pane();
    pane.write("a.txt", format!("{}\n", "ş".repeat(99)).repeat(30_000));
    let log = pane.record(
        &on_a_txt()
            .call("open", json!({ "path": "a.txt" }))
            .call("slice", json!({ "path": "a.txt", "version": 1, "from": 0 }))
            .call(
                "slice",
                json!({ "path": "a.txt", "version": 1, "from": 150_000 }),
            ),
    );
    assert_eq!(tally(&log), (3, 0));
    let heard = answers(&log);
    let opened = heard[0].result();
    assert_eq!(opened["text"], json!(""));
    assert_eq!(opened["partial"], json!({ "length": 3_000_000 }));
    for (answer, from) in [(&heard[1], 0), (&heard[2], 150_000)] {
        let piece = answer.result();
        let text = piece["text"].as_str().expect("a text");
        let end = piece["end"].as_u64().expect("an end");
        assert_eq!(end - from, text.chars().count() as u64);
        assert!(text.len() <= 900 * 1024 && text.len() > 800 * 1024);
        // Character 150,000 begins line 1,500.
        assert!(text.starts_with('ş'));
    }
}
