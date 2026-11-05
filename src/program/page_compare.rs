//! A file side by side with what the last commit keeps of it, as the page lays it out and what the
//! person does in it: the commit's text on the left and the file's own editor on the right, the same
//! lines at the same height (a gap on the side with fewer lines), the two scrolled together, the
//! lines and words that differ in colour, and the way out. The page and the program's own handlers
//! in one process, as `page_tests` runs them, on a project that is a git repository.
//!
//! Each opens the interpreter (compiled once a run): `make test-page` runs them, in `make check`.

use std::process::Command;

use pencere_drive::headless::{Error, Focus};
use pencere_view::{Id, MenuEntry, Paint};
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
    assert_header_stands_on_the_sides(&page, &left);
    Ok(())
}

/// The rule between the two halves of the header is on the rule between the two sides, whatever each
/// half holds.
fn assert_header_stands_on_the_sides(page: &Page, left: &str) {
    let half = page.bounds("compare-commit").expect("the header");
    let side = page.bounds(left).expect("the left side");
    assert!(
        (half.x + half.w - (side.x + side.w)).abs() < 1.0,
        "the header's half {half:?}, the left side {side:?}"
    );
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

/// A long file with six lines added in it and, further on, six taken away, committed before they were.
fn a_long_file_changed_far_down(project: &Project) {
    let lines: Vec<String> = (0..600)
        .map(|n| format!("export const line{n} = {n};\n"))
        .collect();
    project.write("src/long.ts", &lines.concat());
    committed(project);
    let mut changed = lines;
    changed.drain(340..346);
    for n in 0..6 {
        changed.insert(300, format!("export const added{n} = {n};\n"));
    }
    project.write("src/long.ts", &changed.concat());
}

/// A tall gap is hatched in pieces that meet and fill what shows, and scrolling it brings the same
/// few pictures back: a host that holds a picture does not draw it again, and a tall one takes tens of
/// milliseconds, so that a new height at every turn of the wheel is what makes scrolling stutter.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_tall_gap_is_the_same_few_pictures_however_far_it_is_scrolled() -> Result<(), Error> {
    let project = lantern();
    project.write("src/long.ts", "export const first = 1;\n");
    committed(&project);
    let lines: String = (0..1500)
        .map(|n| format!("export const line{n} = {n};\n"))
        .collect();
    project.write("src/long.ts", &format!("export const first = 1;\n{lines}"));
    let session = project.session(&["src/long.ts"]);
    let mut page = project.open(&session, WIDE)?;
    let (left, right) = compared(&mut page)?;
    let room = page.bounds(&left).expect("the left side's box");
    let mut pictures: Vec<(f32, Vec<pencere_view::Path>)> = Vec::new();
    for turn in 0..60 {
        page.wheel(&right, 0.0, 180.0)?;
        page.advance(0.2)?;
        page.settle()?;
        let list = page.list();
        let mut pieces: Vec<_> = list
            .items
            .iter()
            .filter_map(|item| match item {
                Paint::Figure(figure) if figure.rect.x < room.x + room.w => Some(figure),
                _ => None,
            })
            .collect();
        pieces.sort_by(|a, b| a.rect.y.total_cmp(&b.rect.y));
        let first = pieces.first().expect("a piece");
        let last = pieces.last().expect("a piece");
        assert!(
            first.rect.y <= room.y + 1.0 && last.rect.y + last.rect.h >= room.y + room.h - 1.0,
            "turn {turn}: {} to {} does not fill {room:?}",
            first.rect.y,
            last.rect.y + last.rect.h
        );
        for pair in pieces.windows(2) {
            let (above, below) = (pair[0], pair[1]);
            assert!(
                (above.rect.y + above.rect.h - below.rect.y).abs() < 0.5,
                "turn {turn}: {:?} and {:?} do not meet",
                above.rect,
                below.rect
            );
        }
        for piece in pieces {
            if !pictures
                .iter()
                .any(|(h, paths)| (*h - piece.rect.h).abs() < 0.01 && *paths == piece.paths)
            {
                pictures.push((piece.rect.h, piece.paths.clone()));
            }
        }
    }
    // A piece, and what is left of a gap's parts at their ends.
    assert!(pictures.len() <= 3, "{} pictures", pictures.len());
    Ok(())
}

/// The hatching of a gap shows wherever the sides were scrolled to, on the side the wheel is on and on
/// the one that follows it, and not only at the top of a file, where a picture of it would be taken.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_gaps_are_hatched_on_both_sides_far_down_a_file() -> Result<(), Error> {
    let project = lantern();
    a_long_file_changed_far_down(&project);
    let session = project.session(&["src/long.ts"]);
    let mut page = project.open(&session, WIDE)?;
    let (left, right) = compared(&mut page)?;
    page.press("ctrl+alt+]")?;
    page.advance(0.5)?;
    page.settle()?;
    let row = extent(&page, &right).0 / 600.0;
    page.wheel(&right, 0.0, 300.0 * row)?;
    page.advance(0.5)?;
    page.settle()?;
    for name in [&left, &right] {
        let room = page.bounds(name).expect("the side's box");
        let list = page.list();
        let hatched: Vec<_> = list
            .items
            .iter()
            .filter_map(|item| match item {
                Paint::Figure(figure)
                    if figure.rect.x < room.x + room.w
                        && figure.rect.x + figure.rect.w > room.x
                        && figure.rect.y < room.y + room.h
                        && figure.rect.y + figure.rect.h > room.y =>
                {
                    Some(figure.clip.intersect(figure.rect))
                }
                _ => None,
            })
            .collect();
        assert!(!hatched.is_empty(), "no hatching on {name}");
        for shows in hatched {
            assert!(
                shows.w > 0.0 && shows.h > 0.0,
                "the hatching on {name} is cut away: {shows:?}"
            );
        }
    }
    Ok(())
}

