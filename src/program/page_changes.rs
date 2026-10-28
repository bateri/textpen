//! The marks of the lines that differ from git's index, as the page draws them and what the person
//! does with them: a bar along the lines changed or added and a triangle where lines were taken
//! away, the same on the overview ruler, a peek of the lines as they were under a change, Revert
//! Change, the change before and after, and the setting that turns them off. The page and the
//! program's own handlers in one process, as `page_tests` runs them, on a project that is a git
//! repository.
//!
//! Each opens the interpreter (compiled once a run): `make test-page` runs them, in `make check`.

use std::process::Command;

use pencere_drive::headless::Error;
use pencere_view::edit::{GutterShape, Mark};
use pencere_view::{Button, Role};

use crate::page_tests::{Page, Project, WIDE, lantern, page_words, shown, shows, text, wait};

/// The project made a repository, with everything as it is committed.
fn committed(project: &Project) {
    let git = |args: &[&str]| {
        let status = Command::new("git")
            .args(args)
            .current_dir(project.root())
            .env("GIT_AUTHOR_NAME", "textpen")
            .env("GIT_AUTHOR_EMAIL", "textpen@example.com")
            .env("GIT_COMMITTER_NAME", "textpen")
            .env("GIT_COMMITTER_EMAIL", "textpen@example.com")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .status()
            .expect("git");
        assert!(status.success(), "git {args:?}");
    };
    git(&["init", "-q", "-b", "main"]);
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "The shop"]);
}

/// What the editor shown marks in the column of changes: the shape, the role of its colour and the
/// lines it is by.
fn marks(page: &Page) -> Vec<(GutterShape, Option<Role>, usize, usize)> {
    let name = shown(page).expect("an editor");
    let buffer = page.editor(&name).expect("the editor");
    let mut found: Vec<_> = buffer
        .decorations_in(0, buffer.len_chars())
        .into_iter()
        .filter_map(|(_, decoration)| match &decoration.mark {
            Mark::Gutter(mark) if mark.column == "changes" => {
                let first = buffer.line_of(decoration.from);
                let last = if decoration.to > decoration.from {
                    buffer.line_of(decoration.to - 1)
                } else {
                    first
                };
                Some((mark.shape.clone(), mark.color.role(), first, last))
            }
            _ => None,
        })
        .collect();
    found.sort_by_key(|(_, _, first, last)| (*first, *last));
    found
}

/// How many marks the editor shown has on the overview ruler.
fn ruler(page: &Page) -> usize {
    let name = shown(page).expect("an editor");
    let buffer = page.editor(&name).expect("the editor");
    buffer
        .decorations_in(0, buffer.len_chars())
        .into_iter()
        .filter(|(_, decoration)| matches!(decoration.mark, Mark::Ruler(_)))
        .count()
}

/// The character where `needle` is in the text of the editor shown.
fn at(page: &Page, needle: &str) -> usize {
    let all = text(page);
    let byte = all.find(needle).expect("the words are in the text");
    all[..byte].chars().count()
}

const PORT_LINE: &str = "const PORT = Number(process.env.PORT ?? 8080);";

