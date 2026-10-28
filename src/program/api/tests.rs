//! The calls of `api/` as the page makes them, in a conversation in the same process: pencere's
//! `Conversation` serves the handlers by `serve`'s own code (the threads, the lanes, a hand-off of
//! the pane) with no terminal between. What crosses a terminal — the size of a message, the
//! characters the stream escapes, the command line, the ways out — is held by the tests in
//! `tests/`, which run the program in a pane; these hold what a pane cannot show at will: calls sent
//! together, and the pane lent and given back.

use std::os::unix::process::ExitStatusExt;
use std::process::ExitStatus;
use std::sync::Arc;
use std::time::Duration;

use pencere::{Conversation, Options, Said};
use serde_json::{Value, json};

use super::serve;
use crate::state::session::Session;
use crate::state::store::Store;

/// A session on a project of its own, with its settings and drafts beside it.
fn session(files: &[(&str, &str)]) -> (tempfile::TempDir, Arc<Session>) {
    let dir = tempfile::tempdir().unwrap();
    let path = |name: &str| dir.path().join(name).to_str().unwrap().to_owned();
    std::fs::create_dir(dir.path().join("project")).unwrap();
    for (name, text) in files {
        std::fs::write(dir.path().join("project").join(name), text).unwrap();
    }
    let store = Store::at(path("config"), path("state"));
    let session = Session::new(&path("project"), &[], None, store);
    (dir, Arc::new(session))
}

fn conversation(session: &Arc<Session>) -> Conversation {
    Conversation::new(serve(
        Options::new().control(session.desk.control()),
        session,
    ))
}

fn send(conversation: &mut Conversation, id: u64, method: &str, params: Value) {
    let call = json!({ "call": { "id": id, "method": method, "params": params } });
    assert!(conversation.send(&call.to_string()), "{method} was taken");
}

/// The next thing the program says.
fn heard(conversation: &mut Conversation) -> Said {
    conversation
        .receive(Duration::from_secs(10))
        .expect("the program says something")
}

/// The next answer: its id, and its result or its error.
fn answer(conversation: &mut Conversation) -> (u64, Result<Value, Value>) {
    let Said::Message(text) = heard(conversation) else {
        panic!("an answer");
    };
    let message: Value = serde_json::from_str(&text).unwrap();
    let response = &message["response"];
    let id = response["id"].as_u64().unwrap();
    match response.get("result") {
        Some(result) => (id, Ok(result.clone())),
        None => (id, Err(response["error"].clone())),
    }
}

#[test]
fn a_hundred_edits_sent_together_are_made_in_the_order_they_were_sent() {
    let (dir, session) = session(&[("a.txt", "")]);
    let mut conversation = conversation(&session);
    send(&mut conversation, 1, "open", json!({ "path": "a.txt" }));
    assert_eq!(answer(&mut conversation).1.unwrap()["version"], json!(1));
    // Each made on the version the one before it leaves, none waiting for an answer.
    let letters: Vec<char> = "abcdefghijklmnopqrstuvwxyzş😀"
        .chars()
        .cycle()
        .take(100)
        .collect();
    for (at, letter) in letters.iter().enumerate() {
        let edit =
            json!({ "path": "a.txt", "base": at + 1, "edits": [[at, at, letter.to_string()]] });
        send(&mut conversation, 2 + at as u64, "edit", edit);
    }
    send(
        &mut conversation,
        102,
        "save",
        json!({ "path": "a.txt", "version": 101 }),
    );
    for at in 0..101u64 {
        let (id, result) = answer(&mut conversation);
        assert_eq!(id, 2 + at, "the answers come in the order of the calls");
        let result = result.unwrap_or_else(|error| panic!("call {id}: {error}"));
        if id <= 101 {
            assert_eq!(result["version"], json!(id));
        } else {
            assert_eq!(result["saved"], json!(true));
        }
    }
    let written = std::fs::read_to_string(dir.path().join("project/a.txt")).unwrap();
    assert_eq!(written, letters.iter().collect::<String>());
}