/// Each side's bands stand where their changes do: lines added and then a line changed start at the same
/// place of the left side, and the second band is below the first one's gap, on the lines it colours.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_band_stands_on_its_lines_where_two_changes_start_at_the_same_place() -> Result<(), Error> {
    let project = lantern();
    let lines: Vec<String> = (0..40)
        .map(|n| format!("export const line{n} = {n};\n"))
        .collect();
    let old = lines.concat();
    project.write("src/long.ts", &old);
    committed(&project);
    let mut changed = lines;
    changed[14] =
        "export const first = 1;\nexport const second = 2;\nexport const third = 3;\n".to_owned();
    project.write("src/long.ts", &changed.concat());
    let session = project.session(&["src/long.ts"]);
    let mut page = project.open(&session, WIDE)?;
    let (left, right) = compared(&mut page)?;
    assert_aligned(&page, &left, &right, &old, &changed.concat(), 5);
    let line = page
        .gutter_cell(&left, "numbers", 14)
        .expect("the line changed, in view");
    let room = page.bounds(&left).expect("the side");
    let bands: Vec<_> = page
        .list()
        .items
        .iter()
        .filter_map(|item| match item {
            Paint::Quad(quad)
                if quad.fill.is_some()
                    && quad.rect.x == room.x
                    && quad.rect.w < 100.0
                    && (quad.rect.h - line.h).abs() < 0.5 =>
            {
                Some(quad.rect.y)
            }
            _ => None,
        })
        .collect();
    assert!(
        bands.iter().any(|y| (y - line.y).abs() < 0.5),
        "no band on line 14 at {}: {bands:?}",
        line.y
    );
    Ok(())
}

/// A gap taller than a canvas should be is hatched too, where it shows.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_tall_gap_is_hatched_where_it_shows() -> Result<(), Error> {
    let project = lantern();
    let lines: Vec<String> = (0..300)
        .map(|n| format!("export const line{n} = {n};\n"))
        .collect();
    project.write("src/long.ts", &lines.concat());
    committed(&project);
    let mut changed = lines;
    for n in 0..150 {
        changed.insert(100, format!("export const added{n} = {n};\n"));
    }
    project.write("src/long.ts", &changed.concat());
    let session = project.session(&["src/long.ts"]);
    let mut page = project.open(&session, WIDE)?;
    let (left, right) = compared(&mut page)?;
    page.press("ctrl+alt+]")?;
    page.advance(0.5)?;
    page.settle()?;
    let height = extent(&page, &right).0;
    page.wheel(&right, 0.0, height / 3.0)?;
    page.advance(0.5)?;
    page.settle()?;
    let room = page.bounds(&left).expect("the side");
    // Its pieces, in order, from the top of the room down with nothing between them.
    let mut pieces: Vec<_> = page
        .list()
        .items
        .iter()
        .filter_map(|item| match item {
            Paint::Figure(figure) if figure.rect.x <= room.x + 1.0 && figure.rect.w > 100.0 => {
                Some(figure.clip.intersect(figure.rect))
            }
            _ => None,
        })
        .filter(|cut| cut.h > 0.0)
        .collect();
    pieces.sort_by(|a, b| a.y.total_cmp(&b.y));
    let mut reached = room.y + 1.0;
    for cut in pieces {
        if cut.y <= reached {
            reached = reached.max(cut.y + cut.h);
        }
    }
    assert!(
        reached >= room.y + room.h - 1.0,
        "the left side's gap, all of the room, is not hatched: {reached} of {room:?}"
    );
    Ok(())
}

/// A file that only moved says so: it is on the list of changes, so "No changes" would read as a mistake.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_file_that_only_moved_says_so_and_not_that_nothing_changed() -> Result<(), Error> {
    let project = lantern();
    committed(&project);
    git(
        &project,
        &["mv", "src/lib/format.ts", "src/lib/formatting.ts"],
    );
    let session = project.session(&["src/lib/formatting.ts"]);
    let mut page = project.open(&session, WIDE)?;
    compared(&mut page)?;
    wait(&mut page, |page| shows(page, "Renamed, same text"))?;
    assert!(!shows(&page, "No changes"), "{:?}", page_words(&page));
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

