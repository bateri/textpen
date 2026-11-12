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
use pencere_view::Id;

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

fn open_tui(program: InProcess, (cols, rows): (u16, u16), files: &[&[u8]]) -> Result<Page, Error> {
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
        .draws(if has_pictures() {
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
