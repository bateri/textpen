//! The page drawn into the cells of a terminal (`pencere-tui`), for the eye: the scenes of the
//! documentation's pictures at 80×24 and 160×48, written to `target/shots/<scene>-<cols>x<rows>.ans`
//! (what a terminal is sent) and `.txt` (the characters). Not committed.
//!
//! The page is laid out on the cells (the measure says a column is TUI_N points, default 8, and a row
//! TUI_ROW, default 18), and the frames are written with `-r<TUI_ROW>` after their size.
//! `TUI_PICTURES=none` draws them as a terminal with no graphics protocol does (pictures in half
//! blocks, and the page told so): `-nopics` last.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pencere_drive::headless::{Error, Headless, InProcess};
use pencere_tui::{CanvasMode, Cells, Frame, ImageMode, Options, Theme, render, show_texts};
use pencere_view::{Id, Paint, Rect};

use super::{lantern_art, spec_pdf, with_changed_server, with_git};
use crate::options;
use crate::page_compare::drawn;
use crate::page_pdf::pages_drawn;
use crate::page_tests::{Page, limits, page_words, pictures_drawn, recording, shown, shows, wait};
use crate::page_tests::{go_to, lantern};

const WITHIN: Duration = Duration::from_secs(20);

fn cell_pt() -> f32 {
    std::env::var("TUI_N")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(8.0)
}

/// Whether the terminal the scenes are drawn for has Kitty graphics (the default) or none.
fn has_pictures() -> bool {
    std::env::var("TUI_PICTURES").map_or(true, |pictures| pictures != "none")
}

fn row_pt() -> f32 {
    std::env::var("TUI_ROW")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(18.0)
}

fn open_tui(program: InProcess, size: (u16, u16), files: &[&[u8]]) -> Result<Page, Error> {
    open_tui_with(program, size, files, has_pictures())
}

/// The page of a terminal that draws pictures (`pictures`) or draws none, and so tells its page that a
/// picture is coarse.
fn open_tui_with(
    program: InProcess,
    (cols, rows): (u16, u16),
    files: &[&[u8]],
    pictures: bool,
) -> Result<Page, Error> {
    let (n, row) = (cell_pt(), row_pt());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |since| since.as_secs_f64());
    let mut opening = Headless::app(env!("CARGO_MANIFEST_DIR"))
        .size(f32::from(cols) * n, f32::from(rows) * row)
        .measure(Cells::new(n, row))
        // What the screen tells its page of the keyboard: a terminal has none of a Mac's.
        .keyboard(pencere_view::KeyConvention::Linux)
        // What the screen tells its page.
        .draws(if pictures {
            vec![pencere_view::Drawing::Cells]
        } else {
            vec![
                pencere_view::Drawing::Cells,
                pencere_view::Drawing::CoarseImages,
            ]
        })
        .wall_clock(now, 0)
        .limits(limits(None));
    for bytes in files {
        opening = opening.attached(bytes);
    }
    let mut page = opening.peer(program).open()?;
    page.wait_for(WITHIN, |page| {
        shown(page).is_some()
            || !pictures_drawn(page).is_empty()
            || page_words(page).iter().any(|word| word == "Go to File")
            || shows(page, "This terminal does not show PDFs")
    })?;
    page.advance(0.5)?;
    Ok(page)
}

fn open(
    project: &crate::page_tests::Project,
    tabs: &[&str],
    size: (u16, u16),
) -> Result<Page, Error> {
    let session = project.session(tabs);
    let program = InProcess::new(options(&session, &[], &Arc::new(Mutex::new(None))));
    let mut page = open_tui(program, size, &[])?;
    wait(&mut page, |page| shown(page).is_some() || tabs.is_empty())?;
    page.advance(1.5)?;
    Ok(page)
}

