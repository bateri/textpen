//! A file the person gives the page crossing the terminal into the project, and the other way: the
//! stand-in terminal drops a real file on the page (`--drop`) and the page's calls pass its handle on
//! (`$1`), so the bytes travel the stream as they do in bateri, in pieces, with the progress the page
//! is told; or the path is passed, and the file is copied on the machine. Nothing is written over a
//! file unasked, and a file given up (the page cancels, ⌃C) leaves no half file behind.
//!
//! See `common` for how the stand-in terminal is found.

// A test that cannot go on fails where it stands, helpers included.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{Answer, Pane, Script, count};
use serde_json::{Value, json};

/// A file of `size` bytes that is not the same all through.
fn bytes(size: usize) -> Vec<u8> {
    (0..size).map(|n| (n * 31 % 251) as u8).collect()
}

/// The stand-in drops the file at `path` on the page as it starts; `$1` is its handle.
fn dropping(path: &std::path::Path) -> Vec<String> {
    vec!["--drop".to_owned(), path.to_string_lossy().into_owned()]
}

fn script(calls: &[(&str, Value)], host: &[String]) -> Script {
    let mut script = Script::new();
    for (method, params) in calls {
        script = script.call(method, params.clone());
    }
    let host: Vec<&str> = host.iter().map(String::as_str).collect();
    script.host_args(&host)
}

/// What the folder of the project holds, parts and all.
fn left_in(pane: &Pane, dir: &str) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(pane.path(dir))
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// A pane with a file of the person's outside the project (`given.bin` of three pieces and a bit)
/// and a folder to put it in.
fn pane_with_a_file() -> (Pane, Vec<u8>, std::path::PathBuf) {
    let pane = Pane::new();
    std::fs::create_dir_all(pane.path("project/docs")).unwrap();
    let data = bytes(3 * 1024 * 1024 + 123);
    pane.write("outside/given.bin", &data);
    let given = pane.path("outside/given.bin");
    (pane, data, given)
}

fn upload_params(size: usize, name: &str) -> Value {
    json!({ "dir": "docs", "name": name, "handle": "$1", "size": size })
}

fn in_project(pane: &Pane, script: Script) -> Script {
    script.args(&[pane.path("project").to_str().unwrap()])
}

#[test]
fn a_file_given_by_its_handle_crosses_the_terminal_in_pieces_and_the_page_is_told_as_it_goes() {
    let (pane, data, given) = pane_with_a_file();
    let run = script(
        &[("upload", upload_params(data.len(), "given.bin"))],
        &dropping(&given),
    );
    let log = pane.record(&in_project(&pane, run));
    let answers = common::answers(&log);
    assert_eq!(
        answers[0].result(),
        &json!({ "path": "docs/given.bin", "size": data.len() })
    );
    assert_eq!(
        std::fs::read(pane.path("project/docs/given.bin")).unwrap(),
        data
    );
    // Four pieces were asked of the terminal, and the page was told after each.
    assert!(count(&log, "program: read ") >= 4, "{log}");
    assert!(count(&log, "view: <- progress id=1") >= 3, "{log}");
    assert_eq!(
        left_in(&pane, "project/docs"),
        ["given.bin"],
        "no part left"
    );
}

#[test]
fn where_the_page_is_handed_the_path_the_file_is_copied_on_the_machine_and_no_byte_crosses_the_terminal()
 {
    let (pane, data, given) = pane_with_a_file();
    let mut params = upload_params(data.len(), "given.bin");
    params["path"] = json!(given.to_string_lossy());
    let run = script(&[("upload", params)], &dropping(&given));
    let log = pane.record(&in_project(&pane, run));
    assert_eq!(common::answers(&log)[0].result()["size"], json!(data.len()));
    assert_eq!(
        std::fs::read(pane.path("project/docs/given.bin")).unwrap(),
        data
    );
    assert_eq!(count(&log, "program: read "), 0, "{log}");
    assert!(count(&log, "view: <- progress id=1") >= 3, "{log}");
    assert_eq!(left_in(&pane, "project/docs"), ["given.bin"]);
}

#[test]
fn a_path_that_is_no_longer_the_file_the_page_was_handed_is_received_by_the_handle() {
    let (pane, data, given) = pane_with_a_file();
    // The size the page was handed is not the file's now.
    let mut params = upload_params(data.len() - 1, "given.bin");
    params["path"] = json!(given.to_string_lossy());
    params["size"] = json!(data.len() - 1);
    let run = script(&[("upload", params)], &dropping(&given));
    let log = pane.record(&in_project(&pane, run));
    assert_eq!(
        std::fs::read(pane.path("project/docs/given.bin")).unwrap(),
        data
    );
    assert!(count(&log, "program: read ") >= 4, "{log}");
}

