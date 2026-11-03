//! A file side by side with what the last commit keeps of it, as the page lays it out and what the
//! person does in it: the commit's text on the left and the file's own editor on the right, the same
//! lines at the same height (a gap on the side with fewer lines), the two scrolled together, the
//! lines and words that differ in colour, and the way out. The page and the program's own handlers
//! in one process, as `page_tests` runs them, on a project that is a git repository.
//!
//! Each opens the interpreter (compiled once a run): `make test-page` runs them, in `make check`.

use std::process::Command;

use pencere_drive::headless::{Error, Focus};
use pencere_view::{Id, MenuEntry};
use similar::{DiffOp, TextDiff};

use crate::page_changes::committed;
use crate::page_tests::{PANE, Page, Project, WIDE, lantern, page_words, shown, shows, text, wait};

/// The file the tests compare, as it was committed (`page_tests::SERVER`), and as it is when the page
/// opens it: lines added at the top, one taken out, a number changed, one line made into three, and
/// the last lines gone.
const CHANGED: &str = "#!/usr/bin/env node\n// The shop's server.\nimport { createServer } from \"node:http\";\n\nconst PORT = Number(process.env.PORT ?? 3000);\n\n/** Answers with JSON. */\nfunction send(res, status: number, body: unknown) {\n  res.end(JSON.stringify(body));\n}\n\nconst server = createServer(async (req, res) => {\n  if (req.method === \"GET\") {\n    return send(res, 200, []);\n  }\n  // Nothing else is served.\n  const known = false;\n  send(res, 404, { error: `no route for ${req.method}`, known });\n});\n";

/// The project committed, and `src/server.ts` as `CHANGED` on disk.
fn changed_project() -> Project {
    let project = lantern();
    committed(&project);
    project.write("src/server.ts", CHANGED);
    project
}

/// The name of the left-hand editor, where the comparison is shown.
pub(crate) fn left_of(page: &Page) -> Option<String> {
    let laid = page.laid()?;
    let rooms = laid.editors();
    (1..50).map(|n| format!("compare:{n}")).find(|name| {
        rooms
            .iter()
            .any(|room| !room.hidden && room.id == Id::named(name))
    })
}

/// The two sides are on the page, their text is in, and the page has told them what differs.
pub(crate) fn drawn(page: &Page) -> bool {
    let (Some(left), Some(right)) = (left_of(page), shown(page)) else {
        return false;
    };
    let told = |name: &str| {
        page.editor(name)
            .map(|buffer| buffer.layer_names())
            .unwrap_or_default()
            .iter()
            .any(|layer| layer.starts_with("compare"))
    };
    page.editor(&left).is_some() && (told(&left) || told(&right))
}

/// The file shown is compared, and it is laid out.
fn compared(page: &mut Page) -> Result<(String, String), Error> {
    page.press("cmd+shift+d")?;
    wait(page, drawn)?;
    page.advance(0.5)?;
    page.settle()?;
    Ok((
        left_of(page).expect("a left editor"),
        shown(page).expect("a right editor"),
    ))
}

/// How tall the content of an editor is, and where it is scrolled (down, across).
fn extent(page: &Page, name: &str) -> (f32, f32, f32) {
    let found = page
        .laid()
        .and_then(|laid| laid.scroll_extent(Id::named(name)))
        .expect("an extent");
    (found.content.h, found.offset.y, found.offset.x)
}

/// The pairs of lines (old, new; from 0) the two texts have the same and no other line of either has:
/// where one stands in the other, whichever way the texts are lined up.
fn same_lines(old: &str, new: &str) -> Vec<(usize, usize)> {
    let once = |text: &str, line: &str| text.lines().filter(|other| *other == line).count() == 1;
    let diff = TextDiff::from_lines(old, new);
    let mut out = Vec::new();
    for op in diff.ops() {
        if let DiffOp::Equal {
            old_index,
            new_index,
            len,
        } = *op
        {
            for k in 0..len {
                let line = old.lines().nth(old_index + k).unwrap_or_default();
                if !line.trim().is_empty() && once(old, line) && once(new, line) {
                    out.push((old_index + k, new_index + k));
                }
            }
        }
    }
    out
}