/// A file in a repository, as it is committed, has no marks, and the person's changes make them: a
/// line added is green, one changed blue, lines taken away a red triangle, each on the ruler too.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_lines_that_differ_from_the_index_are_marked_in_the_gutter_and_on_the_ruler()
-> Result<(), Error> {
    let project = lantern();
    committed(&project);
    let session = project.session(&["src/server.ts"]);
    let mut page = project.open(&session, WIDE)?;
    let editor = shown(&page).expect("an editor");
    // The column is there before there is anything in it.
    assert!(page.gutter_cell(&editor, "changes", 0).is_some());
    assert!(marks(&page).is_empty());
    // A line added at the top.
    page.caret_to(&editor, 0)?;
    page.type_text("// the server\n")?;
    wait(&mut page, |page| !marks(page).is_empty())?;
    assert_eq!(
        marks(&page),
        vec![(GutterShape::Bar, Some(Role::Green), 0, 0)]
    );
    // A line changed (its line is the 6th now) and one taken away (the last).
    let place = at(&page, "8080") + 4;
    page.caret_to(&editor, place)?;
    page.type_text("1")?;
    let last = text(&page).lines().count() - 1;
    let from = at(&page, "server.listen(PORT);");
    page.select(&editor, from, text(&page).chars().count())?;
    page.press("backspace")?;
    wait(&mut page, |page| marks(page).len() == 3)?;
    let found = marks(&page);
    assert_eq!(found[0], (GutterShape::Bar, Some(Role::Green), 0, 0));
    assert_eq!(found[1], (GutterShape::Bar, Some(Role::Blue), 5, 5));
    assert_eq!(found[2].1, Some(Role::Red));
    assert!(matches!(
        found[2].0,
        GutterShape::Above | GutterShape::Below
    ));
    assert!(
        found[2].2 >= last - 2,
        "{found:?} near the last line {last}"
    );
    assert!(ruler(&page) >= 3, "the ruler has {}", ruler(&page));
    // Said to a person who cannot see them: what a mark is in words, which the host shows too.
    assert!(
        page.outline()
            .tips
            .iter()
            .any(|tip| tip.words.starts_with("1 line changed")),
        "{:?}",
        page.outline().tips
    );
    // A change put back is no mark: both are undone, and the line added at the top is all that is left.
    page.press("cmd+z")?;
    page.press("cmd+z")?;
    wait(&mut page, |page| marks(page).len() == 1)?;
    Ok(())
}

/// A file git does not keep, and a change that is staged, have no marks: the original is the index.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_file_git_does_not_keep_and_a_change_that_is_staged_have_no_marks() -> Result<(), Error> {
    let project = lantern();
    committed(&project);
    project.write("notes/new.txt", "one\ntwo\n");
    let session = project.session(&["notes/new.txt", "src/server.ts"]);
    let mut page = project.open(&session, WIDE)?;
    let editor = shown(&page).expect("an editor");
    page.caret_to(&editor, 0)?;
    page.type_text("three\n")?;
    page.advance(1.5)?;
    assert!(marks(&page).is_empty(), "an untracked file has none");
    // The other file: changed, saved, and staged.
    page.press("ctrl+2")?;
    wait(&mut page, |page| text(page).starts_with("import"))?;
    let editor = shown(&page).expect("an editor");
    page.caret_to(&editor, 0)?;
    page.type_text("// x\n")?;
    wait(&mut page, |page| marks(page).len() == 1)?;
    page.press("cmd+s")?;
    wait(&mut page, |_| {
        project.read("src/server.ts").starts_with("// x")
    })?;
    let staged = Command::new("git")
        .args(["add", "src/server.ts"])
        .current_dir(project.root())
        .status()
        .expect("git");
    assert!(staged.success());
    // The index changed: the program says so on its own, and the marks go.
    wait(&mut page, |page| marks(page).is_empty())?;
    Ok(())
}