/// What the page's fields hold, by the names the page gives them: the screen draws it over the frame.
fn typed(page: &Page) -> HashMap<Id, String> {
    [
        "find-query",
        "find-replace",
        "palette-query",
        "search-query",
        "dialog-input",
        "help-filter",
    ]
    .into_iter()
    .filter_map(|name| {
        let text = page.field(name).filter(|text| !text.is_empty())?;
        Some((Id::named(name), text.to_string()))
    })
    .collect()
}

fn shoot(
    page: &mut Page,
    name: &str,
    size: (u16, u16),
    theme: &Theme,
    images: ImageMode,
    canvases: CanvasMode,
) -> Result<Frame, Error> {
    let options = Options {
        col: cell_pt(),
        row: row_pt(),
        images,
        canvases,
        pixels: (12, 24),
    };
    let mut tries = 0;
    let mut frame = loop {
        let frame = render(
            page.list(),
            size.0,
            size.1,
            theme,
            &page.page().images(),
            &options,
        );
        if !frame.waiting || tries >= 100 {
            break frame;
        }
        tries += 1;
        page.advance(0.1)?;
        std::thread::sleep(Duration::from_millis(60));
    };
    // What a field says (its hint) and holds, which the screen puts over the frame as it draws it.
    show_texts(
        &mut frame,
        page.page().fields(),
        &typed(page),
        (cell_pt(), row_pt()),
        theme,
    );
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/shots");
    std::fs::create_dir_all(&out).expect("the folder");
    let light = std::env::var("TUI_THEME").is_ok_and(|theme| theme == "light");
    let stem = format!(
        "{name}-{}x{}-r{}{}{}",
        size.0,
        size.1,
        row_pt(),
        if light { "-light" } else { "" },
        if has_pictures() { "" } else { "-nopics" }
    );
    std::fs::write(out.join(format!("{stem}.ans")), frame.ansi()).expect("the frame written");
    std::fs::write(out.join(format!("{stem}.txt")), frame.text()).expect("the text written");
    eprintln!(
        "SHOT {stem}: text_cells={} split={} overlaps={} placed={} waiting={}",
        frame.stats.text_cells,
        frame.stats.split,
        frame.stats.overlaps,
        frame.placements.len(),
        frame.waiting,
    );
    Ok(frame)
}

fn dark(page: &mut Page, name: &str, size: (u16, u16)) -> Result<Frame, Error> {
    shoot(
        page,
        name,
        size,
        &if std::env::var("TUI_THEME").is_ok_and(|theme| theme == "light") {
            Theme::light()
        } else {
            Theme::dark()
        },
        if has_pictures() {
            ImageMode::Kitty
        } else {
            ImageMode::Blocks
        },
        CanvasMode::Characters,
    )
}

fn editor_scenes(size: (u16, u16)) -> Result<(), Error> {
    let wide = size.0 >= 120;
    // The editor, a file among its tabs, the tree open on its folders where there is room.
    let project = lantern();
    with_git(&project);
    let mut page = open(
        &project,
        &[
            "src/server.ts",
            "src/lib/format.ts",
            "README.md",
            "worker/mailer.py",
        ],
        size,
    )?;
    if wide {
        page.press("ctrl+shift+e")?;
        for folder in ["row:src/lib", "row:src/routes"] {
            page.click(folder)?;
            page.advance(0.3)?;
        }
        page.press("escape")?;
    }
    go_to(&mut page, "src/server.ts:19:5")?;
    page.advance(1.5)?;
    dark(&mut page, "editor", size)?;

    // The find bar over the editor.
    page.press("ctrl+f")?;
    page.type_text("order")?;
    page.advance(0.5)?;
    dark(&mut page, "find", size)?;
    page.press("escape")?;

    // The palette, with what is typed.
    go_to(&mut page, "src/lib/format.ts")?;
    page.press("ctrl+k")?;
    page.advance(0.5)?;
    page.type_text("ord")?;
    page.advance(0.5)?;
    dark(&mut page, "palette", size)?;
    page.press("escape")?;

    // The search in the project's files, a match open.
    page.press("ctrl+shift+f")?;
    page.type_text("order")?;
    page.advance(0.5)?;
    wait(&mut page, |page| shows(page, "results in"))?;
    page.press("down")?;
    page.press("down")?;
    page.press("down")?;
    page.press("space")?;
    page.advance(1.5)?;
    dark(&mut page, "search", size)?;
    Ok(())
}