/// The panel stands its files under their folders at its button or T and back: a press on a file does
/// what it does in the list, a folder folds at a press and at the keys of the tree, and the next page
/// finds the panel as it was left.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_panel_stands_its_files_under_folders_that_fold() -> Result<(), Error> {
    let project = panel_project();
    let session = project.session(&["README.md"]);
    let mut page = project.open(&session, WIDE)?;
    page.press("cmd+shift+g")?;
    wait(&mut page, |page| shows(page, "Unversioned Files"))?;
    const SRC: &str = "change-dir:dir:Changes:src";
    const LIB: &str = "change-dir:dir:Changes:src/lib";
    const FORMAT: &str = "change:src/lib/format.ts";
    assert!(page.bounds(SRC).is_none(), "a list to begin with");
    page.click("changes-tree")?;
    for row in [
        SRC,
        LIB,
        "change-dir:dir:Changes:src/routes",
        "change-dir:dir:Unversioned Files:notes",
        FORMAT,
        "change:src/server.ts",
        "change:src/routes/returns.ts",
        "change:notes/new.txt",
    ] {
        assert!(page.bounds(row).is_some(), "{row} is listed");
    }
    // Folders before the files beside them, and each row further in than its folder.
    let top = |page: &Page, row: &str| page.bounds(row).expect(row).y;
    assert!(top(&page, LIB) < top(&page, "change:src/server.ts"));
    let (lib, format) = (
        page.bounds(LIB).expect("lib"),
        page.bounds(FORMAT).expect("file"),
    );
    assert!(lib.y < format.y);
    assert!(!shows(&page, "src/lib"), "{:?}", page_words(&page));
    // A file shows as it does in the list.
    page.click("change:src/server.ts")?;
    wait(&mut page, |page| {
        page.bounds("compare-count").is_some() || shows(page, "1 of")
    })?;
    // A press folds a folder and unfolds it; the rows of the other folders stay.
    page.click(LIB)?;
    assert!(page.bounds(FORMAT).is_none());
    assert!(page.bounds("change:src/server.ts").is_some());
    page.click(LIB)?;
    assert!(page.bounds(FORMAT).is_some());
    // The keys of the tree: h to the folder, h folds it, l unfolds it, l goes in.
    page.click("change:src/server.ts")?;
    page.click(FORMAT)?;
    page.press("h")?;
    assert!(page.bounds(FORMAT).is_some(), "only chosen the folder");
    page.press("h")?;
    assert!(page.bounds(FORMAT).is_none(), "folded");
    page.press("l")?;
    assert!(page.bounds(FORMAT).is_some(), "unfolded");
    page.press("t")?;
    assert!(page.bounds(SRC).is_none(), "a list again");
    assert!(page.bounds(FORMAT).is_some());
    page.press("t")?;
    assert!(page.bounds(SRC).is_some());
    // The next page has it as it was left.
    page.advance(2.5)?;
    page.settle()?;
    drop(page);
    let mut again = project.open(&session, WIDE)?;
    again.press("cmd+shift+g")?;
    wait(&mut again, |page| shows(page, "Unversioned Files"))?;
    assert!(again.bounds(SRC).is_some());
    Ok(())
}

/// A file says how many lines it adds and takes away, and a group and a folder those of what is in them;
/// what a row has none of is not said.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_panel_says_how_many_lines_each_change_adds_and_takes_away() -> Result<(), Error> {
    let project = lantern();
    committed(&project);
    let readme = project.read("README.md");
    project.write("README.md", &format!("{readme}one more\n"));
    let format = project.read("src/lib/format.ts");
    let lines = format.lines().count();
    std::fs::remove_file(project.root().join("src/lib/format.ts")).expect("removed");
    project.write("notes/new.txt", "a\nb\nc\n");
    let session = project.session(&[]);
    let mut page = project.open(&session, WIDE)?;
    page.press("cmd+shift+g")?;
    wait(&mut page, |page| shows(page, "Unversioned Files"))?;
    let words = page_words(&page);
    let count = |word: &str| words.iter().filter(|said| *said == word).count();
    let taken = format!("−{lines}");
    // Each file's and its group's.
    assert_eq!(count("+1"), 2, "{words:?}");
    assert_eq!(count(&taken), 2, "{words:?}");
    assert_eq!(count("+3"), 2, "{words:?}");
    // Under their folders: each folder on the way says it too.
    page.press("t")?;
    let words = page_words(&page);
    let count = |word: &str| words.iter().filter(|said| *said == word).count();
    assert_eq!(
        count(&taken),
        4,
        "the file, lib, src and the group: {words:?}"
    );
    assert_eq!(count("+3"), 3, "the file, notes and the group: {words:?}");
    Ok(())
}

