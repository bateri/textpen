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

use common::{Answer, Pane, Script, program, pushes, run};
use serde_json::{Value, json};

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
            "tabs": [], "active": null, "expanded": [], "places": {}, "recent": [], "preview": []
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
            "sidebarWidth": 240,
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
    assert_eq!(boot["git"], Value::Null);
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
        })
    );
    assert_eq!(boot["handoff"], json!(false));
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
        "vim": true,
    });
    let heard = ask(&pane, &[("settings", json!({ "settings": settings }))]);
    let saved = &heard[0].result()["settings"];
    assert_eq!(saved["tabSize"], json!(1));
    assert_eq!(saved["sidebarWidth"], json!(160));
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
fn edits_count_code_points_and_a_file_is_written_as_it_was_read() {
    let pane = Pane::new();
    // "a", then ş (two bytes), then an emoji (four bytes), then "z": one code point each, so the
    // emoji is at 2..3 and z at 3..4.
    pane.write("u.txt", "\u{feff}aş😀z\r\nlast\r\n");
    let heard = ask(
        &pane,
        &[
            ("open", json!({ "path": "u.txt" })),
            (
                "edit",
                json!({ "path": "u.txt", "base": 1, "edits": [[3, 4, "Z"], [2, 3, "<>"]] }),
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
                json!({ "path": "u.txt", "base": 2, "edits": [[1, 1, "€"], [8, 8, "𝄞"]] }),
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
    // "a€ş<>Z\n" is seven code points: the second edit, on what the first left, is after "l".
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
fn a_folder_of_thousands_is_cut_at_five_thousand_and_the_files_of_a_big_project_come_a_messages_worth_at_a_time()
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
    assert_eq!(listing["entries"].as_array().unwrap().len(), 5000);
    assert_eq!(listing["truncated"], json!(true));
    assert_eq!(listing["entries"][0]["name"], json!("f0000"));
    let page = heard[1].result();
    let next = page["next"].as_u64().expect("the first page is a part");
    assert!(next > 0 && next < 3600 + 5100);
    assert!(
        page.to_string().len() <= 1024 * 1024,
        "a page fits a message"
    );
    assert_eq!(page["truncated"], json!(false));
    let rest = ask(&pane, &[("files", json!({ "from": next }))]);
    assert_eq!(rest[0].result()["next"], Value::Null);
    let mut all = strings(&page["files"]);
    all.extend(strings(&rest[0].result()["files"]));
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
    let git_info = &heard[0].result()["git"];
    assert_eq!(git_info["branch"], json!("trunk"));
    assert_eq!(git_info["ahead"], json!(0));
    assert_eq!(
        git_info["changes"],
        json!({ "kept.txt": "modified", "new file.txt": "untracked", "ünï/çödé.txt": "untracked" })
    );
    let mut files = strings(&heard[1].result()["files"]);
    files.sort();
    assert_eq!(
        files,
        [".gitignore", "kept.txt", "new file.txt", "ünï/çödé.txt"]
    );
    assert_eq!(heard[1].result()["next"], Value::Null);
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
    assert_eq!(pushes[0], json!({ "buffers": [], "dirs": [], "git": null }));
    assert!(
        pushes[1].get("git").is_none(),
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
fn quitting_by_hangup_keeps_what_is_not_saved_as_a_draft_with_its_length_in_code_points() {
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
    // "world😀!\n" is eight code points.
    assert_eq!(heard[0].result()["draft"]["length"], json!(8));
    assert!(heard[1].result().get("draft").is_none());
    assert_eq!(heard[1].result()["text"], json!("world\n"));
    assert!(pane.drafts().is_empty(), "a dropped draft is gone");
}

#[test]
fn a_long_text_of_four_byte_characters_is_read_in_pieces_that_count_code_points() {
    // 300,000 emoji are 1.2 MB: more than a message holds, and each piece ends between two of them.
    let pane = Pane::new();
    pane.write("e.txt", "😀".repeat(300_000));
    let budget = 900 * 1024;
    let heard = ask(
        &pane,
        &[
            ("open", json!({ "path": "e.txt" })),
            ("slice", json!({ "path": "e.txt", "version": 1, "from": 0 })),
            (
                "slice",
                json!({ "path": "e.txt", "version": 1, "from": budget / 4 }),
            ),
            (
                "slice",
                json!({ "path": "e.txt", "version": 1, "from": 300_000 }),
            ),
            ("slice", json!({ "path": "e.txt", "version": 2, "from": 0 })),
        ],
    );
    assert_eq!(heard[0].result()["partial"], json!({ "length": 300_000 }));
    assert_eq!(heard[0].result()["text"], json!(""));
    let first = heard[1].result();
    assert_eq!(first["end"], json!(budget / 4));
    assert_eq!(first["text"].as_str().unwrap().chars().count(), budget / 4);
    let second = heard[2].result();
    assert_eq!(second["end"], json!(300_000));
    assert_eq!(
        second["text"].as_str().unwrap(),
        "😀".repeat(300_000 - budget / 4)
    );
    assert_eq!(heard[3].result(), &json!({ "text": "", "end": 300_000 }));
    assert!(heard[4].failed("out-of-sync"));
}