/// How many of the pairs have both lines in view, and the worst distance between the heights they
/// stand at, points.
fn apart(page: &Page, left: &str, right: &str, pairs: &[(usize, usize)]) -> (usize, f32) {
    let (mut seen, mut worst) = (0, 0.0_f32);
    for &(old, new) in pairs {
        let (Some(a), Some(b)) = (
            page.gutter_cell(left, "numbers", old),
            page.gutter_cell(right, "numbers", new),
        ) else {
            continue;
        };
        seen += 1;
        if (a.y - b.y).abs() >= 0.5 && std::env::var("COMPARE_DEBUG").is_ok() {
            eprintln!("apart: old {old} at {} / new {new} at {}", a.y, b.y);
        }
        worst = worst.max((a.y - b.y).abs());
    }
    (seen, worst)
}

/// The two sides are as tall as each other, and the lines they have the same are at the same height.
#[track_caller]
fn assert_aligned(page: &Page, left: &str, right: &str, old: &str, new: &str, least: usize) {
    let (l, r) = (extent(page, left), extent(page, right));
    assert!(
        (l.0 - r.0).abs() < 0.5,
        "the sides are {} and {} points tall",
        l.0,
        r.0
    );
    let (seen, worst) = apart(page, left, right, &same_lines(old, new));
    assert!(seen >= least, "only {seen} lines the same are in view");
    assert!(worst < 0.5, "lines the same stand {worst} points apart");
}

fn layers_of(page: &Page, name: &str) -> Vec<String> {
    page.editor(name)
        .map(|buffer| buffer.layer_names())
        .unwrap_or_default()
}

/// How many decorations the layers the comparison makes have in the editor `name`.
fn compare_decorations(page: &Page, name: &str) -> usize {
    let Some(buffer) = page.editor(name) else {
        return 0;
    };
    let names = buffer.layer_names();
    buffer
        .decorations_in(0, buffer.len_chars())
        .into_iter()
        .filter(|(layer, _)| {
            names
                .get(*layer)
                .is_some_and(|layer| layer.starts_with("compare"))
        })
        .count()
}

/// A file as it is committed, then changed: shown against the commit its two sides are as tall as
/// each other, the lines they have the same stand at the same height, the commit's text is on the left
/// and the file's on the right.
fn stands_side_by_side(size: (f32, f32), least: usize) -> Result<(), Error> {
    let project = changed_project();
    let session = project.session(&["src/server.ts"]);
    let mut page = project.open(&session, size)?;
    let (left, right) = compared(&mut page)?;
    assert_eq!(
        page.editor_text(&left).expect("the left"),
        lantern_server_committed()
    );
    assert_eq!(text(&page), CHANGED);
    assert_aligned(
        &page,
        &left,
        &right,
        lantern_server_committed(),
        CHANGED,
        least,
    );
    // The lines that differ are marked on both sides, and the words in a line that was changed.
    assert!(compare_decorations(&page, &left) > 0);
    assert!(compare_decorations(&page, &right) > 0);
    assert!(
        shows(&page, "Last Commit"),
        "the header: {:?}",
        page_words(&page)
    );
    // The marks of the index and the column that folds are not in the gutter.
    assert!(page.gutter_cell(&right, "changes", 0).is_none());
    assert!(page.gutter_cell(&right, "folds", 0).is_none());
    Ok(())
}

#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_file_and_the_last_commit_stand_side_by_side_with_the_same_lines_at_the_same_height()
-> Result<(), Error> {
    stands_side_by_side(WIDE, 8)
}

#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn an_80x24_pane_lays_the_two_sides_out_as_a_wide_one_does() -> Result<(), Error> {
    stands_side_by_side(PANE, 3)
}