/// `root/dir` a repository of its own, on the branch `branch`, with what is in it committed.
fn repository_at(project: &Project, dir: &str, branch: &str) {
    let folder = project.root().join(dir);
    let in_it = |args: &[&str]| {
        let status = Command::new("git")
            .args(args)
            .current_dir(&folder)
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
    in_it(&["init", "-q", "-b", branch]);
    in_it(&["add", "."]);
    in_it(&["commit", "-qm", "one"]);
}

/// A project in no repository, with three in its folders: one with changes and a file git does not keep,
/// one with a change, one with nothing to say, and a folder that is none.
fn project_of_repositories() -> Project {
    let project = lantern();
    project.write("alpha/one.txt", "a\nb\n");
    project.write("alpha/deep/two.txt", "x\n");
    project.write("alpha/.gitignore", "out/\n");
    project.write("beta/three.txt", "a\n");
    project.write("calm/four.txt", "a\n");
    project.write("loose/five.txt", "a\n");
    repository_at(&project, "alpha", "main");
    repository_at(&project, "beta", "other");
    repository_at(&project, "calm", "main");
    project.write("alpha/one.txt", "a\nb\nc\n");
    project.write("alpha/new.txt", "n\n");
    project.write("alpha/out/made.txt", "m\n");
    project.write("beta/three.txt", "a\nb\n");
    project
}

/// The panel has each repository with something to say, its branch and its lines, and its files under it;
/// a press shows a file beside the last commit of its own repository; a repository folds as a folder does,
/// and the keys go through; those with nothing to say are one line. The status line has the branch of the
/// file shown, the tree has each repository's and what it ignores, and the quick open has what git lists.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_project_of_repositories_has_each_in_the_panel_and_the_tree() -> Result<(), Error> {
    const ALPHA: &str = "change-dir:repo:alpha";
    const BETA: &str = "change-dir:repo:beta";
    const ONE: &str = "change:alpha/one.txt";
    const THREE: &str = "change:beta/three.txt";
    let project = project_of_repositories();
    let session = project.session(&[]);
    let mut page = project.open(&session, WIDE)?;
    page.press("cmd+shift+g")?;
    wait(&mut page, |page| page.bounds(ALPHA).is_some())?;
    for row in [ALPHA, BETA, ONE, "change:alpha/new.txt", THREE] {
        assert!(page.bounds(row).is_some(), "{row} is listed");
    }
    assert!(page.bounds("change-dir:repo:calm").is_none());
    let words = page_words(&page);
    assert!(
        words.iter().any(|said| said == "1 repo with no changes"),
        "{words:?}"
    );
    assert!(words.iter().any(|said| said == "3 repos"), "{words:?}");
    assert!(words.iter().any(|said| said == "other"), "{words:?}");
    assert!(!shows(&page, "Unversioned Files"), "{words:?}");
    // What git keeps comes before what it does not, each below its repository, further in.
    let top = |page: &Page, row: &str| page.bounds(row).expect(row).y;
    assert!(top(&page, ALPHA) < top(&page, ONE));
    assert!(top(&page, ONE) < top(&page, "change:alpha/new.txt"));
    assert!(top(&page, "change:alpha/new.txt") < top(&page, BETA));
    // A file is shown beside its own repository's last commit; the status line has its branch.
    page.click(ONE)?;
    wait(&mut page, |page| shows(page, "Last Commit"))?;
    assert_eq!(status_branch(&page), "main");
    page.click(THREE)?;
    wait(&mut page, |page| status_branch(page) == "other")?;
    // A repository folds at a press and by the keys, and the keys go through its rows.
    page.click(ALPHA)?;
    assert!(page.bounds(ONE).is_none());
    assert!(page.bounds(BETA).is_some());
    page.click(ALPHA)?;
    assert!(page.bounds(ONE).is_some());
    page.click(ONE)?;
    wait(&mut page, |page| status_branch(page) == "main")?;
    page.press("h")?;
    assert!(page.bounds(ONE).is_some(), "only chosen the repository");
    page.press("h")?;
    assert!(page.bounds(ONE).is_none(), "folded");
    page.press("l")?;
    page.press("j")?;
    wait(&mut page, |page| {
        page.bounds(ONE).is_some() && shows(page, "Last Commit")
    })?;
    // Under their folders, inside the repository.
    page.press("t")?;
    assert!(page.bounds("change-dir:repo:alpha").is_some());
    page.press("t")?;
    // The tree: each repository's branch beside its folder, and what it ignores quiet.
    page.press("cmd+shift+e")?;
    wait(&mut page, |page| page.bounds("row:alpha").is_some())?;
    let words = page_words(&page);
    let beside = words.iter().position(|said| said == "beta").expect("beta");
    assert_eq!(words[beside + 1], "other", "{words:?}");
    // The quick open: what the repository's git lists, and what is in none.
    page.press("cmd+k")?;
    page.fill("palette-query", "made")?;
    assert!(!shows(&page, "made.txt"), "{:?}", page_words(&page));
    page.fill("palette-query", "five")?;
    wait(&mut page, |page| shows(page, "five.txt"))?;
    page.press("escape")?;
    Ok(())
}

/// The words of the status line's branch: the word before what the status line has of the file.
fn status_branch(page: &Page) -> String {
    let words = page_words(page);
    let at = words
        .iter()
        .rposition(|said| said.starts_with("Ln "))
        .unwrap_or(words.len());
    words[..at]
        .iter()
        .rev()
        .find(|said| matches!(said.as_str(), "main" | "other"))
        .cloned()
        .unwrap_or_default()
}

