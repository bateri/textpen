//! What the program answers, read off the stand-in terminal's record of a pane: the calls of the
//! page with their params and the shapes of their results (`docs/calls.md`), the command line, the
//! drafts. Places in a text are code points, and each call that takes or gives one is tried with
//! characters of two, three and four bytes. See `common` for how the stand-in terminal is found.

// A test that cannot go on fails where it stands, helpers included.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::os::unix::fs::{PermissionsExt, symlink};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime};

use common::{Answer, Pane, Script, pdf_of_lines, pdf_of_pages, program, pushes, run};
use serde_json::{Value, json};

/// What the program puts in one message at most, bytes (docs/calls.md): a quarter of the 1 MiB a
/// message may have, what a page reads in the time it has for a call.
const PIECE: usize = 256 * 1024;

fn ask(pane: &Pane, calls: &[(&str, Value)]) -> Vec<Answer> {
    let mut script = Script::new();
    for (method, params) in calls {
        script = script.call(method, params.clone());
    }
    pane.ask(&script)
}

fn names(listing: &Value) -> Vec<String> {
    listing["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .map(|entry| entry["name"].as_str().expect("a name").to_owned())
        .collect()
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .expect("a list")
        .iter()
        .map(|item| item.as_str().expect("a string").to_owned())
        .collect()
}

#[test]
fn a_listing_has_the_folders_first_then_the_files_in_the_order_people_read_them() {
    let pane = Pane::new();
    for name in ["b.txt", "A.txt", "file10", "file9", "Zeta.txt", "é.txt"] {
        pane.write(name, "");
    }
    std::fs::create_dir(pane.path("zdir")).unwrap();
    std::fs::create_dir(pane.path("Adir")).unwrap();
    let heard = ask(&pane, &[("list", json!({ "dir": "" }))]);
    let list = heard[0].result();
    assert_eq!(
        names(list),
        [
            "Adir", "zdir", "A.txt", "b.txt", "file9", "file10", "Zeta.txt", "é.txt"
        ]
    );
    assert_eq!(list["truncated"], json!(false));
    assert_eq!(list["dir"], json!(""));
}

#[test]
fn the_listing_says_which_entries_are_links_and_a_link_to_nothing_is_a_file_of_no_size() {
    let pane = Pane::new();
    pane.write("real.txt", "12345");
    std::fs::create_dir(pane.path("realdir")).unwrap();
    symlink("real.txt", pane.path("to-file")).unwrap();
    symlink("realdir", pane.path("to-dir")).unwrap();
    symlink("nowhere", pane.path("to-nothing")).unwrap();
    let heard = ask(&pane, &[("list", json!({ "dir": "" }))]);
    let entries = heard[0].result()["entries"].as_array().unwrap().clone();
    let named = |name: &str| {
        entries
            .iter()
            .find(|entry| entry["name"] == json!(name))
            .unwrap_or_else(|| panic!("{name} is listed"))
            .clone()
    };
    assert_eq!(
        named("to-file"),
        json!({ "name": "to-file", "path": "to-file", "kind": "file", "link": true, "size": 5 })
    );
    // (A link to a directory has the size the system gives that directory.)
    let mut to_dir = named("to-dir");
    to_dir["size"] = json!(0);
    assert_eq!(
        to_dir,
        json!({ "name": "to-dir", "path": "to-dir", "kind": "dir", "link": true, "size": 0 })
    );
    assert_eq!(
        named("to-nothing"),
        json!({ "name": "to-nothing", "path": "to-nothing", "kind": "file", "link": true, "size": 0 })
    );
    assert_eq!(
        named("real.txt"),
        json!({ "name": "real.txt", "path": "real.txt", "kind": "file", "size": 5 })
    );
    assert_eq!(
        named("realdir"),
        json!({ "name": "realdir", "path": "realdir", "kind": "dir", "size": 0 })
    );
}

#[test]
fn a_file_is_created_only_where_nothing_is_nothing_is_truncated_and_no_link_is_written_through() {
    let pane = Pane::new();
    pane.write("kept.txt", "keep me");
    symlink(pane.path("target-of-the-link"), pane.path("dangling")).unwrap();
    let heard = ask(
        &pane,
        &[
            ("create", json!({ "path": "kept.txt", "kind": "file" })),
            ("create", json!({ "path": "dangling", "kind": "file" })),
            ("create", json!({ "path": "new.txt", "kind": "file" })),
            (
                "create",
                json!({ "path": "deep/er/new.md", "kind": "file" }),
            ),
            ("create", json!({ "path": "deep", "kind": "dir" })),
            ("create", json!({ "path": "fresh/dir", "kind": "dir" })),
        ],
    );
    assert!(heard[0].failed("failed"));
    assert!(heard[0].message().contains("already exists"), "{heard:?}");
    assert!(heard[1].failed("failed"));
    assert_eq!(heard[2].result(), &json!({ "path": "new.txt" }));
    assert_eq!(heard[3].result(), &json!({ "path": "deep/er/new.md" }));
    assert!(heard[4].failed("failed"));
    assert_eq!(heard[5].result(), &json!({ "path": "fresh/dir" }));
    assert_eq!(pane.read("kept.txt"), "keep me");
    assert!(
        !pane.exists("target-of-the-link"),
        "a link is not written through"
    );
    assert_eq!(pane.read("new.txt"), "");
    assert_eq!(pane.read("deep/er/new.md"), "");
    assert!(pane.path("fresh/dir").is_dir());
}

#[test]
fn rename_moves_a_file_and_its_buffer_refuses_a_name_that_is_taken_and_delete_spares_the_project() {
    let pane = Pane::new();
    pane.write("a.txt", "one");
    pane.write("b.txt", "two");
    pane.write("Öz.txt", "case");
    let heard = ask(
        &pane,
        &[
            ("open", json!({ "path": "a.txt" })),
            ("rename", json!({ "from": "a.txt", "to": "sub/c.txt" })),
            ("rename", json!({ "from": "b.txt", "to": "sub/c.txt" })),
            ("create", json!({ "path": "gone.txt", "kind": "file" })),
            ("delete", json!({ "path": "gone.txt" })),
            ("delete", json!({ "path": "." })),
            ("delete", json!({ "path": ".." })),
            ("open", json!({ "path": "Öz.txt" })),
            ("rename", json!({ "from": "Öz.txt", "to": "öz.txt" })),
        ],
    );
    assert_eq!(heard[1].result()["path"], json!("sub/c.txt"));
    assert_eq!(heard[1].result()["buffers"][0]["path"], json!("sub/c.txt"));
    assert!(heard[2].failed("failed"));
    assert_eq!(heard[4].result(), &json!({ "path": "gone.txt" }));
    assert!(heard[5].failed("failed"), "the project is not removed");
    assert!(heard[6].failed("failed"), "nor the directory it is in");
    // A rename by case alone is allowed for any letter, and the buffer follows.
    let by_case = heard[8].result();
    assert_eq!(by_case["path"], json!("öz.txt"));
    assert_eq!(by_case["buffers"][1]["path"], json!("öz.txt"));
    assert!(!pane.exists("a.txt"));
    assert_eq!(pane.read("sub/c.txt"), "one");
    assert_eq!(pane.read("b.txt"), "two");
    assert_eq!(pane.read("öz.txt"), "case");
    assert!(pane.dir.path().is_dir());
}

#[test]
fn params_that_are_not_what_a_call_asks_are_refused_with_bad_params_and_nothing_runs() {
    let pane = Pane::new();
    pane.write("a.txt", "x");
    let place = json!({ "anchor": 0, "head": 0, "top": 0 });
    let ui = |key: &str, value: Option<Value>| {
        let mut ui = json!({
            "tabs": [], "active": null, "expanded": [], "places": {}, "recent": [], "preview": [],
            "changesTree": false
        });
        match value {
            Some(value) => ui[key] = value,
            None => {
                ui.as_object_mut().unwrap().remove(key);
            }
        }
        json!({ "ui": ui })
    };
    let settings = |key: &str, value: Option<Value>| {
        let mut settings = json!({
            "wrap": false, "lineNumbers": "on", "tabSize": 2, "insertSpaces": true,
            "whitespace": "selection", "indentGuides": true, "autoSave": false, "sidebar": true,
            "sidebarWidth": 240, "diffDecorations": true, "hideIgnored": false,
        });
        match value {
            Some(value) => settings[key] = value,
            None => {
                settings.as_object_mut().unwrap().remove(key);
            }
        }
        json!({ "settings": settings })
    };
    let refused = [
        ("open", json!({})),
        ("open", json!({ "path": "" })),
        ("open", json!({ "path": 5 })),
        ("open", json!({ "path": null })),
        ("save", json!({ "path": "a.txt" })),
        (
            "save",
            json!({ "path": "a.txt", "version": 1, "force": null }),
        ),
        (
            "edit",
            json!({ "path": "a.txt", "base": 1, "edits": [[0, 0]] }),
        ),
        ("edit", json!({ "path": "a.txt", "base": 1.5, "edits": [] })),
        ("edit", json!({ "path": null, "base": 1, "edits": [] })),
        (
            "slice",
            json!({ "path": "a.txt", "version": 1, "from": -1 }),
        ),
        ("files", json!({ "from": null })),
        ("files", json!({ "from": -1 })),
        ("search", json!({ "query": "" })),
        ("search", json!({ "query": "x", "regex": null })),
        ("create", json!({ "path": "x", "kind": "link" })),
        ("buffer", json!({ "path": "a.txt", "action": "explode" })),
        (
            "buffer",
            json!({ "path": "a.txt", "action": "close", "discard": null }),
        ),
        ("handoff", json!({ "kind": "quit" })),
        ("ui", ui("places", None)),
        ("ui", ui("places", Some(json!([])))),
        ("ui", ui("places", Some(json!([place])))),
        ("ui", ui("places", Some(json!({ "a.txt": 5 })))),
        (
            "ui",
            ui(
                "places",
                Some(json!({ "a.txt": { "anchor": 0, "head": 0 } })),
            ),
        ),
        ("ui", ui("tabs", Some(json!([1])))),
        ("ui", ui("active", None)),
        ("ui", ui("recent", Some(Value::Null))),
        ("settings", json!({ "settings": { "vim": "yes" } })),
        ("settings", settings("indentGuides", None)),
        ("settings", settings("whitespace", Some(json!(true)))),
        ("settings", settings("tabSize", Some(json!("2")))),
        ("picture", json!({ "path": "a.png" })),
        ("picture", json!({ "path": "", "owner": "a.png" })),
        (
            "picture",
            json!({ "path": "a.png", "owner": "a.png", "from": null }),
        ),
        ("release", json!({})),
        ("release", json!({ "owner": "" })),
    ];
    let mut calls: Vec<(&str, Value)> = refused.to_vec();
    calls.push(("nothing-by-this-name", json!({})));
    let heard = ask(&pane, &calls);
    for (index, answer) in heard.iter().enumerate() {
        if index == heard.len() - 1 {
            assert!(answer.failed("not-found"), "call {index}: {answer:?}");
        } else {
            assert!(
                answer.failed("bad-params"),
                "call {index} ({:?}): {answer:?}",
                calls[index]
            );
        }
    }
    // The words say where.
    assert_eq!(
        heard[23].message(),
        "ui.tabs.0: expected a string, got number"
    );
    assert_eq!(
        heard[24].message(),
        "ui.active: expected a string or null, got nothing"
    );
    assert_eq!(heard[10].message(), "from: expected an integer, got null");
    assert_eq!(pane.read("a.txt"), "x");
}

#[test]
fn every_key_of_the_pages_places_is_a_key_and_comes_back() {
    let pane = Pane::new();
    let place = json!({ "anchor": 1, "head": 2, "top": 3 });
    let ui = json!({
        "tabs": ["src/a.ts"],
        "active": "src/a.ts",
        "expanded": [],
        "places": { "__proto__": place, "constructor": place, "__index": place, "src/a.ts": place },
        "recent": [],
        "preview": [],
        "changesTree": true,
    });
    let mut emptied = ui.clone();
    emptied["active"] = Value::Null;
    emptied["places"] = json!({});
    let heard = ask(
        &pane,
        &[
            ("ui", json!({ "ui": ui })),
            ("boot", json!({})),
            ("ui", json!({ "ui": emptied })),
            ("boot", json!({})),
        ],
    );
    assert_eq!(heard[0].result(), &json!({}));
    let places = heard[1].result()["ui"]["places"]
        .as_object()
        .unwrap()
        .clone();
    let mut keys: Vec<&String> = places.keys().collect();
    keys.sort();
    assert_eq!(keys, ["__index", "__proto__", "constructor", "src/a.ts"]);
    assert_eq!(places["src/a.ts"], place);
    assert_eq!(heard[3].result()["ui"]["active"], Value::Null);
    assert_eq!(heard[3].result()["ui"]["places"], json!({}));
}

#[test]
fn boot_tells_what_the_page_starts_from_with_null_where_there_is_nothing() {
    let pane = Pane::new();
    let heard = ask(&pane, &[("boot", json!({}))]);
    let boot = heard[0].result();
    assert_eq!(boot["root"], json!(pane.root()));
    assert_eq!(
        boot["rootName"],
        json!(pane.dir.path().file_name().unwrap().to_str().unwrap())
    );
    assert_eq!(boot["roots"], json!([]));
    assert_eq!(boot["repos"], json!([]));
    assert_eq!(boot["buffers"], json!([]));
    assert_eq!(boot["problems"], json!([]));
    assert_eq!(boot["ui"]["active"], Value::Null);
    assert_eq!(boot["ui"]["places"], json!({}));
    assert_eq!(
        boot["settings"],
        json!({
            "wrap": false,
            "lineNumbers": "on",
            "tabSize": 2,
            "insertSpaces": true,
            "whitespace": "selection",
            "indentGuides": true,
            "autoSave": false,
            "sidebar": true,
            "sidebarWidth": 240,
            "diffDecorations": true,
            "hideIgnored": false,
        })
    );
    assert_eq!(boot["handoff"], json!(true));
    assert_eq!(boot["closeQuits"], json!(false));
    assert_eq!(boot["remote"], json!(false));
    assert_eq!(boot["home"], json!(pane.dir.path().to_str().unwrap()));
    assert!(boot["hostname"].is_string() && boot["user"].is_string());
    assert!(boot["version"].is_string());
    assert!(boot.get("focus").is_none() && boot.get("returned").is_none());
}

#[test]
fn the_command_line_names_the_files_to_open_and_where_the_cursor_goes() {
    let pane = Pane::new();
    pane.write("a.txt", "one\ntwo\n");
    pane.write("x:5", "a file that has a place's name\n");
    let boot = |args: &[&str]| {
        pane.ask(&Script::new().call("boot", json!({})).args(args))[0]
            .result()
            .clone()
    };
    let at = boot(&["a.txt:2:3"]);
    assert_eq!(
        at["focus"],
        json!({ "path": "a.txt", "line": 2, "column": 3 })
    );
    assert_eq!(at["ui"]["tabs"], json!(["a.txt"]));
    assert_eq!(at["ui"]["active"], json!("a.txt"));
    assert_eq!(at["closeQuits"], json!(true));
    assert_eq!(
        boot(&["+7", "a.txt"])["focus"],
        json!({ "path": "a.txt", "line": 7 })
    );
    let named = boot(&["x:5"]);
    assert!(
        named.get("focus").is_none(),
        "a file with that very name is that file"
    );
    assert_eq!(named["ui"]["tabs"], json!(["x:5"]));
    let project = boot(&["."]);
    assert_eq!(project["closeQuits"], json!(false));
    assert_eq!(project["ui"]["tabs"], json!([]));
}

#[test]
fn version_and_help_are_answered_without_a_terminal() {
    let pane = Pane::new();
    let ran = |args: &[&str]| {
        let mut command = pane.command(program());
        command.args(args);
        run(command, None, Duration::from_secs(15))
    };
    let version = ran(&["--version"]);
    assert_eq!(version.status, Some(0));
    assert_eq!(version.stdout, format!("{}\n", env!("CARGO_PKG_VERSION")));
    assert_eq!(ran(&["-v"]).stdout, version.stdout);
    assert!(ran(&["-h"]).stdout.starts_with("usage: textpen"));
    let unknown = ran(&["--nonsense"]);
    assert_eq!(unknown.status, Some(2));
    assert!(unknown.stderr.contains("unknown option --nonsense"));
}

#[test]
fn started_with_no_terminal_at_all_it_says_it_needs_one() {
    let pane = Pane::new();
    pane.write("a.txt", "x\n");
    let mut command = pane.command(program());
    command.arg("a.txt");
    let ran = run(command, Some(""), Duration::from_secs(15));
    assert_eq!(ran.status, Some(1));
    assert!(
        ran.stderr.contains("textpen: needs a terminal to edit in"),
        "{}",
        ran.stderr
    );
}

#[test]
fn settings_are_kept_within_their_bounds_where_the_xdg_directories_say() {
    let pane = Pane::new();
    let settings = json!({
        "wrap": true,
        "lineNumbers": "relative",
        "tabSize": 0,
        "insertSpaces": false,
        "whitespace": "all",
        "indentGuides": false,
        "autoSave": true,
        "sidebar": false,
        "sidebarWidth": 10,
        "diffDecorations": false,
        "hideIgnored": true,
        "vim": true,
    });
    let heard = ask(&pane, &[("settings", json!({ "settings": settings }))]);
    let saved = &heard[0].result()["settings"];
    assert_eq!(saved["tabSize"], json!(1));
    assert_eq!(saved["sidebarWidth"], json!(228));
    assert_eq!(saved["diffDecorations"], json!(false));
    assert_eq!(saved["hideIgnored"], json!(true));
    assert!(
        saved.get("vim").is_none(),
        "what the program does not know is not kept"
    );
    let file: Value =
        serde_json::from_str(&pane.read("config/textpen/settings.json")).expect("JSON");
    assert_eq!(file["lineNumbers"], json!("relative"));
    assert_eq!(file["tabSize"], json!(1));
    let heard = ask(&pane, &[("boot", json!({}))]);
    let booted = &heard[0].result()["settings"];
    assert_eq!(booted["whitespace"], json!("all"));
    assert_eq!(booted["indentGuides"], json!(false));
    assert_eq!(booted["wrap"], json!(true));
}

#[test]
fn the_settings_an_older_textpen_kept_are_read_as_this_one_has_them() {
    let pane = Pane::new();
    pane.write(
        "config/textpen/settings.json",
        r#"{ "vim": true, "fontSize": 20, "whitespace": true, "tabSize": 4, "sidebarWidth": 999 }"#,
    );
    let heard = ask(&pane, &[("boot", json!({}))]);
    let settings = &heard[0].result()["settings"];
    assert_eq!(settings["whitespace"], json!("all"));
    assert_eq!(settings["indentGuides"], json!(true));
    assert_eq!(settings["tabSize"], json!(4));
    assert_eq!(settings["sidebarWidth"], json!(520));
    assert!(settings.get("vim").is_none() && settings.get("fontSize").is_none());
    pane.write("config/textpen/settings.json", r#"{ "whitespace": false }"#);
    let heard = ask(&pane, &[("boot", json!({}))]);
    assert_eq!(heard[0].result()["settings"]["whitespace"], json!("none"));
}

#[test]
fn edits_count_bytes_and_a_file_is_written_as_it_was_read() {
    let pane = Pane::new();
    // "a", then ş (two bytes), then an emoji (four bytes), then "z": places are bytes, so ş is at
    // 1..3, the emoji at 3..7 and z at 7..8.
    pane.write("u.txt", "\u{feff}aş😀z\r\nlast\r\n");
    let heard = ask(
        &pane,
        &[
            ("open", json!({ "path": "u.txt" })),
            (
                "edit",
                json!({ "path": "u.txt", "base": 1, "edits": [[7, 8, "Z"], [3, 7, "<>"]] }),
            ),
            (
                "edit",
                json!({ "path": "u.txt", "base": 2, "edits": [[0, 99, "x"]] }),
            ),
            ("save", json!({ "path": "u.txt", "version": 2 })),
            (
                "edit",
                json!({ "path": "u.txt", "base": 1, "edits": [[3, 3, "!"]] }),
            ),
            ("buffer", json!({ "path": "u.txt", "action": "text" })),
            (
                "edit",
                json!({ "path": "u.txt", "base": 2, "edits": [[1, 1, "€"], [11, 11, "𝄞"]] }),
            ),
            ("buffer", json!({ "path": "u.txt", "action": "text" })),
        ],
    );
    let opened = heard[0].result();
    assert_eq!(opened["text"], json!("aş😀z\nlast\n"));
    assert_eq!(opened["eol"], json!("crlf"));
    assert_eq!(opened["bom"], json!(true));
    assert_eq!(heard[1].result()["version"], json!(2));
    assert_eq!(heard[1].result()["dirty"], json!(true));
    assert!(
        heard[2].failed("out-of-sync"),
        "an edit that reaches outside the text"
    );
    assert_eq!(heard[3].result()["saved"], json!(true));
    assert!(
        heard[4].failed("out-of-sync"),
        "an edit made on a version that is gone"
    );
    assert_eq!(heard[5].result()["text"], json!("aş<>Z\nlast\n"));
    // "a€ş<>Z\n" is ten bytes: the second edit, on what the first left, is after "l" (byte 11).
    assert_eq!(heard[7].result()["text"], json!("a€ş<>Z\nl𝄞ast\n"));
    assert_eq!(pane.read("u.txt"), "\u{feff}aş<>Z\r\nlast\r\n");
}

#[test]
fn a_file_that_cannot_be_text_is_not_opened_as_one() {
    let pane = Pane::new();
    pane.write("b.bin", [1u8, 2, 0, 3]);
    pane.write("latin1.txt", [0x63u8, 0x61, 0x66, 0xe9]);
    let heard = ask(
        &pane,
        &[
            ("open", json!({ "path": "b.bin" })),
            ("open", json!({ "path": "latin1.txt" })),
            ("open", json!({ "path": "." })),
            ("open", json!({ "path": "not/there/yet.md" })),
        ],
    );
    assert_eq!(heard[0].result()["reason"], json!("binary"));
    assert_eq!(heard[0].result()["path"], json!("b.bin"));
    assert_eq!(heard[1].result()["reason"], json!("binary"));
    assert_eq!(heard[2].result()["reason"], json!("directory"));
    assert_eq!(heard[3].result()["missing"], json!(true));
    assert_eq!(heard[3].result()["text"], json!(""));
    assert_eq!(heard[3].result()["mtime"], json!(0));
}

#[test]
fn an_edit_back_to_the_saved_text_is_not_a_change_and_a_new_file_comes_to_be_when_saved() {
    let pane = Pane::new();
    pane.write("a.txt", "abc");
    let heard = ask(
        &pane,
        &[
            ("open", json!({ "path": "a.txt" })),
            (
                "edit",
                json!({ "path": "a.txt", "base": 1, "edits": [[3, 3, "d"]] }),
            ),
            (
                "edit",
                json!({ "path": "a.txt", "base": 2, "edits": [[3, 4, ""]] }),
            ),
            ("open", json!({ "path": "deep/er/new.md" })),
            (
                "edit",
                json!({ "path": "deep/er/new.md", "base": 1, "edits": [[0, 0, "# New\n"]] }),
            ),
            ("save", json!({ "path": "deep/er/new.md", "version": 2 })),
        ],
    );
    assert_eq!(heard[1].result()["dirty"], json!(true));
    assert_eq!(heard[2].result()["dirty"], json!(false));
    assert_eq!(heard[5].result()["saved"], json!(true));
    assert_eq!(heard[5].result()["info"]["missing"], json!(false));
    assert_eq!(pane.read("deep/er/new.md"), "# New\n");
}

#[test]
fn a_file_another_program_wrote_since_it_was_read_is_not_written_over_unless_the_person_says_so() {
    // Two buffers of one file (one by a link's name) stand for two writers: the first save changes
    // the file under the other buffer, which has changes of its own.
    let pane = Pane::new();
    pane.write("c.txt", "first");
    let past = SystemTime::now() - Duration::from_secs(3600);
    std::fs::File::options()
        .write(true)
        .open(pane.path("c.txt"))
        .unwrap()
        .set_modified(past)
        .unwrap();
    symlink("c.txt", pane.path("l.txt")).unwrap();
    let heard = ask(
        &pane,
        &[
            ("open", json!({ "path": "c.txt" })),
            ("open", json!({ "path": "l.txt" })),
            (
                "edit",
                json!({ "path": "l.txt", "base": 1, "edits": [[0, 0, "mine "]] }),
            ),
            (
                "edit",
                json!({ "path": "c.txt", "base": 1, "edits": [[0, 0, "theirs "]] }),
            ),
            ("save", json!({ "path": "c.txt", "version": 2 })),
            ("save", json!({ "path": "l.txt", "version": 2 })),
            ("buffer", json!({ "path": "l.txt", "action": "keep" })),
            (
                "save",
                json!({ "path": "l.txt", "version": 2, "force": true }),
            ),
        ],
    );
    assert_eq!(heard[4].result()["saved"], json!(true));
    assert_eq!(heard[5].result()["saved"], json!(false));
    assert_eq!(heard[5].result()["info"]["conflict"], json!(true));
    assert_eq!(heard[6].result()["conflict"], json!(false));
    assert_eq!(heard[7].result()["saved"], json!(true));
    assert_eq!(pane.read("c.txt"), "mine first");
    assert!(pane.path("l.txt").symlink_metadata().unwrap().is_symlink());
}

#[test]
fn a_folder_of_thousands_is_cut_to_what_a_message_carries_and_the_files_of_a_big_project_come_a_messages_worth_at_a_time()
 {
    let pane = Pane::new();
    for i in 0..5100 {
        pane.write(&format!("many/f{i:04}"), "");
    }
    // 3,600 files whose paths are 300 characters: more than one message holds.
    let folder = format!("d{}", "x".repeat(98));
    let mut long = Vec::new();
    for i in 0..3600 {
        let name = format!("{folder}/{i:04}{}", "y".repeat(196));
        pane.write(&name, "");
        long.push(name);
    }
    let heard = ask(
        &pane,
        &[("list", json!({ "dir": "many" })), ("files", json!({}))],
    );
    let listing = heard[0].result();
    let entries = listing["entries"].as_array().unwrap().len();
    assert!(entries > 1000 && entries < 5100, "{entries} entries");
    assert_eq!(listing["truncated"], json!(true));
    assert_eq!(listing["entries"][0]["name"], json!("f0000"));
    assert!(
        listing.to_string().len() <= PIECE + 1024,
        "a listing fits a piece"
    );
    // The index, page after page, each a piece, until there is no next. A listing's pages are of
    // one run of the program (they name the generation of its first): each run here reads the pages
    // known so far and one more.
    let mut all = Vec::new();
    let mut page = heard[1].result().clone();
    let generation = page["generation"].clone();
    let mut froms: Vec<u64> = Vec::new();
    loop {
        assert!(
            page.to_string().len() <= PIECE + 1024,
            "a page fits a piece"
        );
        assert_eq!(page["truncated"], json!(false));
        assert_eq!(page["generation"], generation);
        all.extend(strings(&page["files"]));
        let Some(next) = page["next"].as_u64() else {
            break;
        };
        assert!(next > 0 && next < 3600 + 5100);
        froms.push(next);
        let mut calls = vec![("files", json!({}))];
        for from in &froms {
            calls.push(("files", json!({ "from": from, "generation": generation })));
        }
        page = ask(&pane, &calls).last().unwrap().result().clone();
    }
    assert!(froms.len() > 1, "the files of a big project come in pages");
    let unique: std::collections::HashSet<&String> = all.iter().collect();
    assert_eq!(unique.len(), 3600 + 5100, "every file, once");
    assert_eq!(all.len(), 3600 + 5100);
    assert!(long.iter().all(|name| unique.contains(name)));
}

#[test]
fn search_finds_words_whole_words_patterns_counts_code_points_and_refuses_a_pattern_that_is_not_one()
 {
    let pane = Pane::new();
    pane.write("one.ts", "const order = 1;\nconst orders = [order];\n");
    pane.write("two.md", "No Order here\n");
    pane.write(
        "wide.txt",
        format!("{}😀 order {} order\n", "ş".repeat(5), "x".repeat(300)),
    );
    let q = |query: &str, options: Value| {
        let mut params = options;
        params["query"] = json!(query);
        ("search", params)
    };
    let heard = ask(
        &pane,
        &[
            q("order", json!({})),
            q("order", json!({ "caseSensitive": true, "wholeWord": true })),
            q(
                "orders?\\b",
                json!({ "regex": true, "caseSensitive": true }),
            ),
            q("(", json!({ "regex": true })),
            q("order", json!({ "caseSensitive": true })),
            q("nothing-like-this", json!({})),
            q("(a)\\1", json!({ "regex": true })),
            q("😀 ord", json!({})),
        ],
    );
    assert_eq!(heard[0].result()["matches"], json!(6));
    let whole = heard[1].result();
    assert_eq!(whole["matches"], json!(4));
    let first: Vec<(u64, String)> = whole["files"][0]["matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            let text: String = m["text"]
                .as_str()
                .unwrap()
                .chars()
                .skip(m["start"].as_u64().unwrap() as usize)
                .take((m["end"].as_u64().unwrap() - m["start"].as_u64().unwrap()) as usize)
                .collect();
            (m["line"].as_u64().unwrap(), text)
        })
        .collect();
    assert_eq!(first, [(1, "order".to_owned()), (2, "order".to_owned())]);
    assert_eq!(heard[2].result()["matches"], json!(5));
    assert!(heard[3].failed("failed"));
    assert!(heard[3].message().contains("Not a regular expression"));
    let wide = heard[4].result()["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|file| file["path"] == json!("wide.txt"))
        .unwrap()["matches"]
        .clone();
    // The first match is after five ş, an emoji and a space: code point 7, and the start of the
    // line is shown.
    assert_eq!(
        [&wide[0]["start"], &wide[0]["end"], &wide[0]["offset"]],
        [&json!(7), &json!(12), &json!(0)]
    );
    // A long line is cut around the match: the second is far along it.
    let second = &wide[1];
    assert_eq!(second["offset"], json!(314 - 40));
    let shown: Vec<char> = second["text"].as_str().unwrap().chars().collect();
    let (start, end) = (
        second["start"].as_u64().unwrap() as usize,
        second["end"].as_u64().unwrap() as usize,
    );
    assert_eq!(shown[start..end].iter().collect::<String>(), "order");
    assert!(shown.len() <= 160);
    assert_eq!(heard[5].result()["matches"], json!(0));
    assert_eq!(heard[5].result()["truncated"], json!(false));
    assert!(heard[6].failed("failed"));
    assert!(
        heard[6].message().contains("back-reference"),
        "{}",
        heard[6].message()
    );
    let astral = &heard[7].result()["files"][0]["matches"][0];
    assert_eq!([&astral["start"], &astral["end"]], [&json!(5), &json!(10)]);
}

#[test]
fn the_files_of_a_project_and_the_results_of_a_search_come_in_the_order_a_listing_has() {
    // (Numbers are numbers: file9 comes before file10, in the tree and in a search alike.)
    let pane = Pane::new();
    for name in ["file10.txt", "file9.txt", "B.txt", "a.txt"] {
        pane.write(name, "needle\n");
    }
    let heard = ask(&pane, &[("search", json!({ "query": "needle" }))]);
    let paths: Vec<&Value> = heard[0].result()["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| &file["path"])
        .collect();
    assert_eq!(
        paths,
        [
            &json!("a.txt"),
            &json!("B.txt"),
            &json!("file9.txt"),
            &json!("file10.txt")
        ]
    );
}

fn git(pane: &Pane, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(pane.dir.path())
        .env("HOME", pane.dir.path())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .status()
        .expect("git is installed: the tests of the repository need it");
    assert!(status.success(), "git {args:?}");
}

#[test]
fn a_git_repository_is_read_for_the_quick_open_and_for_the_changes() {
    let pane = Pane::new();
    git(&pane, &["init", "-q", "-b", "trunk"]);
    git(&pane, &["config", "user.email", "t@example.com"]);
    git(&pane, &["config", "user.name", "t"]);
    pane.write("kept.txt", "1");
    pane.write(".gitignore", "ignored.txt\n");
    git(&pane, &["add", "."]);
    git(&pane, &["commit", "-q", "-m", "one"]);
    pane.write("kept.txt", "2");
    pane.write("new file.txt", "");
    pane.write("ignored.txt", "");
    pane.write("ünï/çödé.txt", "");
    let heard = ask(&pane, &[("boot", json!({})), ("files", json!({}))]);
    let boot = heard[0].result();
    assert_eq!(boot["roots"], json!([""]));
    assert_eq!(boot["repos"][0]["root"], json!(""));
    let git_info = &boot["repos"][0]["git"];
    assert_eq!(git_info["branch"], json!("trunk"));
    assert_eq!(git_info["ahead"], json!(0));
    assert_eq!(
        git_info["changes"],
        json!({ "kept.txt": "modified", "new file.txt": "untracked", "ünï/çödé.txt": "untracked" })
    );
    assert_eq!(git_info["ignored"], json!(["ignored.txt"]));
    let mut files = strings(&heard[1].result()["files"]);
    files.sort();
    assert_eq!(
        files,
        [".gitignore", "kept.txt", "new file.txt", "ünï/çödé.txt"]
    );
    assert_eq!(heard[1].result()["next"], Value::Null);
}

/// A project in no repository, with repositories in its folders: each is said by paths from the project's
/// root; a file of one is compared and marked by its own git, what it ignores is not among the files, and a
/// file in none has no git to ask.
#[test]
fn repositories_in_the_folders_of_a_project_are_each_asked_of_what_is_theirs() {
    let pane = Pane::new();
    let in_repo = |dir: &str, args: &[&str]| {
        let mut all = vec!["-C", dir];
        all.extend_from_slice(args);
        git(&pane, &all);
    };
    for dir in ["alpha", "group/beta"] {
        pane.write(&format!("{dir}/kept.txt"), "one\ntwo\n");
        pane.write(&format!("{dir}/.gitignore"), "out.log\n");
        in_repo(dir, &["init", "-q", "-b", dir.rsplit('/').next().unwrap()]);
        in_repo(dir, &["config", "user.email", "t@example.com"]);
        in_repo(dir, &["config", "user.name", "t"]);
        in_repo(dir, &["add", "."]);
        in_repo(dir, &["commit", "-q", "-m", "one"]);
    }
    pane.write("loose.txt", "x\n");
    pane.write("alpha/kept.txt", "one\ntwo\nthree\n");
    pane.write("alpha/new.txt", "a\nb\n");
    pane.write("alpha/out.log", "");
    let heard = ask(
        &pane,
        &[
            ("boot", json!({})),
            ("files", json!({})),
            ("open", json!({ "path": "alpha/kept.txt" })),
            ("diff", json!({ "path": "alpha/kept.txt" })),
            ("compare", json!({ "path": "alpha/kept.txt" })),
            ("open", json!({ "path": "loose.txt" })),
            ("diff", json!({ "path": "loose.txt" })),
            ("compare", json!({ "path": "loose.txt" })),
        ],
    );
    let boot = heard[0].result();
    assert_eq!(boot["roots"], json!(["alpha", "group/beta"]));
    let alpha = &boot["repos"][0];
    assert_eq!(alpha["root"], json!("alpha"));
    assert_eq!(alpha["git"]["branch"], json!("alpha"));
    assert_eq!(
        alpha["git"]["changes"],
        json!({ "alpha/kept.txt": "modified", "alpha/new.txt": "untracked" })
    );
    assert_eq!(
        alpha["git"]["stats"],
        json!({ "alpha/kept.txt": [1, 0], "alpha/new.txt": [2, 0] })
    );
    assert_eq!(alpha["git"]["ignored"], json!(["alpha/out.log"]));
    let beta = &boot["repos"][1];
    assert_eq!(beta["root"], json!("group/beta"));
    assert_eq!(beta["git"]["branch"], json!("beta"));
    assert_eq!(beta["git"]["changes"], json!({}));
    let mut files = strings(&heard[1].result()["files"]);
    files.sort();
    assert_eq!(
        files,
        [
            "alpha/.gitignore",
            "alpha/kept.txt",
            "alpha/new.txt",
            "group/beta/.gitignore",
            "group/beta/kept.txt",
            "loose.txt",
        ]
    );
    assert_eq!(heard[3].result()["hunks"][0]["count"], json!(1));
    let compared = heard[4].result();
    assert_eq!(compared["base"]["path"], json!("alpha/kept.txt"));
    assert_eq!(compared["why"], Value::Null);
    assert_eq!(heard[6].result()["hunks"], Value::Null);
    assert_eq!(heard[7].result()["why"], json!("outside"));
}

#[test]
fn a_git_that_does_not_answer_is_given_up_on_after_15_seconds_and_the_files_are_walked_instead() {
    let pane = Pane::new();
    pane.write("bin/git", "#!/bin/sh\nexec sleep 40\n");
    std::fs::set_permissions(pane.path("bin/git"), std::fs::Permissions::from_mode(0o755)).unwrap();
    pane.write("sub/x.txt", "");
    pane.write("node_modules/skipped.js", "");
    let path = format!(
        "{}:{}",
        pane.path("bin").display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let started = Instant::now();
    let heard = pane.ask(
        &Script::new()
            .call("files", json!({}))
            .limit(45)
            .env("PATH", &path),
    );
    let took = started.elapsed();
    let mut files = strings(&heard[0].result()["files"]);
    files.sort();
    assert_eq!(files, ["bin/git", "sub/x.txt"]);
    assert!(
        took >= Duration::from_secs(14) && took < Duration::from_secs(30),
        "it waited {took:?} for git"
    );
}

#[test]
fn the_changes_topic_tells_the_disk_at_once_then_what_changed_under_a_listed_directory_or_in_an_open_file()
 {
    let pane = Pane::new();
    pane.write("c.txt", "first");
    // A moment into the run, another program writes: the open file changes, and a file appears in
    // the directory the page listed.
    let mut writer = Command::new("sh")
        .args(["-c", "sleep 1.2; echo second > c.txt; echo new > new.txt"])
        .current_dir(pane.dir.path())
        .spawn()
        .unwrap();
    let log = pane.record(
        &Script::new()
            .call("list", json!({ "dir": "" }))
            .call("open", json!({ "path": "c.txt" }))
            .host_args(&["--subscribe", "changes"])
            .then(&["--then", "wait"])
            .limit(6),
    );
    let _ = writer.wait();
    let pushes = pushes(&log);
    assert!(pushes.len() >= 2, "two pushes:\n{log}");
    assert_eq!(pushes[0], json!({ "buffers": [], "dirs": [], "roots": [] }));
    assert!(
        pushes[1].get("roots").is_none() && pushes[1].get("repo").is_none(),
        "git said what it said before"
    );
    assert_eq!(pushes[1]["buffers"][0]["path"], json!("c.txt"));
    assert_eq!(
        pushes[1]["buffers"][0]["version"],
        json!(2),
        "a buffer without changes is read again"
    );
    assert_eq!(pushes[1]["dirs"], json!([""]));
    // The directory is told again until the page lists it again; the file, once.
    for later in &pushes[2..] {
        assert_eq!(later["buffers"], json!([]));
        assert_eq!(later["dirs"], json!([""]));
    }
}

#[test]
fn quitting_by_hangup_keeps_what_is_not_saved_as_a_draft_with_its_length_in_bytes() {
    let pane = Pane::new();
    pane.write("a.txt", "world\n");
    pane.record(
        &Script::new()
            .call("open", json!({ "path": "a.txt" }))
            .call(
                "edit",
                json!({ "path": "a.txt", "base": 1, "edits": [[5, 5, "😀!"]] }),
            )
            .then(&["--then", "hangup"])
            .args(&["a.txt"]),
    );
    assert_eq!(pane.read("a.txt"), "world\n");
    let drafts = pane.drafts();
    assert_eq!(drafts.len(), 1);
    let draft: Value = serde_json::from_slice(&std::fs::read(&drafts[0]).unwrap()).unwrap();
    assert_eq!(draft["text"], json!("world😀!\n"));
    assert_eq!(draft["path"], json!(format!("{}/a.txt", pane.root())));
    let heard = pane.ask(
        &Script::new()
            .call("open", json!({ "path": "a.txt" }))
            .call("buffer", json!({ "path": "a.txt", "action": "drop" }))
            .args(&["a.txt"]),
    );
    // "world😀!\n" is eleven bytes: the emoji is four.
    assert_eq!(heard[0].result()["draft"]["length"], json!(11));
    assert!(heard[1].result().get("draft").is_none());
    assert_eq!(heard[1].result()["text"], json!("world\n"));
    assert!(pane.drafts().is_empty(), "a dropped draft is gone");
}

#[test]
fn a_long_text_of_four_byte_characters_is_read_in_pieces_that_count_bytes() {
    // 300,000 emoji are 1.2 MB: more than a piece holds, and each piece ends between two of them.
    let pane = Pane::new();
    pane.write("e.txt", "😀".repeat(300_000));
    let budget = PIECE;
    let heard = ask(
        &pane,
        &[
            ("open", json!({ "path": "e.txt" })),
            ("slice", json!({ "path": "e.txt", "version": 1, "from": 0 })),
            (
                "slice",
                json!({ "path": "e.txt", "version": 1, "from": budget }),
            ),
            (
                "slice",
                json!({ "path": "e.txt", "version": 1, "from": 1_200_000 }),
            ),
            // A place inside a character is the end of nothing.
            ("slice", json!({ "path": "e.txt", "version": 1, "from": 2 })),
            ("slice", json!({ "path": "e.txt", "version": 2, "from": 0 })),
        ],
    );
    assert_eq!(heard[0].result()["partial"], json!({ "length": 1_200_000 }));
    assert_eq!(heard[0].result()["text"], json!(""));
    let first = heard[1].result();
    assert_eq!(first["end"], json!(budget));
    assert_eq!(first["text"].as_str().unwrap().chars().count(), budget / 4);
    let second = heard[2].result();
    assert_eq!(second["end"], json!(budget * 2));
    assert_eq!(second["text"].as_str().unwrap(), "😀".repeat(budget / 4));
    assert_eq!(heard[3].result(), &json!({ "text": "", "end": 1_200_000 }));
    assert_eq!(heard[4].result(), &json!({ "text": "", "end": 2 }));
    assert!(heard[5].failed("out-of-sync"));
}

/// A PNG that is only the header that says its size, which is all the program reads of it.
fn png_header(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    bytes.extend(width.to_be_bytes());
    bytes.extend(height.to_be_bytes());
    bytes.extend([8, 2, 0, 0, 0]);
    bytes
}

#[test]
fn a_picture_is_measured_and_given_to_the_terminal_to_hold_and_what_is_none_is_refused() {
    let pane = Pane::new();
    let logo = png_header(640, 480);
    pane.write("logo.png", &logo);
    pane.write("art/mark.svg", "<svg viewBox='0 0 24 12'/>");
    pane.write("wide.png", png_header(4097, 10));
    pane.write("words.png", "just words");
    pane.write("notes.txt", &logo);
    pane.write("docs/guide.md", "");
    pane.write("docs/x.png", &logo);
    let heard = ask(
        &pane,
        &[
            (
                "picture",
                json!({ "path": "logo.png", "owner": "logo.png" }),
            ),
            (
                "picture",
                json!({ "path": "art/mark.svg", "owner": "art/mark.svg" }),
            ),
            (
                "picture",
                json!({ "path": "wide.png", "owner": "wide.png" }),
            ),
            (
                "picture",
                json!({ "path": "words.png", "owner": "words.png" }),
            ),
            (
                "picture",
                json!({ "path": "notes.txt", "owner": "notes.txt" }),
            ),
            (
                "picture",
                json!({ "path": "gone.png", "owner": "gone.png" }),
            ),
            (
                "picture",
                json!({ "path": "x.png", "owner": "g", "from": "docs/guide.md" }),
            ),
            (
                "picture",
                json!({ "path": "../logo.png", "owner": "g", "from": "docs/guide.md" }),
            ),
            (
                "picture",
                json!({ "path": "../../logo.png", "owner": "g", "from": "docs/guide.md" }),
            ),
            ("release", json!({ "owner": "logo.png" })),
            ("release", json!({ "owner": "nobody" })),
        ],
    );
    let named = heard[0].result();
    // What the terminal calls a file it holds: its digest.
    assert!(
        named["name"].as_str().unwrap().starts_with("sha256:"),
        "{named}"
    );
    assert_eq!(named["name"].as_str().unwrap().len(), "sha256:".len() + 64);
    assert_eq!(named["size"], json!(logo.len()));
    assert_eq!(
        (named["width"].clone(), named["height"].clone()),
        (json!(640), json!(480))
    );
    let drawing = heard[1].result();
    assert_eq!(
        (drawing["width"].clone(), drawing["height"].clone()),
        (json!(24), json!(12))
    );
    assert_eq!(
        heard[2].result(),
        &json!({ "refused": "too-large", "size": png_header(4097, 10).len() })
    );
    for refused in [3, 4, 5] {
        assert_eq!(
            heard[refused].result(),
            &json!({ "refused": "not-a-picture" }),
            "{refused}"
        );
    }
    // A Markdown file's: beside it, up from it inside the project, and not out of the project.
    assert_eq!(heard[6].result()["name"], named["name"]);
    assert_eq!(heard[7].result()["name"], named["name"]);
    assert_eq!(heard[8].result(), &json!({ "refused": "not-a-picture" }));
    assert_eq!(heard[9].result(), &json!({}));
    assert_eq!(heard[10].result(), &json!({}));
}

/// The PDF the tests read: three pages, 612 by 792, 595 by 842 and 300 by 200 points. The first has
/// "Hello world" and "a needle in a haystack", the second "Second page" and, in bold, "Needle"; the
/// third draws a shape and no letter.
const PAPER: &[u8] = include_bytes!("fixtures/pages.pdf");

#[test]
fn a_pdf_is_given_to_the_terminal_with_its_pages_and_the_words_of_a_page_are_asked_for() {
    let pane = Pane::new();
    pane.write("paper.pdf", PAPER);
    let heard = ask(
        &pane,
        &[
            (
                "picture",
                json!({ "path": "paper.pdf", "owner": "paper.pdf" }),
            ),
            ("pdf-text", json!({ "owner": "paper.pdf", "page": 1 })),
            ("pdf-text", json!({ "owner": "paper.pdf", "page": 2 })),
            ("pdf-text", json!({ "owner": "paper.pdf", "page": 3 })),
            ("pdf-text", json!({ "owner": "paper.pdf", "page": 4 })),
            ("pdf-text", json!({ "owner": "paper.pdf", "page": 0 })),
            ("pdf-text", json!({ "owner": "nobody", "page": 1 })),
            ("pdf-text", json!({ "owner": "paper.pdf" })),
            ("pdf-text", json!({ "owner": "paper.pdf", "page": -1 })),
            ("release", json!({ "owner": "paper.pdf" })),
            ("pdf-text", json!({ "owner": "paper.pdf", "page": 1 })),
        ],
    );
    let paper = heard[0].result();
    assert!(
        paper["name"].as_str().unwrap().starts_with("sha256:"),
        "{paper}"
    );
    assert_eq!(paper["size"], json!(PAPER.len()));
    // The first page's size, then the pages' by runs of one size, then whether there are words.
    assert_eq!(
        (paper["width"].clone(), paper["height"].clone()),
        (json!(612), json!(792))
    );
    assert_eq!(
        paper["pdf"],
        json!({
            "pages": 3,
            "sizes": [[1, 612.0, 792.0], [1, 595.0, 842.0], [1, 300.0, 200.0]],
            "text": true,
        })
    );
    let first = heard[1].result();
    assert_eq!(first["page"], json!(1));
    assert_eq!(first["truncated"], json!(false));
    let lines = first["lines"].as_array().unwrap();
    assert_eq!(lines.len(), 2);
    let hello = &lines[0];
    assert_eq!(hello["text"], json!("Hello world"));
    // Set 700 points up a page 792 tall, 72 in: the box has the baseline, 92 points from the top.
    assert!((hello["x"].as_f64().unwrap() - 72.0).abs() < 0.5, "{hello}");
    let (top, height) = (hello["y"].as_f64().unwrap(), hello["h"].as_f64().unwrap());
    assert!(top < 92.0 && top + height > 92.0, "{hello}");
    // A place for the left edge of every letter and one for the right edge of the last.
    let xs = hello["xs"].as_array().unwrap();
    assert_eq!(xs.len(), "Hello world".chars().count() + 1);
    assert_eq!(xs[0], json!(0.0));
    assert_eq!(xs.last(), Some(&hello["w"]));
    assert_eq!(lines[1]["text"], json!("a needle in a haystack"));
    let texts: Vec<&str> = heard[2].result()["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|line| line["text"].as_str().unwrap())
        .collect();
    assert_eq!(texts, ["Second page", "Needle"]);
    // A page with no letters has no lines.
    assert_eq!(
        heard[3].result(),
        &json!({ "page": 3, "lines": [], "truncated": false })
    );
    // A page the PDF has not, a tab that shows none, and a PDF let go.
    for failed in [4, 6, 10] {
        assert!(
            heard[failed].failed("failed"),
            "{failed}: {:?}",
            heard[failed]
        );
    }
    // Pages are counted from 1: page 0 is no number of one.
    assert!(heard[5].failed("bad-params"));
    assert!(heard[5].message().contains("page: expected at least 1"));
    assert!(heard[4].message().contains("no page 4"));
    assert!(heard[6].message().contains("not open"));
    assert!(heard[7].failed("bad-params"));
    assert!(heard[8].failed("bad-params"));
    assert_eq!(heard[9].result(), &json!({}));
}

#[test]
fn a_pdf_is_searched_through_all_its_pages_for_words_whole_words_and_patterns() {
    let pane = Pane::new();
    pane.write("paper.pdf", PAPER);
    let find = |params: Value| {
        let mut params = params;
        params["owner"] = json!("paper.pdf");
        params
    };
    let heard = ask(
        &pane,
        &[
            (
                "picture",
                json!({ "path": "paper.pdf", "owner": "paper.pdf" }),
            ),
            ("pdf-find", find(json!({ "query": "needle" }))),
            (
                "pdf-find",
                find(json!({ "query": "needle", "caseSensitive": true })),
            ),
            (
                "pdf-find",
                find(json!({ "query": "need", "wholeWord": true })),
            ),
            (
                "pdf-find",
                find(json!({ "query": "h.ystack|sec\\w+", "regex": true })),
            ),
            ("pdf-find", find(json!({ "query": "absent" }))),
            ("pdf-find", find(json!({ "query": "" }))),
            ("pdf-find", find(json!({ "query": "(", "regex": true }))),
            ("pdf-find", json!({ "owner": "nobody", "query": "needle" })),
            ("pdf-find", json!({ "owner": "paper.pdf" })),
        ],
    );
    let found = heard[1].result();
    assert_eq!(found["searched"], json!(3));
    assert_eq!(found["truncated"], json!(false));
    assert!(found["ms"].is_u64());
    let matches = found["matches"].as_array().unwrap();
    // The pages the words are on, and a box for each inside its page.
    let on: Vec<&Value> = matches.iter().map(|found| &found["page"]).collect();
    assert_eq!(on, [&json!(1), &json!(2)]);
    for found in matches {
        let (width, height) = if found["page"] == json!(1) {
            (612.0, 792.0)
        } else {
            (595.0, 842.0)
        };
        let n = |key: &str| found[key].as_f64().unwrap();
        assert!(n("x") >= 0.0 && n("x") + n("w") <= width, "{found}");
        assert!(n("y") >= 0.0 && n("y") + n("h") <= height, "{found}");
        assert!(n("w") > 10.0 && n("h") > 5.0, "{found}");
    }
    // "Needle" of the second page is set at 100 points in and 380 up a page 842 tall.
    let second = &matches[1];
    assert!(
        (second["x"].as_f64().unwrap() - 100.0).abs() < 1.0,
        "{second}"
    );
    let (top, height) = (second["y"].as_f64().unwrap(), second["h"].as_f64().unwrap());
    assert!(
        top < 842.0 - 380.0 && top + height > 842.0 - 380.0,
        "{second}"
    );
    assert_eq!(heard[2].result()["matches"].as_array().unwrap().len(), 1);
    assert_eq!(heard[3].result()["matches"], json!([]));
    assert_eq!(heard[4].result()["matches"].as_array().unwrap().len(), 2);
    assert_eq!(heard[5].result()["matches"], json!([]));
    assert_eq!(heard[5].result()["searched"], json!(3));
    // A query of nothing is no query.
    assert!(heard[6].failed("bad-params"));
    assert!(heard[7].failed("failed"));
    assert!(heard[7].message().contains("Not a regular expression"));
    assert!(heard[8].failed("failed"));
    assert!(heard[9].failed("bad-params"));
}

#[test]
fn what_is_named_a_pdf_and_is_none_or_is_too_large_is_refused_and_a_markdown_files_picture_is_no_pdf()
 {
    let pane = Pane::new();
    pane.write("words.pdf", "just words");
    pane.write("cut.pdf", &PAPER[..PAPER.len() / 5]);
    let mut big = PAPER.to_vec();
    big.resize(8 * 1024 * 1024 + 1, b' ');
    pane.write("big.pdf", &big);
    pane.write("docs/guide.md", "");
    pane.write("docs/paper.pdf", PAPER);
    // The terminal tells a PDF by its bytes: under another name it is one still, and a name that
    // says PDF to a file that is another picture does not make it one.
    pane.write("paper.png", PAPER);
    pane.write("logo.pdf", png_header(640, 480));
    pane.write("many.pdf", pdf_of_pages(&vec![(100.0, 100.0); 10_001]));
    let heard = ask(
        &pane,
        &[
            (
                "picture",
                json!({ "path": "words.pdf", "owner": "words.pdf" }),
            ),
            ("picture", json!({ "path": "cut.pdf", "owner": "cut.pdf" })),
            ("picture", json!({ "path": "big.pdf", "owner": "big.pdf" })),
            (
                "picture",
                json!({ "path": "paper.pdf", "owner": "g", "from": "docs/guide.md" }),
            ),
            (
                "picture",
                json!({ "path": "paper.png", "owner": "paper.png" }),
            ),
            (
                "picture",
                json!({ "path": "logo.pdf", "owner": "logo.pdf" }),
            ),
            (
                "picture",
                json!({ "path": "many.pdf", "owner": "many.pdf" }),
            ),
            ("pdf-text", json!({ "owner": "words.pdf", "page": 1 })),
            ("pdf-text", json!({ "owner": "many.pdf", "page": 1 })),
        ],
    );
    assert_eq!(heard[0].result(), &json!({ "refused": "not-a-picture" }));
    assert_eq!(heard[1].result(), &json!({ "refused": "not-a-picture" }));
    assert_eq!(
        heard[2].result(),
        &json!({ "refused": "too-large", "size": big.len() })
    );
    assert_eq!(heard[3].result(), &json!({ "refused": "not-a-picture" }));
    assert_eq!(heard[4].result()["pdf"]["pages"], json!(3));
    // A PNG under the name of a PDF is a PNG.
    assert_eq!(heard[5].result()["width"], json!(640));
    assert!(heard[5].result().get("pdf").is_none());
    // Too many pages to be told in one message.
    assert_eq!(heard[6].result()["refused"], json!("too-large"));
    assert!(heard[7].failed("failed"));
    assert!(heard[8].failed("failed"));
}

#[test]
fn the_pages_of_the_longest_pdf_are_told_in_one_message_even_when_no_two_are_alike() {
    let pane = Pane::new();
    // The widest sizes to say, one after the other different, so that no run holds two.
    let sizes: Vec<(f32, f32)> = (0..10_000)
        .map(|at| (16_383.99 - (at % 2) as f32 * 0.01, 16_383.99))
        .collect();
    pane.write("long.pdf", pdf_of_pages(&sizes));
    let heard = ask(
        &pane,
        &[
            (
                "picture",
                json!({ "path": "long.pdf", "owner": "long.pdf" }),
            ),
            ("pdf-text", json!({ "owner": "long.pdf", "page": 10_000 })),
            ("pdf-find", json!({ "owner": "long.pdf", "query": "x" })),
        ],
    );
    let long = heard[0].result();
    assert_eq!(long["pdf"]["pages"], json!(10_000));
    let runs = long["pdf"]["sizes"].as_array().unwrap();
    assert_eq!(runs.len(), 10_000);
    assert_eq!(runs[0], json!([1, 16_383.99, 16_383.99]));
    assert_eq!(runs[1], json!([1, 16_383.98, 16_383.99]));
    let told = serde_json::to_string(long).unwrap().len();
    assert!(told < PIECE, "{told} bytes");
    assert_eq!(
        heard[1].result(),
        &json!({ "page": 10_000, "lines": [], "truncated": false })
    );
    assert_eq!(heard[2].result()["matches"], json!([]));
    assert_eq!(heard[2].result()["searched"], json!(10_000));
}

#[test]
fn a_page_with_more_lines_than_a_message_carries_is_told_as_many_as_fit_and_says_so() {
    let pane = Pane::new();
    let lines: Vec<String> = (0..3000)
        .map(|at| format!("Line {at} of a page that has more words than a message may carry"))
        .collect();
    pane.write("dense.pdf", pdf_of_lines(&lines));
    let heard = ask(
        &pane,
        &[
            (
                "picture",
                json!({ "path": "dense.pdf", "owner": "dense.pdf" }),
            ),
            ("pdf-text", json!({ "owner": "dense.pdf", "page": 1 })),
            (
                "pdf-find",
                json!({ "owner": "dense.pdf", "query": "Line", "wholeWord": true }),
            ),
        ],
    );
    assert_eq!(heard[0].result()["pdf"]["text"], json!(true));
    let page = heard[1].result();
    let told = page["lines"].as_array().unwrap();
    assert_eq!(page["truncated"], json!(true));
    assert!(
        told.len() > 100 && told.len() < lines.len(),
        "{}",
        told.len()
    );
    // The lines told are the first ones, from the top of the page.
    assert_eq!(
        told[0]["text"],
        json!("Line 0 of a page that has more words than a message may carry")
    );
    assert_eq!(
        told[told.len() - 1]["text"],
        json!(format!(
            "Line {} of a page that has more words than a message may carry",
            told.len() - 1
        ))
    );
    assert!(serde_json::to_string(page).unwrap().len() < PIECE + 4096);
    // A search is cut at 2000 matches, and says so.
    let found = heard[2].result();
    assert_eq!(found["truncated"], json!(true));
    assert_eq!(found["matches"].as_array().unwrap().len(), 2000);
    assert!(serde_json::to_string(found).unwrap().len() < PIECE + 4096);
}