#[test]
fn a_name_that_is_taken_is_refused_and_untouched_unless_the_page_says_to_replace_it() {
    let (pane, data, given) = pane_with_a_file();
    pane.write("project/docs/given.bin", "mine");
    let mut replacing = upload_params(data.len(), "given.bin");
    replacing["replace"] = json!(true);
    let first = script(
        &[("upload", upload_params(data.len(), "given.bin"))],
        &dropping(&given),
    );
    let log = pane.record(&in_project(&pane, first));
    let answers = common::answers(&log);
    assert!(answers[0].failed("exists"), "{answers:?}");
    assert_eq!(answers[0].message(), "docs/given.bin already exists");
    assert_eq!(pane.read("project/docs/given.bin"), "mine");
    assert_eq!(count(&log, "program: read "), 0, "no byte was asked for");
    assert_eq!(left_in(&pane, "project/docs"), ["given.bin"]);

    let second = script(&[("upload", replacing)], &dropping(&given));
    let log = pane.record(&in_project(&pane, second));
    assert!(
        matches!(common::answers(&log)[0], Answer::Result(_)),
        "{log}"
    );
    assert_eq!(
        std::fs::read(pane.path("project/docs/given.bin")).unwrap(),
        data
    );
}

#[test]
fn the_name_the_person_gave_never_climbs_out_of_its_folder_and_the_folder_is_the_projects() {
    let (pane, data, given) = pane_with_a_file();
    let climbing = script(
        &[("upload", upload_params(data.len(), "../../escaped.bin"))],
        &dropping(&given),
    );
    let log = pane.record(&in_project(&pane, climbing));
    assert_eq!(
        common::answers(&log)[0].result()["path"],
        json!("docs/escaped.bin")
    );
    assert!(!pane.path("escaped.bin").exists() && !pane.path("project/escaped.bin").exists());

    let mut outside = upload_params(data.len(), "given.bin");
    outside["dir"] = json!("../outside");
    let there = script(&[("upload", outside)], &dropping(&given));
    let log = pane.record(&in_project(&pane, there));
    assert!(
        matches!(common::answers(&log)[0], Answer::Error { .. }),
        "{log}"
    );
    assert_eq!(
        left_in(&pane, "outside"),
        ["given.bin"],
        "nothing was added beside it"
    );
}

#[test]
fn a_file_the_page_gives_up_leaves_no_half_file() {
    let (pane, data, given) = pane_with_a_file();
    // The page cancels the call after its first word of progress.
    let mut host = dropping(&given);
    host.extend(["--cancel-after".to_owned(), "1".to_owned()]);
    let run = script(&[("upload", upload_params(data.len(), "given.bin"))], &host);
    let log = pane.record(&in_project(&pane, run));
    assert!(
        count(&log, "view: -> cancel") + count(&log, "cancel") >= 1,
        "{log}"
    );
    assert!(!pane.exists("project/docs/given.bin"));
    assert_eq!(
        left_in(&pane, "project/docs"),
        Vec::<String>::new(),
        "{log}"
    );
}

#[test]
fn a_ctrl_c_in_the_middle_of_a_file_leaves_no_half_file() {
    let (pane, data, given) = pane_with_a_file();
    // ^C is typed as the program asks for the second piece.
    let mut host = dropping(&given);
    host.extend(["--interrupt-at-read".to_owned(), "2".to_owned()]);
    let run = script(&[("upload", upload_params(data.len(), "given.bin"))], &host)
        .then(&["--then", "wait"]);
    let ran = pane.run(&in_project(&pane, run));
    assert!(
        ran.stderr.contains("the person presses ^C"),
        "{}",
        ran.stderr
    );
    assert!(!pane.exists("project/docs/given.bin"));
    assert_eq!(
        left_in(&pane, "project/docs"),
        Vec::<String>::new(),
        "{}",
        ran.stderr
    );
}

#[test]
fn place_says_whether_a_name_is_taken_and_the_next_free_one() {
    let pane = Pane::new();
    std::fs::create_dir_all(pane.path("project/docs")).unwrap();
    pane.write("project/docs/foto.png", "x");
    pane.write("project/docs/foto 2.png", "x");
    let run = script(
        &[
            ("place", json!({ "dir": "docs", "name": "foto.png" })),
            ("place", json!({ "dir": "docs", "name": "new.png" })),
            (
                "place",
                json!({ "dir": "docs", "name": "foto.png", "dash": true }),
            ),
            (
                "place",
                json!({ "dir": "docs", "name": "new.png", "also": ["new.png"] }),
            ),
            ("place", json!({ "dir": "nowhere", "name": "a" })),
        ],
        &[],
    );
    let answers = pane.ask(&in_project(&pane, run));
    assert_eq!(
        answers[0].result(),
        &json!({ "name": "foto.png", "exists": true, "free": "foto 3.png" })
    );
    assert_eq!(
        answers[1].result(),
        &json!({ "name": "new.png", "exists": false, "free": "new.png" })
    );
    assert_eq!(answers[2].result()["free"], json!("foto-1.png"));
    assert_eq!(
        answers[3].result(),
        &json!({ "name": "new.png", "exists": true, "free": "new 2.png" })
    );
    assert!(matches!(answers[4], Answer::Error { .. }));
}