/// A project of repositories with nothing to say says so, and how many there are.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_project_of_repositories_with_nothing_to_say_says_so() -> Result<(), Error> {
    let project = lantern();
    project.write("alpha/one.txt", "a\n");
    repository_at(&project, "alpha", "main");
    project.write("beta/one.txt", "a\n");
    repository_at(&project, "beta", "main");
    let session = project.session(&[]);
    let mut page = project.open(&session, WIDE)?;
    page.press("cmd+shift+g")?;
    wait(&mut page, |page| shows(page, "No changes"))?;
    assert!(shows(&page, "2 repos"), "{:?}", page_words(&page));
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

// --- Folds, and taking a change back -------------------------------------------------------------------

/// Eighty lines, as they are committed, and as they are when the page opens them: one changed, two taken
/// away and two added, so that what is the same is long stretches between.
fn long_old() -> String {
    (0..80).map(|n| format!("const v{n} = {n};\n")).collect()
}

fn long_new() -> String {
    let mut out = String::new();
    for n in 0..80 {
        match n {
            10 => out.push_str("const v10 = 1000;\n"),
            40 | 41 => {}
            _ => out.push_str(&format!("const v{n} = {n};\n")),
        }
        if n == 70 {
            out.push_str("// added a\n// added b\n");
        }
    }
    out
}

fn long_project() -> Project {
    let project = lantern();
    project.write("src/long.ts", &long_old());
    committed(&project);
    project.write("src/long.ts", &long_new());
    project
}

/// The lines (from 0) the right editor has a marker of its column `apply` on whose id starts with
/// `kind`.
fn markers(page: &Page, kind: &str) -> Vec<usize> {
    use pencere_view::edit::Mark;
    let name = shown(page).expect("an editor");
    let buffer = page.editor(&name).expect("the editor");
    let mut found: Vec<usize> = buffer
        .decorations_in(0, buffer.len_chars())
        .into_iter()
        .filter_map(|(_, decoration)| match &decoration.mark {
            Mark::Gutter(mark)
                if mark.column == "apply"
                    && mark.id.as_deref().is_some_and(|id| id.starts_with(kind)) =>
            {
                Some(buffer.line_of(decoration.from))
            }
            _ => None,
        })
        .collect();
    found.sort_unstable();
    found
}

/// The unchanged stretches are folded on both sides alike, so that the two are as tall as each other
/// and the same lines are at the same height; a press on the chevron of one opens it on both; the page's
/// own keys fold them all and show them all; and the next change is never one folded away.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn what_is_the_same_is_folded_on_both_sides_alike_and_a_stretch_opens_on_both_together()
-> Result<(), Error> {
    use pencere_view::Button;
    let project = long_project();
    let session = project.session(&["src/long.ts"]);
    let mut page = project.open(&session, WIDE)?;
    let (left, right) = compared(&mut page)?;
    let (old, new) = (long_old(), long_new());
    let folded = extent(&page, &right).0;
    assert!(
        folded < 45.0 * 21.0,
        "{folded} points: far from the 90 lines there are"
    );
    // One more button in the header than where nothing folds, and its rule has not moved.
    assert_header_stands_on_the_sides(&page, &left);
    assert_aligned(&page, &left, &right, &old, &new, 4);
    // The first line of a stretch is in view; the lines it hides stand where it does.
    let y = |name: &str, line: usize| page.gutter_cell(name, "numbers", line).map(|cell| cell.y);
    assert!(y(&right, 13).is_some());
    assert_eq!(y(&right, 20), y(&right, 14), "folded away");
    assert_eq!(y(&left, 20), y(&left, 14), "folded away");
    // Each says how many lines it hides, at the end of the line it folds from.
    for name in [&left, &right] {
        let buffer = page.editor(name).expect("the editor");
        let widgets = buffer
            .decorations_in(0, buffer.len_chars())
            .into_iter()
            .filter(|(_, decoration)| {
                matches!(decoration.mark, pencere_view::edit::Mark::Widget(_))
            })
            .count();
        assert_eq!(widgets, 4, "{name}: a note for each stretch");
    }
    // The chevron of the stretch opens it on both sides, and the other stretches stay folded.
    let opens = markers(&page, "open:");
    assert!(opens.contains(&13), "{opens:?}");
    page.click_gutter(&right, "apply", 13, Button::Left)?;
    page.advance(0.5)?;
    page.settle()?;
    let (l, r) = (extent(&page, &left), extent(&page, &right));
    assert!((l.0 - r.0).abs() < 0.5, "{} and {} points", l.0, r.0);
    assert!(r.0 > folded + 20.0 * 21.0, "{} points after {folded}", r.0);
    let y = |name: &str, line: usize| page.gutter_cell(name, "numbers", line).map(|cell| cell.y);
    assert_ne!(y(&right, 20), y(&right, 14), "open");
    assert_ne!(y(&left, 20), y(&left, 14), "open");
    assert_aligned(&page, &left, &right, &old, &new, 8);
    // Fold them all again (a key of the page's: the editor's own would fold one side), then show them all.
    page.focus_editor(&right)?;
    page.press("cmd+alt+[")?;
    page.advance(0.5)?;
    page.settle()?;
    let again = extent(&page, &right).0;
    assert!((again - folded).abs() < 0.5, "{again} and {folded}");
    assert_aligned(&page, &left, &right, &old, &new, 4);
    page.press("ctrl+alt+]")?;
    page.advance(0.5)?;
    page.settle()?;
    let (l, r) = (extent(&page, &left), extent(&page, &right));
    assert!(
        (l.0 - r.0).abs() < 0.5 && r.0 > 80.0 * 20.0,
        "{} and {}",
        l.0,
        r.0
    );
    page.press("cmd+alt+[")?;
    page.advance(0.5)?;
    page.settle()?;
    // The next change: where it goes is in view.
    page.caret_to(&right, 0)?;
    let line_of = |page: &Page| {
        page_words(page)
            .iter()
            .find_map(|word| word.strip_prefix("Ln "))
            .and_then(|rest| rest.split(',').next())
            .and_then(|line| line.parse::<usize>().ok())
            .expect("the line in the status")
    };
    for _ in 0..3 {
        page.press("alt+f5")?;
        page.advance(0.5)?;
        page.settle()?;
        let line = line_of(&page);
        assert!(
            page.gutter_cell(&right, "numbers", line - 1).is_some(),
            "line {line} is in view"
        );
    }
    Ok(())
}