fn compare_scene(size: (u16, u16)) -> Result<(), Error> {
    let project = lantern();
    with_changed_server(&project);
    let mut page = open(&project, &["src/server.ts"], size)?;
    page.press("ctrl+shift+d")?;
    wait(&mut page, drawn)?;
    page.advance(1.0)?;
    page.settle()?;
    dark(&mut page, "compare", size)?;
    Ok(())
}

fn open_pdf(bytes: &[u8], size: (u16, u16)) -> Result<(crate::page_tests::Project, Page), Error> {
    let project = lantern();
    project.write_bytes("docs/pencere-spec.pdf", bytes);
    with_git(&project);
    let session = project.session(&["docs/pencere-spec.pdf", "README.md", "src/server.ts"]);
    recording(&session);
    let program = InProcess::new(options(&session, &[], &Arc::new(Mutex::new(None))));
    let mut page = open_tui(program, size, &[bytes])?;
    wait(&mut page, |page| {
        !pages_drawn(page).is_empty() || shows(page, "This terminal does not show PDFs")
    })?;
    page.advance(1.5)?;
    Ok((project, page))
}

fn pdf_scene(size: (u16, u16)) -> Result<(), Error> {
    if !has_pictures() {
        // A terminal with no graphics protocol: the tab says it shows no PDFs.
        let (_project, mut page) = open_pdf(&spec_pdf(), size)?;
        dark(&mut page, "pdf-sentence", size)?;
        return Ok(());
    }
    let (_project, mut page) = open_pdf(&spec_pdf(), size)?;
    page.press("ctrl+g")?;
    page.type_text("3")?;
    page.press("enter")?;
    page.advance(0.5)?;
    page.press("ctrl+f")?;
    page.type_text("attach")?;
    wait(&mut page, |page| shows(page, " of "))?;
    page.press("enter")?;
    page.press("enter")?;
    page.advance(1.0)?;
    // With the terminal's graphics protocol, and with none: the page in half blocks.
    shoot(
        &mut page,
        "pdf-pixels",
        size,
        &Theme::dark(),
        ImageMode::Kitty,
        CanvasMode::Characters,
    )?;
    shoot(
        &mut page,
        "pdf-blocks",
        size,
        &Theme::dark(),
        ImageMode::Blocks,
        CanvasMode::Characters,
    )?;
    Ok(())
}

#[test]
#[ignore = "shots: draws the page into the cells of a terminal, target/shots/"]
fn the_scenes_of_the_cells() -> Result<(), Error> {
    let only = std::env::var("TUI_ONLY").unwrap_or_default();
    let wants = |name: &str| only.is_empty() || only.contains(name);
    let _ = lantern_art;
    for size in [(80, 24), (160, 48)] {
        if wants("editor") {
            editor_scenes(size)?;
        }
        if wants("compare") {
            compare_scene(size)?;
        }
        if wants("pdf") {
            pdf_scene(size)?;
        }
    }
    Ok(())
}

/// What stands past the box it is in, in the cells it does so by, said once for each.
fn spilled(page: &Page) -> Vec<String> {
    let (n, row) = (cell_pt(), row_pt());
    let mut said: Vec<String> = page
        .spills()
        .iter()
        .map(|spill| {
            let [left, top, right, bottom] = spill.past;
            format!(
                "{:?} in {:?}: {} left, {} above, {} right, {} below (columns, rows)",
                spill.words,
                spill.around,
                left / n,
                top / row,
                right / n,
                bottom / row
            )
        })
        .collect();
    said.sort();
    said.dedup();
    said
}