fn lantern_server_committed() -> &'static str {
    "import { createServer } from \"node:http\";\n\nimport { formatPrice } from \"./lib/format.ts\";\n\nconst PORT = Number(process.env.PORT ?? 8080);\n\n/** Answers with JSON. */\nfunction send(res, status: number, body: unknown) {\n  res.end(JSON.stringify(body));\n}\n\nconst server = createServer(async (req, res) => {\n  if (req.method === \"GET\") {\n    return send(res, 200, []);\n  }\n  send(res, 404, { error: `no route for ${req.method}` });\n});\n\nserver.listen(PORT);\n"
}

/// Lines added at the top and lines taken away at the very end, past what one gap holds: a block of
/// 500 lines is as tall on the other side too.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_block_of_five_hundred_lines_is_as_tall_on_the_other_side_and_the_end_of_the_text_lines_up()
-> Result<(), Error> {
    let project = lantern();
    committed(&project);
    let long: String = (0..500)
        .map(|n| format!("export const added{n} = {n};\n"))
        .collect();
    let new = format!(
        "{long}{}",
        lantern_server_committed().replace("server.listen(PORT);\n", "")
    );
    project.write("src/server.ts", &new);
    let session = project.session(&["src/server.ts"]);
    let mut page = project.open(&session, WIDE)?;
    let (left, right) = compared(&mut page)?;
    let (l, r) = (extent(&page, &left), extent(&page, &right));
    assert!((l.0 - r.0).abs() < 0.5, "{} and {} points", l.0, r.0);
    // To the end: where lines were taken away from it, the same lines are at the same height.
    page.wheel(&right, 0.0, 1_000_000.0)?;
    page.advance(0.5)?;
    page.settle()?;
    assert_aligned(&page, &left, &right, lantern_server_committed(), &new, 5);
    assert!(
        extent(&page, &left).1 > 100.0
            && (extent(&page, &left).1 - extent(&page, &right).1).abs() < 0.5
    );
    Ok(())
}

/// The wheel on either side moves both, down and across, and the page comes to rest (the sides telling
/// one another where they were scrolled is no endless round).
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_wheel_on_either_side_scrolls_both_down_and_across_and_the_page_comes_to_rest()
-> Result<(), Error> {
    let project = lantern();
    // A long line both sides have, for them to go across, and lines enough to go down.
    let long = format!(
        "// {}\n",
        "a long line of words to see the sides go across together ".repeat(12)
    );
    let base = format!("{long}{}", lantern_server_committed());
    project.write("src/server.ts", &base);
    committed(&project);
    let tall: String = (0..60).map(|n| format!("// line {n}\n")).collect();
    let new = format!("{}{tall}", base.replace("8080", "3000"));
    project.write("src/server.ts", &new);
    let session = project.session(&["src/server.ts"]);
    let mut page = project.open(&session, WIDE)?;
    let (left, right) = compared(&mut page)?;
    page.wheel(&left, 0.0, 120.0)?;
    page.settle()?;
    let (l, r) = (extent(&page, &left), extent(&page, &right));
    assert!(l.1 > 50.0, "the left was scrolled: {l:?}");
    assert!((l.1 - r.1).abs() < 0.5, "down, from the left: {l:?} {r:?}");
    page.advance(0.3)?;
    page.wheel(&right, 0.0, 60.0)?;
    page.settle()?;
    let (l, r) = (extent(&page, &left), extent(&page, &right));
    assert!(
        l.1 > 100.0 && (l.1 - r.1).abs() < 0.5,
        "down, from the right: {l:?} {r:?}"
    );
    page.advance(0.3)?;
    page.wheel(&right, 80.0, 0.0)?;
    page.settle()?;
    let (l, r) = (extent(&page, &left), extent(&page, &right));
    assert!(r.2 > 20.0, "the right was scrolled across: {r:?}");
    assert!((l.2 - r.2).abs() < 0.5, "across: {l:?} {r:?}");
    page.advance(0.3)?;
    page.wheel(&left, -80.0, -10_000.0)?;
    page.settle()?;
    let (l, r) = (extent(&page, &left), extent(&page, &right));
    assert!(
        l.1 < 0.5 && r.1 < 0.5 && l.2 < 0.5 && r.2 < 0.5,
        "back: {l:?} {r:?}"
    );
    Ok(())
}

