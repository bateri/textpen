//! What an agent reads of the page, whole, in the states that matter, against goldens, and the reading audit
//! of each: how the page is read is held to what is written down.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::page_tests::{PANE, Page, Project, WIDE, lantern, shown, shows, wait};
use crate::state::session::Session;

/// Sets what the page reads now against the golden `name` (`src/program/reading/<name>.txt`), and holds the
/// page to the reading audit: a control with no name, an item said twice, a modal's page read behind it.
/// `PENCERE_BLESS=1` writes the goldens (read the diff: it is how an agent reads the page); `READING_OUT`
/// names a folder the readings are copied to, for comparing two pencere.
fn check(name: &str, page: &mut Page) {
    // The project is made in a folder of its own for each run: its path is not part of what is read.
    let reading: String = page
        .reading()
        .text()
        .split_inclusive('\n')
        .map(|line| match line.trim_start().strip_prefix("text \"/") {
            Some(path) if path.trim_end().ends_with("/lantern\"") => {
                let indent = &line[..line.len() - line.trim_start().len()];
                format!("{indent}text \"<project>/lantern\"\n")
            }
            _ => line.to_owned(),
        })
        .collect();
    if let Some(dir) = std::env::var_os("READING_OUT") {
        let dir = PathBuf::from(dir);
        std::fs::create_dir_all(&dir).expect("a folder for the readings");
        std::fs::write(dir.join(format!("{name}.txt")), &reading).expect("a reading");
    }
    let file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/program/reading")
        .join(format!("{name}.txt"));
    if std::env::var_os("PENCERE_BLESS").is_some() {
        std::fs::create_dir_all(file.parent().expect("a folder"))
            .expect("a folder for the goldens");
        std::fs::write(&file, &reading).expect("a golden");
    } else {
        let golden = std::fs::read_to_string(&file)
            .unwrap_or_else(|_| panic!("{name}: no golden at {}: PENCERE_BLESS=1", file.display()));
        assert_eq!(
            golden, reading,
            "{name}: the page is read otherwise than the golden says: PENCERE_BLESS=1 writes it"
        );
    }
    let flaws: String = pencere_drive::audit::report(&page.audit())
        .lines()
        .filter(|line| {
            !WAIVED
                .iter()
                .any(|(state, flaw)| *state == name && line.contains(flaw))
        })
        .map(|line| format!("{line}\n"))
        .collect();
    assert!(
        flaws.is_empty(),
        "{name}: the reading audit finds:\n{flaws}"
    );
}

/// What the audit finds that is not this page's to mend, by the state it is found in: a task's box in a
/// markdown preview is the kit's (`@pencere/markdown`): a checkbox that shows a state and no name or id
/// (a gap of the kit, carried in set 037's plan).
const WAIVED: &[(&str, &str)] = &[("preview", "checkbox \"\"")];

/// The lantern project with `tabs` open, and its page at `size`.
fn open_on(tabs: &[&str], size: (f32, f32)) -> (Project, Arc<Session>, Page) {
    let project = lantern();
    let session = project.session(tabs);
    let page = project.open(&session, size).expect("the page opens");
    (project, session, page)
}

const SERVER_AND_README: [&str; 2] = ["src/server.ts", "README.md"];