#[test]
fn the_pane_is_lent_after_the_answer_and_the_page_shown_again_is_told_how_the_shell_ended() {
    let (dir, session) = session(&[("a.txt", "world\n")]);
    let mut conversation = conversation(&session);
    send(&mut conversation, 1, "boot", json!({}));
    assert_eq!(answer(&mut conversation).1.unwrap()["handoff"], json!(true));
    send(&mut conversation, 2, "open", json!({ "path": "a.txt" }));
    answer(&mut conversation).1.unwrap();
    let edit = json!({ "path": "a.txt", "base": 1, "edits": [[5, 5, "!"]] });
    send(&mut conversation, 3, "edit", edit);
    answer(&mut conversation).1.unwrap();
    send(&mut conversation, 4, "handoff", json!({ "kind": "shell" }));
    assert_eq!(
        answer(&mut conversation),
        (4, Ok(json!({ "state": "scheduled" }))),
        "the answer goes out before the view closes"
    );
    let Said::HandedOff(lent) = heard(&mut conversation) else {
        panic!("the pane lent");
    };
    // The shell in the project, told it is inside textpen, after the drafts were written.
    let command = lent.command();
    assert_eq!(command.get_program(), "/bin/sh");
    assert_eq!(
        command.get_current_dir(),
        Some(dir.path().join("project").as_path())
    );
    assert!(
        command
            .get_envs()
            .any(|(name, value)| name == "TEXTPEN_SHELL" && value == Some("1".as_ref()))
    );
    let drafts = std::fs::read_dir(dir.path().join("state/drafts")).unwrap();
    assert_eq!(
        drafts.count(),
        1,
        "the drafts are written before the shell starts"
    );
    // Asked again while the shell has the pane, it is busy.
    assert_eq!(
        session.desk.shell(|| panic!("nothing is done")),
        crate::terminal::handoff::Asked::Busy
    );
    lent.returned(ExitStatus::from_raw(3 << 8));
    send(&mut conversation, 1, "boot", json!({}));
    let boot = answer(&mut conversation).1.unwrap();
    assert_eq!(boot["returned"]["exitCode"], json!(3));
    assert!(boot["returned"]["duration"].is_u64());
    assert!(boot["returned"].get("error").is_none());
    // Told once, and the buffer is the program's still.
    send(&mut conversation, 2, "boot", json!({}));
    let again = answer(&mut conversation).1.unwrap();
    assert!(again.get("returned").is_none());
    assert_eq!(again["buffers"][0]["dirty"], json!(true));
}

#[test]
fn a_shell_ended_by_a_signal_has_no_exit_code() {
    let (_dir, session) = session(&[]);
    let mut conversation = conversation(&session);
    send(&mut conversation, 1, "handoff", json!({ "kind": "shell" }));
    answer(&mut conversation).1.unwrap();
    let Said::HandedOff(lent) = heard(&mut conversation) else {
        panic!("the pane lent");
    };
    lent.returned(ExitStatus::from_raw(libc::SIGKILL));
    send(&mut conversation, 1, "boot", json!({}));
    let returned = &answer(&mut conversation).1.unwrap()["returned"];
    assert_eq!(returned["exitCode"], Value::Null);
}

#[test]
fn a_terminal_that_cannot_lend_its_pane_is_told_on_the_changes_and_not_asked_again() {
    let (_dir, session) = session(&[]);
    let mut conversation = conversation(&session);
    let subscribe = json!({ "subscribe": { "id": 1, "topic": "changes" } });
    assert!(conversation.send(&subscribe.to_string()));
    let pushed = |conversation: &mut Conversation| {
        let Said::Message(text) = heard(conversation) else {
            panic!("a push");
        };
        let message: Value = serde_json::from_str(&text).unwrap();
        message["push"]["value"].clone()
    };
    assert!(pushed(&mut conversation).get("handoffFailed").is_none());
    // The terminal answers that it cannot close its view (a conversation always can).
    session.desk.settled(&pencere::HandOffResult::Unsupported);
    assert_eq!(
        pushed(&mut conversation)["handoffFailed"],
        json!("This terminal cannot hand its pane to a command and back.")
    );
    send(&mut conversation, 2, "handoff", json!({ "kind": "shell" }));
    assert_eq!(
        answer(&mut conversation),
        (2, Ok(json!({ "state": "unavailable" })))
    );
    send(&mut conversation, 3, "boot", json!({}));
    assert_eq!(
        answer(&mut conversation).1.unwrap()["handoff"],
        json!(false)
    );
}