/// Looks at `page` at three heights and, in each of three widths a page lays itself out for, at
/// seven widths one after another (a size that rounds wrong does so at some widths of every few
/// and not at the others), then at every width from 80 to 170 columns at one height (a line that
/// fills does so over a stretch of widths between the ones a page lays itself out for), and adds
/// what stands past its box to `found` under `name`.
fn sweep(
    page: &mut Page,
    name: &str,
    found: &mut std::collections::BTreeMap<String, Vec<String>>,
) -> Result<usize, Error> {
    let (n, row) = (cell_pt(), row_pt());
    let mut looked = 0;
    let banded = [24_u16, 40, 56].into_iter().flat_map(|rows| {
        (80..87_u16)
            .chain(120..127)
            .chain(156..163)
            .map(move |cols| (cols, rows))
    });
    let every = (80..=170_u16).map(|cols| (cols, 40_u16));
    for (cols, rows) in banded.chain(every) {
        page.resize(f32::from(cols) * n, f32::from(rows) * row)?;
        looked += 1;
        for line in spilled(page) {
            found
                .entry(format!("{name}: {line}"))
                .or_default()
                .push(format!("{cols}x{rows}"));
        }
    }
    Ok(looked)
}

/// On the cells of a terminal nothing on any screen of the page stands past the box it is in, at
/// any size of the window: a part a column wider than the room its box has for it, or a row taller,
/// is cut off at the box's edge or stands on the box's padding.
#[test]
#[ignore = "opens the kit's interpreter: make test-page"]
fn nothing_stands_past_its_box_on_any_screen_at_any_size_of_a_terminal() -> Result<(), Error> {
    let mut found = std::collections::BTreeMap::new();
    let mut looked = 0;
    let project = lantern();
    with_git(&project);
    let mut page = open(
        &project,
        &[
            "src/server.ts",
            "src/lib/format.ts",
            "README.md",
            "worker/mailer.py",
        ],
        (160, 48),
    )?;
    go_to(&mut page, "src/server.ts:19:5")?;
    page.advance(0.5)?;
    looked += sweep(&mut page, "the editor", &mut found)?;

    page.press("ctrl+shift+e")?;
    page.advance(0.5)?;
    looked += sweep(&mut page, "the tree", &mut found)?;
    page.press("escape")?;

    page.press("ctrl+f")?;
    page.type_text("order")?;
    page.advance(0.5)?;
    looked += sweep(&mut page, "the find bar", &mut found)?;
    page.press("escape")?;

    page.press("ctrl+k")?;
    page.advance(0.5)?;
    page.type_text("ord")?;
    page.advance(0.5)?;
    looked += sweep(&mut page, "the palette", &mut found)?;
    page.press("escape")?;

    page.press("ctrl+shift+f")?;
    page.type_text("order")?;
    page.advance(0.5)?;
    wait(&mut page, |page| shows(page, "results in"))?;
    looked += sweep(&mut page, "the search", &mut found)?;
    drop(page);

    let project = lantern();
    with_changed_server(&project);
    let mut page = open(&project, &["src/server.ts"], (160, 48))?;
    page.press("ctrl+shift+d")?;
    page.advance(1.0)?;
    looked += sweep(&mut page, "the comparison", &mut found)?;

    assert!(looked >= 900, "only {looked} windows were looked at");
    assert!(found.is_empty(), "{} kinds: {found:#?}", found.len());
    Ok(())
}

// --- The marks and lines of a page in the pixels of a terminal with pictures --------------------------------

/// What a frame's pictures put on the pixels of the terminal: for each picture, the pixels it covers
/// (the cells it is placed on, `PX` pixels across and `PY` down a cell), each the picture's pixel
/// that falls there, as red, green, blue and opacity.
const PX: usize = 12;
const PY: usize = 24;