#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_page_is_read_in_its_states() {
    // The editor in front, the tree beside it, the tabs above.
    let (_project, _session, mut page) = open_on(&SERVER_AND_README, WIDE);
    check("main", &mut page);

    // The editor's menu, open at the caret.
    page.press("shift+f10").expect("the menu key");
    page.advance(0.3).expect("a moment");
    check("menu", &mut page);

    // The markdown file in front with its preview.
    let (_project, _session, mut page) = open_on(&["README.md"], WIDE);
    page.press("cmd+shift+v").expect("the preview key");
    page.advance(0.3).expect("a moment");
    check("preview", &mut page);

    // The find bar, with a match chosen.
    let (_project, _session, mut page) = open_on(&["src/server.ts"], WIDE);
    page.press("cmd+f").expect("find");
    page.type_text("send").expect("the pattern");
    wait(&mut page, |page| shows(page, "1 of 3")).expect("the count");
    page.advance(0.3).expect("a moment");
    check("find", &mut page);

    // The project's search, a match chosen.
    let (_project, _session, mut page) = open_on(&["README.md"], WIDE);
    page.press("cmd+shift+f").expect("search");
    page.type_text("order").expect("the words");
    page.advance(0.5).expect("a moment");
    wait(&mut page, |page| shows(page, "orders.ts")).expect("the matches");
    page.press("down").expect("down");
    page.advance(0.3).expect("a moment");
    check("search", &mut page);

    // The tree: folders open, a row chosen by the keyboard.
    let (_project, _session, mut page) = open_on(&[], WIDE);
    page.click("row:src").expect("src");
    wait(&mut page, |page| page.bounds("row:src/lib").is_some()).expect("src is open");
    page.click("row:src/lib").expect("src/lib");
    wait(&mut page, |page| {
        page.bounds("row:src/lib/format.ts").is_some()
    })
    .expect("src/lib is open");
    page.press("cmd+shift+e")
        .expect("the tree takes the keyboard");
    page.advance(0.3).expect("a moment");
    check("tree", &mut page);

    // The menu of a row of the tree.
    page.right_click("row:src/lib/format.ts")
        .expect("a right press");
    page.advance(0.3).expect("a moment");
    check("tree_menu", &mut page);

    // The quick open over the page, with a file found.
    let (_project, _session, mut page) = open_on(&SERVER_AND_README, WIDE);
    page.press("cmd+k").expect("quick open");
    page.type_text("ord").expect("the letters");
    wait(&mut page, |page| shows(page, "orders.ts")).expect("a file is found");
    page.advance(0.3).expect("a moment");
    check("palette_files", &mut page);

    // The command palette.
    let (_project, _session, mut page) = open_on(&SERVER_AND_README, WIDE);
    page.press("f1").expect("the palette");
    page.advance(0.3).expect("a moment");
    check("palette_commands", &mut page);

    // The open files.
    let (_project, _session, mut page) = open_on(&SERVER_AND_README, WIDE);
    page.press("cmd+e").expect("the open files");
    page.advance(0.3).expect("a moment");
    check("palette_open_files", &mut page);

    // The settings sheet, the second row chosen.
    let (_project, _session, mut page) = open_on(&SERVER_AND_README, WIDE);
    page.press("cmd+,").expect("the settings");
    page.advance(0.3).expect("a moment");
    page.press("down").expect("down");
    page.advance(0.3).expect("a moment");
    check("settings", &mut page);

    // The sheet of keys, filtered.
    let (_project, _session, mut page) = open_on(&SERVER_AND_README, WIDE);
    page.press("cmd+shift+/").expect("the keys");
    page.advance(0.3).expect("a moment");
    check("keys", &mut page);
    page.type_text("close tab").expect("the filter");
    page.advance(0.3).expect("a moment");
    check("keys_filtered", &mut page);

    // A dialog with a field: a new file.
    let (_project, _session, mut page) = open_on(&SERVER_AND_README, WIDE);
    page.press("cmd+alt+n").expect("the dialog");
    page.advance(0.3).expect("a moment");
    check("dialog_new", &mut page);

    // A dialog with buttons: closing a tab with changes.
    let (_project, _session, mut page) = open_on(&SERVER_AND_README, WIDE);
    let editor = shown(&page).expect("an editor");
    page.caret_to(&editor, 0).expect("the caret");
    page.type_text("x").expect("a letter");
    page.advance(1.0).expect("a moment");
    page.press("ctrl+w").expect("close the tab");
    wait(&mut page, |page| {
        shows(page, "Save the changes to server.ts?")
    })
    .expect("the dialog");
    page.advance(0.3).expect("a moment");
    check("dialog_save", &mut page);

    // A note that comes in over the page: the file changed on disk and was read again.
    let (project, _session, mut page) = open_on(&["README.md"], WIDE);
    project.write("README.md", "# Lantern\n\nChanged on disk.\n");
    wait(&mut page, |page| {
        shows(page, "changed on disk and was read again")
    })
    .expect("the note");
    page.advance(0.5).expect("a moment");
    check("note", &mut page);

    // A banner: the file changed on disk while it had changes here.
    let (project, _session, mut page) = open_on(&["README.md"], WIDE);
    let editor = shown(&page).expect("an editor");
    page.caret_to(&editor, 0).expect("the caret");
    page.type_text("x").expect("a letter");
    page.advance(1.0).expect("a moment");
    project.write("README.md", "# Other\n");
    wait(&mut page, |page| {
        shows(page, "changed on disk while it had changes here")
    })
    .expect("the banner");
    page.advance(0.3).expect("a moment");
    check("banner", &mut page);

    // An 80x24 pane: the sidebar as a drawer over the page.
    let project = lantern();
    let session = project.session_on(&["README.md"]);
    let mut page = project.open(&session, PANE).expect("the page opens");
    check("pane", &mut page);
    page.press("cmd+b").expect("the drawer");
    wait(&mut page, |page| page.bounds("sidebar").is_some()).expect("the drawer is open");
    page.advance(0.3).expect("a moment");
    check("drawer", &mut page);

    // With no file open: the start page.
    let (_project, _session, mut page) = open_on(&[], PANE);
    check("start", &mut page);
}