/// A letter typed and a line made keep the sides lined up: once the program has what was typed the gaps
/// are made again where the lines are now.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn what_is_typed_in_the_file_keeps_the_sides_lined_up() -> Result<(), Error> {
    let project = changed_project();
    let session = project.session(&["src/server.ts"]);
    let mut page = project.open(&session, WIDE)?;
    let (left, right) = compared(&mut page)?;
    let all = text(&page);
    let at = |needle: &str| {
        all[..all.find(needle).expect("in the text")]
            .chars()
            .count()
    };
    // A letter in a changed line.
    page.focus_editor(&right)?;
    page.caret_to(&right, at("3000") + 1)?;
    page.type_text("x")?;
    page.advance(1.5)?;
    page.settle()?;
    let now = text(&page);
    assert_aligned(&page, &left, &right, lantern_server_committed(), &now, 8);
    // A line made where the lines are the same, and one made in the middle of what was changed.
    let at = |text: &str, needle: &str| {
        text[..text.find(needle).expect("in the text")]
            .chars()
            .count()
    };
    page.caret_to(&right, at(&now, "const server"))?;
    page.type_text("// one more\n")?;
    page.advance(1.5)?;
    page.settle()?;
    let now = text(&page);
    assert_aligned(&page, &left, &right, lantern_server_committed(), &now, 6);
    page.caret_to(&right, at(&now, "const known"))?;
    page.type_text("\n")?;
    page.advance(1.5)?;
    page.settle()?;
    let now = text(&page);
    assert_aligned(&page, &left, &right, lantern_server_committed(), &now, 6);
    // Undo is the file's own.
    page.press("cmd+z")?;
    page.advance(1.5)?;
    page.settle()?;
    assert_aligned(
        &page,
        &left,
        &right,
        lantern_server_committed(),
        &text(&page),
        6,
    );
    Ok(())
}

/// Esc ends it: the left editor and every layer the comparison made are gone, the file's editor is
/// wrapped as the setting says again and its gutter has the marks of the index.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn closing_the_comparison_leaves_the_editor_as_it_was() -> Result<(), Error> {
    let project = changed_project();
    let session = project.session(&["src/server.ts"]);
    let mut page = project.open(&session, WIDE)?;
    page.command("wrap")?;
    page.advance(0.3)?;
    let right = shown(&page).expect("an editor");
    let wrapped = |page: &Page| {
        page.laid()
            .and_then(|laid| {
                laid.editors()
                    .into_iter()
                    .find(|room| room.id == Id::named(&right))
                    .map(|room| room.options.wrap)
            })
            .expect("the editor")
    };
    assert!(wrapped(&page), "the setting wraps the lines");
    wait(&mut page, |page| crate::page_changes::marks(page).len() > 1)?;
    let before = crate::page_changes::marks(&page);
    let (_, right) = compared(&mut page)?;
    assert!(!wrapped(&page), "two columns do not wrap");
    assert!(
        crate::page_changes::marks(&page).is_empty()
            || page.gutter_cell(&right, "changes", 0).is_none()
    );
    page.press("escape")?;
    page.advance(0.5)?;
    page.settle()?;
    assert!(left_of(&page).is_none(), "the left editor is gone");
    assert_eq!(
        compare_decorations(&page, &right),
        0,
        "layers {:?}",
        layers_of(&page, &right)
    );
    assert!(wrapped(&page), "wrapped again");
    wait(&mut page, |page| {
        crate::page_changes::marks(page).len() == before.len()
    })?;
    assert_eq!(crate::page_changes::marks(&page), before);
    // The tab's button shows it, and the same button begins it again.
    page.press("cmd+shift+d")?;
    wait(&mut page, drawn)?;
    page.click("toggle-compare")?;
    page.advance(0.3)?;
    assert!(left_of(&page).is_none());
    Ok(())
}