struct Seen {
    z: i32,
    /// The left and top pixel of the cells it is placed on, and how many pixels across and down.
    at: (usize, usize),
    size: (usize, usize),
    pixels: Vec<[u8; 4]>,
}

impl Seen {
    fn of(placement: &pencere_tui::Placement) -> Self {
        let (cols, rows) = (usize::from(placement.cols), usize::from(placement.rows));
        let (width, height) = (cols * PX, rows * PY);
        let source = placement.rgba();
        let (sw, sh) = (placement.width as usize, placement.height as usize);
        let mut pixels = Vec::with_capacity(width * height);
        for y in 0..height {
            for x in 0..width {
                let (u, v) = ((x * sw / width).min(sw - 1), (y * sh / height).min(sh - 1));
                let at = (v * sw + u) * 4;
                pixels.push([source[at], source[at + 1], source[at + 2], source[at + 3]]);
            }
        }
        Self {
            z: placement.z,
            at: (
                usize::from(placement.col) * PX,
                usize::from(placement.row) * PY,
            ),
            size: (width, height),
            pixels,
        }
    }

    /// The pixel at the terminal's pixel `x`, `y`, if the picture covers it.
    fn pixel(&self, x: usize, y: usize) -> Option<[u8; 4]> {
        let (left, top) = (x.checked_sub(self.at.0)?, y.checked_sub(self.at.1)?);
        (left < self.size.0 && top < self.size.1).then(|| self.pixels[top * self.size.0 + left])
    }

    /// The pixels of the terminal that the picture draws on (opaque at all), as the smallest box:
    /// left, top, right and bottom, the last two not included.
    fn bounds(&self) -> Option<(usize, usize, usize, usize)> {
        let mut bounds: Option<(usize, usize, usize, usize)> = None;
        for y in 0..self.size.1 {
            for x in 0..self.size.0 {
                if self.pixels[y * self.size.0 + x][3] > 0 {
                    let (x, y) = (x + self.at.0, y + self.at.1);
                    bounds = Some(match bounds {
                        None => (x, y, x + 1, y + 1),
                        Some((l, t, r, b)) => (l.min(x), t.min(y), r.max(x + 1), b.max(y + 1)),
                    });
                }
            }
        }
        bounds
    }
}

/// Whether the terminal's pixel `x`, `y` of the page is ink: dark where the page's picture is drawn.
fn ink(page: &Seen, x: usize, y: usize) -> bool {
    page.pixel(x, y)
        .is_some_and(|[r, g, b, a]| a > 0 && u32::from(r) + u32::from(g) + u32::from(b) < 3 * 190)
}

/// A frame of the PDF open at its third page, "attach" found and the first of the finds chosen, drawn
/// for a terminal with the Kitty graphics protocol, as the pictures it puts on the terminal's pixels.
fn found_pdf(size: (u16, u16)) -> Result<(Frame, Vec<Seen>), Error> {
    found_pdf_then(size, 0)
}

/// As [`found_pdf`], and the next find chosen `next` times more.
fn found_pdf_then(size: (u16, u16), next: usize) -> Result<(Frame, Vec<Seen>), Error> {
    let (_project, mut page) = open_pdf(&spec_pdf(), size)?;
    page.press("ctrl+g")?;
    page.type_text("3")?;
    page.press("enter")?;
    page.advance(0.5)?;
    page.press("ctrl+f")?;
    page.type_text("attach")?;
    wait(&mut page, |page| shows(page, " of "))?;
    page.press("enter")?;
    page.press("enter")?;
    for _ in 0..next {
        page.press("enter")?;
    }
    page.advance(1.0)?;
    let frame = shoot(
        &mut page,
        "pdf-pixels",
        size,
        &Theme::dark(),
        ImageMode::Kitty,
        CanvasMode::Characters,
    )?;
    let seen = frame.placements.iter().map(Seen::of).collect();
    Ok((frame, seen))
}

