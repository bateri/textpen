//! textpen in a terminal pane, end to end: the view is opened, the page's calls reach the program
//! through the tty, and the ways out (⌃C, the page closing, a terminal that shows no views) leave
//! the files and the drafts as they should. See `common` for how the stand-in terminal is found.

// A test that cannot go on fails where it stands, helpers included.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};

use common::{Pane, Script, answers, count, host, program, run};
use serde_json::json;

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

/// A shell for the pane: a script that runs `body` and ends with `status`.
fn shell(pane: &Pane, body: &str, status: i32) -> String {
    pane.write("shell", format!("#!/bin/sh\n{body}\nexit {status}\n"));
    let path = pane.path("shell");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path.to_str().unwrap().to_owned()
}

#[test]
fn the_pane_goes_to_a_shell_in_the_project_and_the_editor_comes_back_told_how_it_ended() {
    let pane = pane();
    std::fs::create_dir(pane.path("proj")).unwrap();
    // What the shell is given: its directory, and the switches of the rc files' integrations.
    let out = pane.path("where.out");
    let shell = shell(
        &pane,
        &format!(
            "echo \"$PWD $TEXTPEN_SHELL $ITERM_SHELL_INTEGRATION_INSTALLED $WEZTERM_SHELL_SKIP_ALL\" >> '{}'",
            out.display()
        ),
        3,
    );
    // Each view the terminal shows runs the page's calls: the second view is told how the shell
    // ended, asks for it again, and ⌃C ends textpen before the second shell has the pane.
    let ran = pane.run(
        &Script::new()
            .call("boot", json!({}))
            .call("handoff", json!({ "kind": "shell" }))
            .then(&["--on-view", "2", "--then", "interrupt"])
            .host_args(&["--passthrough"])
            .args(&["proj"])
            .env("SHELL", &shell),
    );
    let log = &ran.stderr;
    assert!(log.contains("program: dismiss"), "{log}");
    assert!(log.contains("reason=program"), "{log}");
    assert_eq!(count(log, "view: started from the bundle"), 2, "{log}");
    // The second view is served from what the terminal kept of the first.
    assert!(
        log.lines()
            .filter(|line| line.starts_with("host: need id="))
            .nth(1)
            .is_some_and(|line| line.ends_with("missing=0")),
        "{log}"
    );
    let heard = answers(log);
    assert_eq!(heard.len(), 4, "{log}");
    assert_eq!(heard[0].result()["handoff"], json!(true));
    assert!(heard[0].result().get("returned").is_none());
    assert_eq!(heard[1].result(), &json!({ "state": "scheduled" }));
    let returned = &heard[2].result()["returned"];
    assert_eq!(returned["exitCode"], json!(3), "{log}");
    assert!(returned["duration"].is_u64() && returned.get("error").is_none());
    // (/tmp is a link on a Mac: the shell is in the same directory by whichever name.)
    let said = std::fs::read_to_string(&out).unwrap();
    assert!(
        said.starts_with(&format!("{}/proj 1 Yes 1\n", pane.root())),
        "{said}"
    );
    // Its first line says where it is and how to come back.
    assert!(
        ran.stdout
            .contains(&format!("shell in \u{1b}[1m{}/proj", pane.root())),
        "{}",
        ran.stdout
    );
    assert!(ran.stdout.contains("exit or ⌃D goes back to the editor"));
    assert!(log.contains("program: exited with status 130"), "{log}");
}

#[test]
fn a_shell_that_cannot_start_is_told_to_the_editor_that_comes_back() {
    let pane = pane();
    let ran = pane.run(
        &Script::new()
            .call("boot", json!({}))
            .call("handoff", json!({ "kind": "shell" }))
            .then(&["--on-view", "2", "--then", "interrupt"])
            .env("SHELL", "/no/such/shell"),
    );
    let heard = answers(&ran.stderr);
    assert_eq!(heard.len(), 4, "{}", ran.stderr);
    let returned = &heard[2].result()["returned"];
    // The shell's word for a command it could not run: 127 (not found), or 126 where `exec` says it
    // could not run it (macOS's sh).
    assert!(
        [json!(126), json!(127)].contains(&returned["exitCode"]),
        "{returned}"
    );
    assert_eq!(returned["error"], json!("could not start /no/such/shell"));
}

#[test]
fn a_hangup_while_the_shell_has_the_pane_ends_textpen_and_keeps_what_is_not_saved() {
    let pane = pane();
    let (started, hup) = (pane.path("started"), pane.path("hup.out"));
    // The shell takes away the drafts written as it started, so that the ones there after the
    // hangup are the ones textpen wrote on its way out; it says it heard the hangup.
    let shell = shell(
        &pane,
        &format!(
            "trap \"echo hup > '{}'; exit 129\" HUP\nrm -rf \"$XDG_STATE_HOME/textpen/drafts\"\ntouch '{}'\nsleep 30 &\nwait $!",
            hup.display(),
            started.display()
        ),
        0,
    );
    let script = Script::new()
        .call("open", json!({ "path": "a.txt" }))
        .call(
            "edit",
            json!({ "path": "a.txt", "base": 1, "edits": [[5, 5, "😀"]] }),
        )
        .call("handoff", json!({ "kind": "shell" }))
        .then(&["--on-view", "2", "--then", "interrupt"])
        .args(&["a.txt"])
        .limit(40)
        .env("SHELL", &shell);
    let log = pane.path("log");
    let mut command = pane.host_command(&script);
    command.stderr(std::fs::File::create(&log).unwrap());
    let mut host = command.spawn().unwrap();
    let waited = |what: &str, done: &dyn Fn() -> bool| {
        let until = Instant::now() + Duration::from_secs(20);
        while !done() {
            assert!(Instant::now() < until, "waited for {what}");
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    waited("the shell to start", &|| started.exists());
    assert!(pane.drafts().is_empty());
    // The terminal goes away, as when an ssh connection drops.
    host.kill().unwrap();
    host.wait().unwrap();
    let record = std::fs::read_to_string(&log).unwrap();
    let pid: i32 = record
        .split("(pid ")
        .nth(1)
        .and_then(|rest| rest.split(')').next())
        .and_then(|pid| pid.parse().ok())
        .expect("the record names textpen's pid");
    // SAFETY: a signal of nothing asks only whether the process is there.
    waited("textpen to end", &|| unsafe { libc::kill(pid, 0) } != 0);
    assert_eq!(std::fs::read_to_string(&hup).unwrap(), "hup\n");
    let drafts = pane.drafts();
    assert_eq!(drafts.len(), 1, "the drafts are written on the way out");
    let draft: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&drafts[0]).unwrap()).unwrap();
    assert_eq!(draft["text"], json!("world😀\n"));
    assert_eq!(pane.read("a.txt"), "world\n");
}

// The view is closed by the person, as a terminal's own way of closing it does (`--then close`).
// A page of the kit has no close of its own (the protocol, Section 10.6; pencere's set 009 holds it
// so in its stand-in: a kit page's `close` keeps its view): its program ends the view, as textpen's
// does when the page calls `quit` (api/tests.rs). So the person closing the view is the test here,
// and not a stand-in for one.
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
        // A piece is a quarter of a message at most (docs/calls.md), and nearly that.
        assert!(text.len() <= 256 * 1024 && text.len() > 250 * 1024);
        // Character 150,000 begins line 1,500.
        assert!(text.starts_with('ş'));
    }
}