#[test]
fn quitting_closes_the_view_after_the_buffer_calls_sent_before_it() {
    let (dir, session) = session(&[("a.txt", "world\n")]);
    let mut conversation = conversation(&session);
    send(&mut conversation, 1, "open", json!({ "path": "a.txt" }));
    send(
        &mut conversation,
        2,
        "edit",
        json!({ "path": "a.txt", "base": 1, "edits": [[5, 5, "!"]] }),
    );
    send(
        &mut conversation,
        3,
        "save",
        json!({ "path": "a.txt", "version": 2 }),
    );
    send(&mut conversation, 4, "quit", json!({}));
    for id in 1..=3 {
        let (heard_id, result) = answer(&mut conversation);
        assert_eq!(heard_id, id);
        result.unwrap_or_else(|error| panic!("call {id}: {error}"));
    }
    let closed = loop {
        match heard(&mut conversation) {
            Said::Closed => break true,
            Said::Message(_) => {}
            _ => break false,
        }
    };
    assert!(closed, "the program closed its view");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("project/a.txt")).unwrap(),
        "world!\n"
    );
}

/// The names of a page of the file index.
fn names(page: &Value) -> Vec<String> {
    page["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|name| name.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn the_pages_of_the_file_index_are_of_one_reading_of_it_however_the_files_change_meanwhile() {
    let (dir, session) = session(&[]);
    let project = dir.path().join("project");
    // 1 200 names of 255 characters: more than one page holds.
    let name = |n: usize| format!("f{n:04}{}", "x".repeat(250));
    for n in 0..1200 {
        std::fs::write(project.join(name(n * 2)), "").unwrap();
    }
    let mut conversation = conversation(&session);
    send(&mut conversation, 1, "files", json!({}));
    let first = answer(&mut conversation).1.unwrap();
    let generation = first["generation"].as_u64().unwrap();
    let next = first["next"].as_u64().expect("more than one page");
    // Files are made among those of the first page, and the index is read again meanwhile.
    for n in 0..50 {
        std::fs::write(project.join(name(n * 2 + 1)), "").unwrap();
    }
    session.project.forget();
    assert_ne!(session.project.files().generation, generation);
    // The rest of the listing is of the files as its first page found them: each once.
    let mut all = names(&first);
    let (mut from, mut id) = (next, 2);
    loop {
        send(
            &mut conversation,
            id,
            "files",
            json!({ "from": from, "generation": generation }),
        );
        let page = answer(&mut conversation).1.unwrap();
        assert_eq!(page["generation"], json!(generation));
        all.extend(names(&page));
        match page["next"].as_u64() {
            Some(after) => from = after,
            None => break,
        }
        id += 1;
    }
    let unique: std::collections::HashSet<&String> = all.iter().collect();
    assert_eq!((all.len(), unique.len()), (1200, 1200));
    assert!((0..1200).all(|n| unique.contains(&name(n * 2))));
    // A listing begun since is of the files as they are; a page of the one before it is out of
    // step, for the page to read them from the start.
    send(&mut conversation, 100, "files", json!({}));
    let again = answer(&mut conversation).1.unwrap();
    assert_ne!(again["generation"], json!(generation));
    send(
        &mut conversation,
        101,
        "files",
        json!({ "from": next, "generation": generation }),
    );
    let stale = answer(&mut conversation).1.unwrap_err();
    assert_eq!(stale["code"], json!("out-of-sync"));
}

#[test]
fn a_file_is_not_moved_onto_the_name_of_a_buffer_with_changes_that_are_not_saved() {
    let (dir, session) = session(&[("a.txt", "a")]);
    let mut conversation = conversation(&session);
    // `a.txt` is open, and so is `b.txt`, which is not on disk: opened empty, typed in, not saved.
    send(&mut conversation, 2, "open", json!({ "path": "a.txt" }));
    answer(&mut conversation).1.unwrap();
    send(&mut conversation, 3, "open", json!({ "path": "b.txt" }));
    answer(&mut conversation).1.unwrap();
    let edit = json!({ "path": "b.txt", "base": 1, "edits": [[0, 0, "mine"]] });
    send(&mut conversation, 4, "edit", edit);
    answer(&mut conversation).1.unwrap();
    send(
        &mut conversation,
        4,
        "rename",
        json!({ "from": "a.txt", "to": "b.txt" }),
    );
    let (_, refused) = answer(&mut conversation);
    let error = refused.expect_err("the move is refused");
    assert_eq!(error["code"], json!("failed"), "{error}");
    assert!(dir.path().join("project/a.txt").exists());
    assert!(!dir.path().join("project/b.txt").exists());
    // Saved, `b.txt` is on disk, and the move is refused for that, as before.
    send(
        &mut conversation,
        5,
        "save",
        json!({ "path": "b.txt", "version": 2 }),
    );
    answer(&mut conversation).1.unwrap();
    send(
        &mut conversation,
        6,
        "rename",
        json!({ "from": "a.txt", "to": "b.txt" }),
    );
    let (_, refused) = answer(&mut conversation);
    assert!(
        refused.unwrap_err()["message"]
            .as_str()
            .unwrap()
            .contains("already exists")
    );
}

/// Runs git in `dir`, with no config of the machine's.
fn git(dir: &std::path::Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .status()
        .expect("git runs");
    assert!(status.success(), "git {args:?}");
}

/// What `diff` says of `path`: the version, the hunks (as `[line, count, old]`) or none, and `more`.
fn diffed(conversation: &mut Conversation, id: u64, path: &str) -> Value {
    send(conversation, id, "diff", json!({ "path": path }));
    answer(conversation).1.expect("an answer")
}

fn hunks_of(answer: &Value) -> Vec<(u64, u64, Vec<String>)> {
    answer["hunks"]
        .as_array()
        .expect("hunks")
        .iter()
        .map(|hunk| {
            (
                hunk["line"].as_u64().unwrap(),
                hunk["count"].as_u64().unwrap(),
                hunk["old"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|line| line.as_str().unwrap().to_owned())
                    .collect(),
            )
        })
        .collect()
}

#[test]
fn the_lines_that_differ_from_the_index_are_told_as_the_buffer_makes_them_and_staged_ones_are_not()
{
    let (dir, session) = session(&[("a.txt", "one\ntwo\nthree\n"), ("new.txt", "x\n")]);
    let project = dir.path().join("project");
    git(&project, &["init", "-q"]);
    git(&project, &["add", "a.txt"]);
    git(&project, &["commit", "-q", "-m", "first"]);
    let mut conversation = conversation(&session);
    send(&mut conversation, 1, "open", json!({ "path": "a.txt" }));
    answer(&mut conversation).1.unwrap();
    // As it was committed: no hunk, and none for a file git does not keep.
    let told = diffed(&mut conversation, 2, "a.txt");
    assert_eq!(
        (told["version"].clone(), hunks_of(&told)),
        (json!(1), vec![])
    );
    send(&mut conversation, 3, "open", json!({ "path": "new.txt" }));
    answer(&mut conversation).1.unwrap();
    assert_eq!(
        diffed(&mut conversation, 4, "new.txt")["hunks"],
        Value::Null
    );
    // The page's edit: `two` changed, a line added after `three`.
    send(
        &mut conversation,
        5,
        "edit",
        json!({ "path": "a.txt", "base": 1, "edits": [[4, 7, "TWO"], [14, 14, "four\n"]] }),
    );
    answer(&mut conversation).1.unwrap();
    let told = diffed(&mut conversation, 6, "a.txt");
    assert_eq!(told["version"], json!(2));
    assert_eq!(
        hunks_of(&told),
        vec![(1, 1, vec!["two".to_owned()]), (3, 1, vec![])]
    );
    // What is staged is what the buffer is compared with: no hunk where the index has the change.
    send(
        &mut conversation,
        7,
        "save",
        json!({ "path": "a.txt", "version": 2 }),
    );
    answer(&mut conversation).1.unwrap();
    git(&project, &["add", "a.txt"]);
    assert!(hunks_of(&diffed(&mut conversation, 8, "a.txt")).is_empty());
    // A call for a file that is not open fails as a buffer call does.
    send(
        &mut conversation,
        9,
        "diff",
        json!({ "path": "nothing.txt" }),
    );
    assert_eq!(
        answer(&mut conversation).1.unwrap_err()["code"],
        json!("out-of-sync")
    );
}

#[test]
fn a_file_with_crlf_line_ends_and_a_mark_is_compared_as_the_buffer_holds_it() {
    let (dir, session) = session(&[]);
    let project = dir.path().join("project");
    std::fs::write(project.join("w.txt"), "\u{feff}a\r\nb\r\nc\r\n").unwrap();
    git(&project, &["init", "-q"]);
    git(&project, &["-c", "core.autocrlf=false", "add", "w.txt"]);
    git(&project, &["commit", "-q", "-m", "first"]);
    let mut conversation = conversation(&session);
    send(&mut conversation, 1, "open", json!({ "path": "w.txt" }));
    answer(&mut conversation).1.unwrap();
    // The same text: no hunk, not one from the first line to the last.
    assert!(hunks_of(&diffed(&mut conversation, 2, "w.txt")).is_empty());
    send(
        &mut conversation,
        3,
        "edit",
        json!({ "path": "w.txt", "base": 1, "edits": [[2, 3, "B"]] }),
    );
    answer(&mut conversation).1.unwrap();
    assert_eq!(
        hunks_of(&diffed(&mut conversation, 4, "w.txt")),
        vec![(1, 1, vec!["b".to_owned()])]
    );
}

#[test]
fn a_link_a_file_outside_the_project_and_a_project_outside_a_repository_have_no_marks() {
    let (dir, session) = session(&[("real.txt", "a\n")]);
    let project = dir.path().join("project");
    let mut conversation = conversation(&session);
    // Not a repository: none.
    send(&mut conversation, 1, "open", json!({ "path": "real.txt" }));
    answer(&mut conversation).1.unwrap();
    assert_eq!(
        diffed(&mut conversation, 2, "real.txt")["hunks"],
        Value::Null
    );
    git(&project, &["init", "-q"]);
    git(&project, &["add", "real.txt"]);
    git(&project, &["commit", "-q", "-m", "first"]);
    std::os::unix::fs::symlink("real.txt", project.join("link.txt")).unwrap();
    let outside = dir.path().join("outside.txt");
    std::fs::write(&outside, "o\n").unwrap();
    // A link, and a file outside the root (an absolute path): none, repository or not.
    send(&mut conversation, 3, "open", json!({ "path": "link.txt" }));
    answer(&mut conversation).1.unwrap();
    assert_eq!(
        diffed(&mut conversation, 4, "link.txt")["hunks"],
        Value::Null
    );
    send(
        &mut conversation,
        5,
        "open",
        json!({ "path": outside.to_str().unwrap() }),
    );
    answer(&mut conversation).1.unwrap();
    assert_eq!(
        diffed(&mut conversation, 6, outside.to_str().unwrap())["hunks"],
        Value::Null
    );
}