/// A change is taken back with the chevron at its first line: a line changed, lines taken away and lines
/// added, at the top, in the middle and at the end of the text, as one edit that is undone with ⌘Z, the
/// two sides lined up again at once.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_change_is_taken_back_with_its_chevron_as_one_edit_that_can_be_undone() -> Result<(), Error> {
    use pencere_view::Button;
    let project = changed_project();
    let session = project.session(&["src/server.ts"]);
    let mut page = project.open(&session, WIDE)?;
    let (left, right) = compared(&mut page)?;
    let lines = markers(&page, "apply:");
    assert_eq!(lines.len(), 4, "{lines:?}: a chevron for each change");
    // The first: the two lines added at the top.
    page.click_gutter(&right, "apply", lines[0], Button::Left)?;
    wait(&mut page, |page| !text(page).starts_with("#!"))?;
    assert_eq!(
        text(&page),
        CHANGED
            .split_once("import")
            .map(|(_, rest)| format!("import{rest}"))
            .unwrap()
    );
    wait(&mut page, |page| markers(page, "apply:").len() == 3)?;
    page.advance(0.5)?;
    page.settle()?;
    assert_aligned(
        &page,
        &left,
        &right,
        lantern_server_committed(),
        &text(&page),
        4,
    );
    // One ⌘Z brings it back, and the sides are lined up with it.
    page.press("cmd+z")?;
    wait(&mut page, |page| {
        text(page) == CHANGED && markers(page, "apply:").len() == 4
    })?;
    page.advance(0.5)?;
    page.settle()?;
    assert_aligned(&page, &left, &right, lantern_server_committed(), CHANGED, 6);
    // All of them, from the end of the text back: the file is as it was committed.
    for _ in 0..4 {
        let last = *markers(&page, "apply:").last().expect("a chevron");
        let before = text(&page);
        page.click_gutter(&right, "apply", last, Button::Left)?;
        wait(&mut page, |page| text(page) != before)?;
        wait(&mut page, |page| drawn(page) && !shows(page, "Revert"))?;
        page.advance(0.5)?;
        page.settle()?;
    }
    assert_eq!(text(&page), lantern_server_committed());
    wait(&mut page, |page| shows(page, "No changes"))?;
    Ok(())
}

/// The ends of a text, with a line break at its end and without: lines added at the end, lines taken away
/// from it and the last line changed are taken back to what the commit has, to the byte.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_change_at_the_end_of_the_text_is_taken_back_to_the_byte() -> Result<(), Error> {
    use pencere_view::Button;
    for (old, new) in [
        ("a\nb\nc\n", "a\nb\nc\nd\ne\n"),
        ("a\nb\nc\nd\n", "a\nb\n"),
        ("a\nb\nc\nd\n", "a\nb\nX\nd\n"),
        ("a\nb\nc", "a\nb\nX"),
        ("a\nb\nc", "a\nb\nc\nd\ne"),
        ("a\nb\nc\nd", "a\nb"),
        ("a\nb\nc\n", "a\nb\nc"),
        ("", "a\nb\n"),
    ] {
        let project = lantern();
        project.write("eof.txt", old);
        committed(&project);
        project.write("eof.txt", new);
        let session = project.session(&["eof.txt"]);
        let mut page = project.open(&session, WIDE)?;
        let (_, right) = compared(&mut page)?;
        let lines = markers(&page, "apply:");
        assert_eq!(lines.len(), 1, "{old:?} to {new:?}: {lines:?}");
        page.click_gutter(&right, "apply", lines[0], Button::Left)?;
        wait(&mut page, |page| text(page) != new)?;
        assert_eq!(text(&page), old, "{old:?} was changed to {new:?}");
        // As one edit: it is undone at once.
        page.press("cmd+z")?;
        wait(&mut page, |page| text(page) == new)?;
    }
    Ok(())
}