/// Another tab and back: the comparison is as it was, lined up, with no new question to the program
/// needed for the left text.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn another_tab_and_back_leaves_the_sides_lined_up() -> Result<(), Error> {
    let project = changed_project();
    let session = project.session(&["src/server.ts", "README.md"]);
    let mut page = project.open(&session, WIDE)?;
    let (left, right) = compared(&mut page)?;
    page.press("ctrl+2")?;
    wait(&mut page, |page| text(page).starts_with("# Lantern"))?;
    assert!(left_of(&page).is_none(), "the other file is shown alone");
    page.press("ctrl+1")?;
    wait(&mut page, |page| left_of(page).is_some())?;
    page.advance(0.5)?;
    page.settle()?;
    assert_eq!(left_of(&page).as_deref(), Some(left.as_str()));
    assert_eq!(shown(&page).as_deref(), Some(right.as_str()));
    assert_aligned(&page, &left, &right, lantern_server_committed(), CHANGED, 8);
    Ok(())
}

/// A commit made while it is shown: the left side is the new commit's text, and the header says it.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_commit_made_while_it_is_shown_changes_the_left_side() -> Result<(), Error> {
    let project = changed_project();
    let session = project.session(&["src/server.ts"]);
    let mut page = project.open(&session, WIDE)?;
    let (left, _) = compared(&mut page)?;
    let id_of = |page: &Page| {
        page_words(page)
            .into_iter()
            .find(|word| word.len() == 7 && word.chars().all(|ch| ch.is_ascii_hexdigit()))
    };
    let first = id_of(&page).expect("the commit's id in the header");
    for args in [
        vec!["add", "-A"],
        vec!["commit", "-q", "-m", "Serve on 3000"],
    ] {
        let status = Command::new("git")
            .args(&args)
            .current_dir(project.root())
            .env("GIT_AUTHOR_NAME", "textpen")
            .env("GIT_AUTHOR_EMAIL", "textpen@example.com")
            .env("GIT_COMMITTER_NAME", "textpen")
            .env("GIT_COMMITTER_EMAIL", "textpen@example.com")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .status()
            .expect("git");
        assert!(status.success());
    }
    wait(&mut page, |page| id_of(page).is_some_and(|id| id != first))?;
    wait(&mut page, |page| {
        page.editor_text(&left).as_deref() == Some(CHANGED)
    })?;
    page.advance(0.5)?;
    page.settle()?;
    // Nothing differs now.
    assert!(shows(&page, "No changes"), "{:?}", page_words(&page));
    Ok(())
}

/// Where there is nothing to lay out side by side the page says so in a sentence and has a button that
/// shows the file: a file the commit keeps as binary, one outside a repository, and one with more
/// lines than are compared.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn where_nothing_can_be_laid_out_side_by_side_the_page_says_why() -> Result<(), Error> {
    // Not in a repository.
    let project = lantern();
    let session = project.session(&["src/server.ts"]);
    let mut page = project.open(&session, WIDE)?;
    page.press("cmd+shift+d")?;
    wait(&mut page, |page| {
        shows(page, "not in the project's git repository")
    })?;
    assert!(left_of(&page).is_none());
    page.click("compare-close")?;
    page.advance(0.3)?;
    assert!(!shows(&page, "not in the project's git repository"));
    assert!(shown(&page).is_some(), "the file is shown again");
    // The commit has a binary file where there is text now.
    let project = lantern();
    project.write_bytes("blob.dat", &[0, 1, 2, 0, 3]);
    committed(&project);
    project.write("blob.dat", "text now\n");
    let session = project.session(&["blob.dat"]);
    let mut page = project.open(&session, WIDE)?;
    page.press("cmd+shift+d")?;
    wait(&mut page, |page| shows(page, "is not text"))?;
    // More lines than are compared.
    let old = "x\n".repeat(50_001);
    let new = format!("{}y\n", "x\n".repeat(50_000));
    let project = lantern();
    project.write("long.txt", &old);
    committed(&project);
    project.write("long.txt", &new);
    let session = project.session(&["long.txt"]);
    let mut page = project.open(&session, WIDE)?;
    page.press("cmd+shift+d")?;
    wait(&mut page, |page| shows(page, "too large to compare"))?;
    Ok(())
}