/// The mark of a word found in a PDF stands on the word in the pixels of a terminal that draws pictures:
/// on the one line of text it is in, and no wider than the word by more than a few pixels, where a mark
/// that is the cells it touches (a cell is 12 by 24 pixels, a line of the page 20) covers the line under
/// the word and the letters beside it. (How tall the mark is above and below the letters is the text
/// layer's: the box of the word, taller than its ink in a heading.)
#[test]
#[ignore = "opens the kit's interpreter: make test-page"]
fn the_marks_and_lines_of_a_found_word_stand_on_the_word_in_the_pixels_of_a_terminal() {
    let (_frame, seen) = found_pdf((160, 48)).expect("the page opens");
    // The picture of the page is the lowest, under the grounds of the cells; the marks are over it
    // and under the text.
    let page = seen
        .iter()
        .filter(|seen| seen.z < -1_000_000_000)
        .max_by_key(|seen| seen.size.0 * seen.size.1)
        .expect("the picture of the page is drawn");
    let over_the_page = |seen: &&Seen| {
        seen.at.0 >= page.at.0
            && seen.at.1 >= page.at.1
            && seen.at.0 + seen.size.0 <= page.at.0 + page.size.0
            && seen.at.1 + seen.size.1 <= page.at.1 + page.size.1
    };
    let marks: Vec<&Seen> = seen
        .iter()
        .filter(|seen| seen.z == -1)
        .filter(over_the_page)
        .collect();
    assert!(
        marks.len() >= 3,
        "the finds of the page are marked ({} pictures over it)",
        marks.len()
    );
    // The lines of text of the page: the rows of its pixels that have ink, from a blank row to the next.
    let (left, top) = page.at;
    let (right, bottom) = (left + page.size.0, top + page.size.1);
    let has_ink = |y: usize| (left..right).any(|x| ink(page, x, y));
    let mut lines: Vec<(usize, usize)> = Vec::new();
    let mut from = None;
    for y in top..=bottom {
        match (from, y < bottom && has_ink(y)) {
            (None, true) => from = Some(y),
            (Some(start), false) => {
                lines.push((start, y));
                from = None;
            }
            _ => {}
        }
    }
    let mut wrong = Vec::new();
    for mark in &marks {
        let Some((l, t, r, b)) = mark.bounds() else {
            continue;
        };
        let touched = lines
            .iter()
            .filter(|(start, end)| *start < b && t < *end)
            .count();
        // The ink inside the mark, as the smallest box.
        let mut inked: Option<(usize, usize, usize, usize)> = None;
        for y in t..b {
            for x in l..r {
                if ink(page, x, y) {
                    inked = Some(match inked {
                        None => (x, y, x + 1, y + 1),
                        Some((il, it, ir, ib)) => {
                            (il.min(x), it.min(y), ir.max(x + 1), ib.max(y + 1))
                        }
                    });
                }
            }
        }
        let Some((il, it, ir, ib)) = inked else {
            wrong.push(format!(
                "the mark at {l},{t} to {r},{b} has no word under it"
            ));
            continue;
        };
        // How far the mark reaches past the word: left, above, right, below.
        let past = (il - l, it - t, r - ir, b - ib);
        if touched != 1 || past.0 > 4 || past.2 > 4 {
            wrong.push(format!(
                "the mark at {l},{t} to {r},{b} covers {touched} lines of text and reaches {past:?} pixels (left, above, right, below) past the ink under it"
            ));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// The marks of the finds over the page's picture, by where they are on the terminal's pixels: the
/// smallest box each draws on, left, top, right and bottom.
fn mark_boxes(seen: &[Seen]) -> Vec<(usize, usize, usize, usize)> {
    let mut boxes: Vec<(usize, usize, usize, usize)> = seen
        .iter()
        .filter(|seen| seen.z == -1)
        .filter_map(Seen::bounds)
        .collect();
    boxes.sort_by_key(|(l, t, ..)| (*t / 12, *l));
    boxes
}

/// The find that is chosen has its frame on the same box its fill has when another is chosen: a mark does
/// not move by being the current one (it was a few pixels below the others, its top edge on the tops of
/// the letters, where the others stand above).
#[test]
#[ignore = "opens the kit's interpreter: make test-page"]
fn the_chosen_find_stands_on_the_box_it_has_when_another_is_chosen() {
    let (_frame, first) = found_pdf_then((160, 48), 0).expect("the page opens");
    let (_frame, second) = found_pdf_then((160, 48), 1).expect("the page opens");
    let (one, two) = (mark_boxes(&first), mark_boxes(&second));
    assert_eq!(one.len(), two.len(), "{one:?} against {two:?}");
    assert!(one.len() >= 3, "the finds are marked: {one:?}");
    let mut moved = Vec::new();
    for (at, (a, b)) in one.iter().zip(&two).enumerate() {
        // The frame is a pixel or two out from the fill it holds.
        let apart = |x: usize, y: usize| x.abs_diff(y);
        if apart(a.0, b.0) > 2 || apart(a.1, b.1) > 2 || apart(a.2, b.2) > 2 || apart(a.3, b.3) > 2
        {
            moved.push(format!(
                "mark {at}: {a:?} when chosen first, {b:?} when chosen second"
            ));
        }
    }
    assert!(moved.is_empty(), "{moved:#?}");
}

// --- The line over the open tab --------------------------------------------------------------------

/// The lines over the tabs of the page: boxes thinner than half a row, wider than three columns, in the
/// first two rows.
fn tab_lines(page: &Page) -> Vec<Rect> {
    let (n, row) = (cell_pt(), row_pt());
    page.list()
        .items
        .iter()
        .filter_map(|item| match item {
            Paint::Quad(quad)
                if quad.fill.is_some()
                    && quad.rect.h < row / 2.0
                    && quad.rect.w >= 3.0 * n
                    && quad.rect.y < 2.0 * row =>
            {
                Some(quad.rect)
            }
            _ => None,
        })
        .collect()
}

/// The page at an editor with two tabs, and the line over the open one.
fn tabs_open(size: (u16, u16)) -> (Page, Rect) {
    let project = lantern();
    with_git(&project);
    let page = open(&project, &["src/server.ts", "README.md"], size).expect("the page opens");
    let lines = tab_lines(&page);
    assert_eq!(lines.len(), 1, "one line over the open tab: {lines:?}");
    let line = lines[0];
    (page, line)
}

/// The line over the open tab runs unbroken across the title where the terminal draws pictures: it is
/// a picture over the title's letters, as thick as the page has it. (A line of characters is cut by the
/// letters: each cell of the title is the letter or the line.)
#[test]
#[ignore = "opens the kit's interpreter: make test-page"]
fn the_line_over_the_open_tab_is_one_picture_across_its_title_where_the_terminal_has_pictures() {
    let size = (160, 48);
    let (mut page, line) = tabs_open(size);
    let (n, row) = (cell_pt(), row_pt());
    let (first, last) = (
        (line.x / n).floor() as usize,
        ((line.x + line.w) / n).ceil() as usize,
    );
    let frame = shoot(
        &mut page,
        "tab-line",
        size,
        &Theme::dark(),
        ImageMode::Kitty,
        CanvasMode::Characters,
    )
    .expect("the frame");
    let seen: Vec<Seen> = frame
        .placements
        .iter()
        .filter(|p| p.z > 0)
        .filter(|p| usize::from(p.col) < last && usize::from(p.col + p.cols) > first)
        .map(Seen::of)
        .collect();
    assert!(!seen.is_empty(), "the line is a picture over the text");
    let (left, right) = (
        (line.x * PX as f32 / n).round() as usize,
        ((line.x + line.w) * PX as f32 / n).round() as usize,
    );
    let top = (line.y * PY as f32 / row).round() as usize;
    let thick = ((line.h * PY as f32 / row).round() as usize).max(1);
    let gaps: Vec<usize> = (left..right)
        .filter(|x| {
            !(top..top + thick).all(|y| {
                seen.iter()
                    .any(|seen| seen.pixel(*x, y).is_some_and(|[_, _, _, alpha]| alpha > 0))
            })
        })
        .collect();
    assert!(
        gaps.is_empty(),
        "the line is broken at {} of its {} pixels across: {gaps:?}",
        gaps.len(),
        right - left
    );
}

/// Where the terminal has no pictures the line over the open tab is not drawn: a line of characters
/// has the letters of the title's cells missing from it, and the parts left read as marks beside the
/// title.
#[test]
#[ignore = "opens the kit's interpreter: make test-page"]
fn the_line_over_the_open_tab_is_not_drawn_in_pieces_where_the_terminal_has_no_pictures() {
    let size = (160, 48);
    let (mut page, line) = tabs_open(size);
    let n = cell_pt();
    let (first, last) = (
        (line.x / n).floor() as usize,
        ((line.x + line.w) / n).ceil() as usize,
    );
    let frame = shoot(
        &mut page,
        "tab-line",
        size,
        &Theme::dark(),
        ImageMode::Blocks,
        CanvasMode::Characters,
    )
    .expect("the frame");
    let text = frame.text();
    for (at, characters) in text.lines().take(2).enumerate() {
        let parts = characters
            .chars()
            .skip(first)
            .take(last - first)
            .filter(|c| matches!(c, '\u{2594}' | '\u{2581}'))
            .count();
        assert_eq!(
            parts, 0,
            "row {at} has {parts} parts of the line left over the tab: {characters:?}"
        );
    }
}

/// Where the terminal draws no pictures, the icons of the sidebar's buttons and of the branch in the
/// status line are characters, as the icons of the other buttons are: a picture of 15 points in half
/// blocks of a cell is nothing the eye finds, and the button reads as a gap.
#[test]
#[ignore = "opens the kit's interpreter: make test-page"]
fn the_icons_of_the_sidebar_and_of_the_branch_are_characters_where_the_terminal_has_no_pictures() {
    let size = (160, 48);
    let project = lantern();
    with_git(&project);
    let session = project.session(&["src/server.ts"]);
    let program = InProcess::new(options(&session, &[], &Arc::new(Mutex::new(None))));
    let mut page = open_tui_with(program, size, &[], false).expect("the page opens");
    wait(&mut page, |page| shown(page).is_some()).expect("an editor");
    page.advance(1.5).expect("the page settles");
    let frame = shoot(
        &mut page,
        "editor-icons",
        size,
        &Theme::dark(),
        ImageMode::Blocks,
        CanvasMode::Characters,
    )
    .expect("the frame");
    let text = frame.text();
    let rows: Vec<Vec<char>> = text.lines().map(|line| line.chars().collect()).collect();
    let cells = |id: &str| -> String {
        let rect = page
            .bounds(id)
            .unwrap_or_else(|| panic!("{id} is on the page"));
        let (col, row) = ((rect.x / cell_pt()) as usize, (rect.y / row_pt()) as usize);
        let width = ((rect.w / cell_pt()).ceil() as usize).max(1);
        rows.get(row)
            .map(|line| line.iter().skip(col).take(width).collect())
            .unwrap_or_default()
    };
    for id in ["sidebar-files", "sidebar-search", "sidebar-changes"] {
        let held = cells(id);
        assert!(
            held.chars().any(|c| !c.is_whitespace()),
            "the button {id} is {held:?}: nothing to see in its cells"
        );
    }
    // The branch: a character stands before its name.
    let branch = cells("status/branch");
    let first = branch.trim_start();
    assert!(
        !first.is_empty() && !first.starts_with("main"),
        "the branch of the status line is {branch:?}: its name has no icon before it"
    );
}