/// A press on a bar peeks the lines as they were and as they are, Esc closes it, and Revert Change
/// puts the lines back as one change that can be undone.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_press_on_a_mark_peeks_the_lines_as_they_were_and_revert_puts_them_back() -> Result<(), Error> {
    let project = lantern();
    committed(&project);
    let session = project.session(&["src/server.ts"]);
    let mut page = project.open(&session, WIDE)?;
    let editor = shown(&page).expect("an editor");
    let original = text(&page);
    let place = at(&page, "8080") + 4;
    page.caret_to(&editor, place)?;
    page.type_text("1")?;
    wait(&mut page, |page| !marks(page).is_empty())?;
    assert!(page.zone(&editor, "peek").is_none());
    // The bar of the line (the 5th): the peek stands in a gap after it.
    page.click_gutter(&editor, "changes", 4, Button::Left)?;
    let gap = page.zone(&editor, "peek").expect("a gap is open");
    let peek = page.bounds("peek").expect("the peek is on the page");
    assert_eq!((peek.x, peek.y), (gap.x, gap.y));
    assert!(shows(&page, "1 line changed"), "{:?}", page_words(&page));
    assert!(shows(&page, PORT_LINE), "the line as it was");
    assert!(shows(&page, "1 of 1"));
    // Esc closes it, and the gap with it.
    page.press("escape")?;
    assert!(page.zone(&editor, "peek").is_none());
    assert!(page.bounds("peek").is_none());
    // The key opens it again on the change the caret is in, and Revert Change takes the change back.
    page.press("alt+f5")?;
    assert!(page.zone(&editor, "peek").is_some());
    page.click("peek-revert")?;
    wait(&mut page, |page| text(page) == original)?;
    assert!(page.zone(&editor, "peek").is_none());
    wait(&mut page, |page| marks(page).is_empty())?;
    // An edit like any other: undone, the change is back.
    page.press("cmd+z")?;
    assert!(
        text(&page).contains("80801"),
        "{:?}",
        text(&page).lines().nth(4)
    );
    Ok(())
}

/// ⌥F5 and ⇧⌥F5 go from change to change round the file, the caret on the first line of each and the
/// peek open there.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_next_and_previous_change_go_round_the_file() -> Result<(), Error> {
    let project = lantern();
    committed(&project);
    let session = project.session(&["src/server.ts"]);
    let mut page = project.open(&session, WIDE)?;
    let editor = shown(&page).expect("an editor");
    let place = at(&page, "8080") + 4;
    page.caret_to(&editor, place)?;
    page.type_text("1")?;
    let place = at(&page, "res.end(") + 8;
    page.caret_to(&editor, place)?;
    page.type_text(" ")?;
    wait(&mut page, |page| marks(page).len() == 2)?;
    page.caret_to(&editor, 0)?;
    page.press("alt+f5")?;
    assert!(shows(&page, "1 of 2"), "{:?}", page_words(&page));
    page.press("alt+f5")?;
    assert!(shows(&page, "2 of 2"));
    page.press("alt+f5")?;
    assert!(shows(&page, "1 of 2"), "round the file");
    page.press("alt+shift+f5")?;
    assert!(shows(&page, "2 of 2"));
    // Both are in the palette and in the keys listed.
    page.press("f1")?;
    page.type_text("change")?;
    page.advance(0.5)?;
    assert!(shows(&page, "Go to Next Change") && shows(&page, "Revert Change"));
    Ok(())
}

/// The setting turns the marks off: no column, no marks, and the person's choice is kept.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_setting_turns_the_marks_off_and_on() -> Result<(), Error> {
    let project = lantern();
    committed(&project);
    let session = project.session(&["src/server.ts"]);
    let mut page = project.open(&session, WIDE)?;
    let editor = shown(&page).expect("an editor");
    page.caret_to(&editor, 0)?;
    page.type_text("// x\n")?;
    wait(&mut page, |page| !marks(page).is_empty())?;
    page.press("f1")?;
    page.type_text("Toggle Changes in the Gutter")?;
    page.advance(0.3)?;
    page.press("enter")?;
    page.advance(1.0)?;
    assert!(marks(&page).is_empty());
    assert!(page.gutter_cell(&editor, "changes", 0).is_none());
    wait(&mut page, |_| !session.settings().diff_decorations)?;
    // On again: asked again, and marked.
    page.press("f1")?;
    page.type_text("Toggle Changes in the Gutter")?;
    page.advance(0.3)?;
    page.press("enter")?;
    wait(&mut page, |page| !marks(page).is_empty())?;
    assert!(page.gutter_cell(&editor, "changes", 0).is_some());
    Ok(())
}