/// The change after and before the caret's, round the file, and what the header counts.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_next_and_the_previous_change_go_round_the_file_and_the_header_counts_them()
-> Result<(), Error> {
    let project = changed_project();
    let session = project.session(&["src/server.ts"]);
    let mut page = project.open(&session, WIDE)?;
    let (_, right) = compared(&mut page)?;
    // The line the caret is on, as the status line says it.
    let line_of = |page: &Page| {
        page_words(page)
            .iter()
            .find_map(|word| word.strip_prefix("Ln "))
            .and_then(|rest| rest.split(',').next())
            .and_then(|line| line.parse::<usize>().ok())
            .expect("the line in the status")
    };
    page.press("alt+f5")?;
    page.advance(0.5)?;
    let first = line_of(&page);
    page.press("alt+f5")?;
    page.advance(0.5)?;
    let second = line_of(&page);
    assert!(second > first, "{first} then {second}");
    page.press("alt+shift+f5")?;
    page.advance(0.5)?;
    assert_eq!(line_of(&page), first);
    assert!(shows(&page, " of "), "{:?}", page_words(&page));
    let _ = right;
    Ok(())
}

// --- The panel of changes -----------------------------------------------------------------------------

/// Runs git in the project.
fn git(project: &Project, args: &[&str]) {
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
}

const REFUNDS: &str =
    "export function refund(orderId: string) {\n  return { orderId, cents: 0 };\n}\n";

/// The project committed, and since: a file changed, one taken away, one renamed (and changed), and one
/// git does not keep.
fn panel_project() -> Project {
    let project = lantern();
    committed(&project);
    project.write("src/server.ts", CHANGED);
    std::fs::remove_file(project.root().join("src/lib/format.ts")).expect("removed");
    git(
        &project,
        &["mv", "src/routes/refunds.ts", "src/routes/returns.ts"],
    );
    project.write(
        "src/routes/returns.ts",
        &REFUNDS.replace("cents: 0", "cents: 100"),
    );
    project.write("notes/new.txt", "a new note\n");
    project
}

fn titles(page: &Page) -> Vec<String> {
    page.menu()
        .expect("a menu")
        .entries
        .iter()
        .map(|entry| match entry {
            MenuEntry::Item { title, .. } | MenuEntry::Paste { title, .. } => title.clone(),
            MenuEntry::Separator => "-".to_owned(),
        })
        .collect()
}

/// Whether the file shown has its editor read-only.
fn read_only(page: &Page, name: &str) -> bool {
    page.laid()
        .and_then(|laid| {
            laid.editors()
                .into_iter()
                .find(|room| room.id == Id::named(name))
                .map(|room| room.options.read_only)
        })
        .expect("the editor")
}