// --- While the text and git move on ----------------------------------------------------------------------

/// The content of the two sides is as tall as each other, to a point.
fn assert_same_height(page: &Page, left: &str, right: &str) {
    let (left_height, right_height) = (extent(page, left).0, extent(page, right).0);
    assert!(
        (left_height - right_height).abs() < 1.0,
        "the left is {left_height} points tall, the right {right_height}"
    );
}

/// A second chevron pressed before the program has answered of the first takes back the change it
/// stands by, or nothing: never the lines where that change was before the first was taken back.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_chevron_pressed_before_the_program_has_answered_takes_back_no_other_lines() -> Result<(), Error>
{
    use pencere_view::Button;
    let (one, both) = {
        let project = changed_project();
        let session = project.session(&["src/server.ts"]);
        let mut page = project.open(&session, WIDE)?;
        let (_, right) = compared(&mut page)?;
        let lines = markers(&page, "apply:");
        page.click_gutter(&right, "apply", lines[0], Button::Left)?;
        wait(&mut page, |page| markers(page, "apply:").len() == 3)?;
        let one = text(&page);
        page.advance(0.5)?;
        page.settle()?;
        let lines = markers(&page, "apply:");
        page.click_gutter(&right, "apply", lines[0], Button::Left)?;
        wait(&mut page, |page| markers(page, "apply:").len() == 2)?;
        (one, text(&page))
    };
    let project = changed_project();
    let session = project.session(&["src/server.ts"]);
    let mut page = project.open(&session, WIDE)?;
    let (_, right) = compared(&mut page)?;
    let lines = markers(&page, "apply:");
    page.click_gutter(&right, "apply", lines[0], Button::Left)?;
    wait(&mut page, |page| text(page) != CHANGED)?;
    // The chevron of the second change, where it is now: the first's has gone with its lines, or is
    // still there where they were.
    let now = markers(&page, "apply:");
    let second = if now.len() == 4 { now[1] } else { now[0] };
    page.click_gutter(&right, "apply", second, Button::Left)?;
    page.advance(1.0)?;
    page.settle()?;
    let after = text(&page);
    assert!(
        after == one || after == both,
        "neither the first alone nor the two: {after:?}"
    );
    Ok(())
}

/// Git cannot read the commit for a moment: the page says so, and when git answers again the left side
/// has the commit's text.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_git_that_failed_gives_the_commits_text_back_when_it_answers_again() -> Result<(), Error> {
    let project = changed_project();
    let session = project.session(&["src/server.ts"]);
    let mut page = project.open(&session, WIDE)?;
    compared(&mut page)?;
    let said = Command::new("git")
        .args(["rev-parse", "HEAD^{tree}"])
        .current_dir(project.root())
        .output()
        .expect("git");
    let tree = String::from_utf8(said.stdout).expect("an id");
    let tree = tree.trim();
    let object = project
        .root()
        .join(".git/objects")
        .join(&tree[..2])
        .join(&tree[2..]);
    let away = object.with_extension("away");
    std::fs::rename(&object, &away).expect("the tree is a file of its own");
    page.type_text("x")?;
    wait(&mut page, |page| shows(page, "could not be compared"))?;
    std::fs::rename(&away, &object).expect("the tree goes back");
    wait(&mut page, |page| {
        left_of(page).is_some_and(|left| {
            page.editor_text(&left).as_deref() == Some(lantern_server_committed())
        })
    })?;
    wait(&mut page, drawn)?;
    page.advance(0.5)?;
    page.settle()?;
    let (left, right) = (
        left_of(&page).expect("a left editor"),
        shown(&page).expect("a right editor"),
    );
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

/// Find goes to a line of a stretch that is folded: the stretch opens on both sides, and the caret
/// stays on the line it went to.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn find_going_into_a_folded_stretch_opens_it_on_both_sides() -> Result<(), Error> {
    let project = long_project();
    let session = project.session(&["src/long.ts"]);
    let mut page = project.open(&session, WIDE)?;
    let (left, right) = compared(&mut page)?;
    let folded = markers(&page, "open:").len();
    page.press("cmd+f")?;
    page.type_text("v50 ")?;
    page.press("enter")?;
    wait(&mut page, |page| shows(page, "Ln 49,"))?;
    wait(&mut page, |page| markers(page, "open:").len() == folded - 1)?;
    page.advance(0.5)?;
    page.settle()?;
    assert!(
        shows(&page, "Ln 49,"),
        "the caret moved: {:?}",
        page_words(&page)
    );
    assert_same_height(&page, &left, &right);
    Ok(())
}