/// The panel lists what git says in two groups; a press on a row shows that file beside the last
/// commit: a file changed, one taken away (nothing on the right, which cannot be changed, and no word
/// that it was deleted), one renamed (the commit's is read under its old name) and one git does not
/// keep (nothing on the left).
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_panel_lists_the_changed_files_in_two_groups_and_a_press_shows_each_beside_the_last_commit()
-> Result<(), Error> {
    let project = panel_project();
    let session = project.session(&["README.md"]);
    let mut page = project.open(&session, WIDE)?;
    page.press("cmd+shift+g")?;
    wait(&mut page, |page| shows(page, "Unversioned Files"))?;
    assert!(shows(&page, "Changes"), "{:?}", page_words(&page));
    for row in [
        "change:src/server.ts",
        "change:src/lib/format.ts",
        "change:src/routes/returns.ts",
        "change:notes/new.txt",
    ] {
        assert!(page.bounds(row).is_some(), "{row} is listed");
    }
    assert!(page.bounds("change:README.md").is_none());
    // Changed.
    page.click("change:src/server.ts")?;
    wait(&mut page, drawn)?;
    page.advance(0.5)?;
    let left = left_of(&page).expect("the left");
    assert_eq!(
        page.editor_text(&left).as_deref(),
        Some(lantern_server_committed())
    );
    assert_eq!(text(&page), CHANGED);
    // The keys stay in the panel.
    assert!(matches!(page.focus(), Focus::Page), "{:?}", page.focus());
    // Taken away: nothing on the right, which is read-only, and the left is what the commit has.
    page.click("change:src/lib/format.ts")?;
    wait(&mut page, |page| {
        drawn(page) && page_words(page).iter().any(|word| word == "format.ts")
    })?;
    page.advance(0.5)?;
    page.settle()?;
    let (left, right) = (
        left_of(&page).expect("a left"),
        shown(&page).expect("a right"),
    );
    assert_eq!(text(&page), "");
    assert!(
        page.editor_text(&left)
            .is_some_and(|text| text.contains("formatPrice"))
    );
    assert!(
        read_only(&page, &right),
        "a file that is not on disk is not changed"
    );
    assert!(!shows(&page, "deleted on disk"), "{:?}", page_words(&page));
    let (l, r) = (extent(&page, &left), extent(&page, &right));
    assert!((l.0 - r.0).abs() < 0.5, "{} and {} points", l.0, r.0);
    // Saving makes no file.
    page.focus_editor(&right)?;
    page.press("cmd+s")?;
    page.advance(1.0)?;
    assert!(!project.exists("src/lib/format.ts"));
    // Renamed.
    page.click("change:src/routes/returns.ts")?;
    wait(&mut page, |page| {
        text(page).contains("cents: 100") && drawn(page)
    })?;
    page.advance(0.5)?;
    let left = left_of(&page).expect("a left");
    assert_eq!(page.editor_text(&left).as_deref(), Some(REFUNDS));
    // Not kept by git.
    page.click("change:notes/new.txt")?;
    wait(&mut page, |page| {
        text(page) == "a new note\n" && drawn(page)
    })?;
    page.advance(0.5)?;
    let left = left_of(&page).expect("a left");
    assert_eq!(page.editor_text(&left).as_deref(), Some(""));
    Ok(())
}

/// The keys go through the panel's files, each shown beside the last commit as it is come to; the one
/// the panel opened last is closed as the next comes, ⏎ goes into the editor, and Esc there ends the
/// comparison.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_arrows_go_through_the_changed_files_and_enter_goes_into_the_editor() -> Result<(), Error> {
    let project = panel_project();
    let session = project.session(&["README.md"]);
    let mut page = project.open(&session, WIDE)?;
    page.press("cmd+shift+g")?;
    wait(&mut page, |page| shows(page, "Unversioned Files"))?;
    page.click("change:src/lib/format.ts")?;
    wait(&mut page, |page| {
        page.bounds("tab:src/lib/format.ts").is_some() && drawn(page)
    })?;
    page.press("down")?;
    wait(&mut page, |page| {
        text(page).contains("cents: 100") && drawn(page)
    })?;
    // What was opened on the way is not left behind; what was open before is.
    wait(&mut page, |page| {
        page.bounds("tab:src/lib/format.ts").is_none()
    })?;
    assert!(page.bounds("tab:README.md").is_some());
    page.press("down")?;
    wait(&mut page, |page| text(page) == CHANGED && drawn(page))?;
    page.press("down")?;
    wait(&mut page, |page| {
        text(page) == "a new note\n" && drawn(page)
    })?;
    page.press("down")?;
    page.press("up")?;
    wait(&mut page, |page| text(page) == CHANGED && drawn(page))?;
    // ⏎: the keys are in the editor, and what is typed is the file's.
    page.press("enter")?;
    assert!(
        matches!(page.focus(), Focus::Editor(_)),
        "{:?}",
        page.focus()
    );
    page.type_text("x")?;
    assert_ne!(text(&page), CHANGED);
    page.press("cmd+z")?;
    page.press("escape")?;
    page.advance(0.5)?;
    assert!(left_of(&page).is_none(), "Esc ends the comparison");
    Ok(())
}

/// Nothing changed, and not a repository, are said in words; of thousands of files only those in view
/// have elements.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_panel_says_when_there_is_nothing_and_makes_only_the_rows_in_view() -> Result<(), Error> {
    let project = lantern();
    committed(&project);
    let session = project.session(&["README.md"]);
    let mut page = project.open(&session, WIDE)?;
    page.press("cmd+shift+g")?;
    wait(&mut page, |page| shows(page, "No changes"))?;
    let project = lantern();
    let session = project.session(&["README.md"]);
    let mut page = project.open(&session, WIDE)?;
    page.press("cmd+shift+g")?;
    wait(&mut page, |page| shows(page, "Not a git repository"))?;
    // Thousands of files git does not keep.
    let project = lantern();
    committed(&project);
    for n in 0..3000 {
        project.write(&format!("many/f{n:04}.txt"), "x\n");
    }
    let session = project.session(&["README.md"]);
    let mut page = project.open(&session, WIDE)?;
    page.press("cmd+shift+g")?;
    wait(&mut page, |page| {
        page.bounds("change:many/f0000.txt").is_some()
    })?;
    assert!(page.bounds("change:many/f2999.txt").is_none());
    let rows = (0..3000)
        .filter(|n| page.bounds(&format!("change:many/f{n:04}.txt")).is_some())
        .count();
    assert!(rows > 5 && rows < 300, "{rows} rows are made");
    Ok(())
}

/// In an 80×24 pane the panel is a drawer, and a press on a row closes it on the comparison. The menus
/// of a row, a tab and a file in the tree have the comparison too.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn in_a_narrow_pane_the_panel_is_a_drawer_that_closes_on_the_comparison_and_the_menus_offer_it()
-> Result<(), Error> {
    let project = panel_project();
    let session = project.session(&["src/server.ts", "README.md"]);
    let mut page = project.open(&session, PANE)?;
    page.press("cmd+shift+g")?;
    wait(&mut page, |page| shows(page, "Unversioned Files"))?;
    page.click("change:src/routes/returns.ts")?;
    wait(&mut page, |page| {
        text(page).contains("cents: 100") && drawn(page)
    })?;
    page.advance(0.5)?;
    assert!(!shows(&page, "Unversioned Files"), "the drawer closed");
    // The menu of a row.
    let mut page = project.open(&session, WIDE)?;
    page.press("cmd+shift+g")?;
    wait(&mut page, |page| shows(page, "Unversioned Files"))?;
    page.right_click("change:src/server.ts")?;
    assert_eq!(
        titles(&page),
        ["Open File", "Reveal in Files", "-", "Copy Path"]
    );
    page.press("escape")?;
    // The menu of a tab and of a file of the tree.
    page.right_click("tab:README.md")?;
    assert!(
        titles(&page)
            .iter()
            .any(|title| title == "Compare with Last Commit")
    );
    page.press("escape")?;
    page.press("cmd+shift+e")?;
    wait(&mut page, |page| page.bounds("row:README.md").is_some())?;
    page.right_click("row:README.md")?;
    assert!(
        titles(&page)
            .iter()
            .any(|title| title == "Compare with Last Commit")
    );
    page.press("escape")?;
    // The menu of the editor offers it, and says so when it is on.
    let editor = shown(&page).expect("an editor");
    page.right_click(&editor)?;
    assert!(
        titles(&page)
            .iter()
            .any(|title| title == "Compare with Last Commit"),
        "{:?}",
        titles(&page)
    );
    page.press("escape")?;
    Ok(())
}