/// The caret on a line that would be folded: it stays there, beside the commit and after it.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_caret_stays_on_its_line_through_the_comparison() -> Result<(), Error> {
    let project = long_project();
    let session = project.session(&["src/long.ts"]);
    let mut page = project.open(&session, WIDE)?;
    page.press("cmd+f")?;
    page.type_text("v50 ")?;
    page.press("enter")?;
    wait(&mut page, |page| shows(page, "Ln 49,"))?;
    page.press("escape")?;
    page.press("left")?;
    wait(&mut page, |page| shows(page, "Ln 49,"))?;
    let (left, right) = compared(&mut page)?;
    assert!(
        shows(&page, "Ln 49,"),
        "beside the commit: {:?}",
        page_words(&page)
    );
    assert_same_height(&page, &left, &right);
    page.press("escape")?;
    wait(&mut page, |page| left_of(page).is_none())?;
    assert!(
        shows(&page, "Ln 49,"),
        "after the comparison: {:?}",
        page_words(&page)
    );
    Ok(())
}

/// The comparison asked for while a save is on its way is laid out when the save is done, with nothing
/// typed since.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_comparison_asked_for_during_a_save_is_laid_out_after_it() -> Result<(), Error> {
    let project = changed_project();
    let session = project.session(&["src/server.ts"]);
    let mut page = project.open(&session, WIDE)?;
    page.type_text("x")?;
    page.advance(0.5)?;
    page.settle()?;
    page.press("cmd+s")?;
    page.press("cmd+shift+d")?;
    wait(&mut page, drawn)?;
    Ok(())
}

/// A new empty file, and lines typed in it: the two sides are lined up by a row that was measured, not
/// by a guess made when there was nothing to measure.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn lines_typed_in_a_new_empty_file_are_lined_up_with_the_empty_commit() -> Result<(), Error> {
    let project = lantern();
    committed(&project);
    project.write("empty.txt", "");
    let session = project.session(&["empty.txt"]);
    let mut page = project.open(&session, WIDE)?;
    page.press("cmd+shift+d")?;
    wait(&mut page, |page| left_of(page).is_some())?;
    page.advance(0.5)?;
    page.settle()?;
    // More than the room shows: a content shorter than its room is as tall as the room.
    page.paste(&"a line\n".repeat(120))?;
    wait(&mut page, |page| markers(page, "apply:").len() == 1)?;
    page.advance(0.5)?;
    page.settle()?;
    let (left, right) = (
        left_of(&page).expect("a left editor"),
        shown(&page).expect("a right editor"),
    );
    assert_same_height(&page, &left, &right);
    Ok(())
}

/// Esc takes the carets added away first, as it does in any editor; the next closes the comparison.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn esc_takes_added_carets_away_before_it_closes_the_comparison() -> Result<(), Error> {
    let project = changed_project();
    let session = project.session(&["src/server.ts"]);
    let mut page = project.open(&session, WIDE)?;
    compared(&mut page)?;
    page.press("ctrl+shift+down")?;
    page.advance(0.1)?;
    page.settle()?;
    page.press("escape")?;
    page.advance(0.1)?;
    page.settle()?;
    assert!(left_of(&page).is_some(), "the first Esc closed it");
    page.press("escape")?;
    wait(&mut page, |page| left_of(page).is_none())?;
    Ok(())
}

/// A change is taken back from the keyboard, and the stretch the caret is on opened; elsewhere the same
/// key shows every line.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_change_and_a_stretch_are_reached_from_the_keyboard() -> Result<(), Error> {
    let project = long_project();
    let session = project.session(&["src/long.ts"]);
    let mut page = project.open(&session, WIDE)?;
    compared(&mut page)?;
    let folded = markers(&page, "open:").len();
    assert!(folded > 1, "{folded} stretches");
    // The caret is on the first line, the one the first stretch folds from.
    page.press("cmd+alt+]")?;
    wait(&mut page, |page| markers(page, "open:").len() == folded - 1)?;
    page.advance(0.5)?;
    page.settle()?;
    // The first change: `const v10 = 1000;` goes back to what the commit has.
    page.press("alt+f5")?;
    wait(&mut page, |page| shows(page, "Ln 11,"))?;
    page.press("ctrl+shift+backspace")?;
    wait(&mut page, |page| text(page).contains("const v10 = 10;\n"))?;
    wait(&mut page, |page| markers(page, "apply:").len() == 2)?;
    page.advance(0.5)?;
    page.settle()?;
    // Not on a stretch's line: every line.
    page.press("alt+f5")?;
    page.advance(0.2)?;
    page.settle()?;
    page.press("cmd+alt+]")?;
    wait(&mut page, |page| markers(page, "open:").is_empty())?;
    Ok(())
}

/// `⇧⌘G` opens the panel of the changed files, except with the find bar open, where it is what it is in
/// any editor: the previous match.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_key_of_the_panel_leaves_the_find_bar_its_previous_match() -> Result<(), Error> {
    let project = changed_project();
    let session = project.session(&["src/server.ts"]);
    let mut page = project.open(&session, WIDE)?;
    page.press("cmd+f")?;
    page.type_text("res")?;
    page.advance(0.2)?;
    page.settle()?;
    page.press("cmd+shift+g")?;
    page.advance(0.2)?;
    page.settle()?;
    assert!(page.bounds("change:src/server.ts").is_none());
    page.press("escape")?;
    page.press("cmd+shift+g")?;
    wait(&mut page, |page| {
        page.bounds("change:src/server.ts").is_some()
    })?;
    Ok(())
}
