//! A PDF in its tab, as the page lays it out and what the person does in it: the pages one under the
//! other as wide as the tab, only those in view drawn, the page and the zoom in the status line, going
//! to a page, searching the words, selecting them with the pointer and copying them. The page and
//! the program's own handlers in one process, as `page_tests` runs them.
//!
//! Each opens the interpreter (compiled once a run): `make test-page` runs them, in `make check`.

use pencere_drive::headless::Error;
use pencere_view::{Button, Cursor, Input, Mods, Paint, Point, Rect};

use crate::page_tests::{
    PANE, Page, Project, WIDE, lantern, methods, page_words, recording, shows, wait,
};

/// Three pages, 612 × 792, 595 × 842 and 300 × 200 points: "Hello world" and "a needle in a haystack"
/// on the first, "Second page" and "Needle" on the second, the third a blue box.
pub(crate) const PAGES: &[u8] = include_bytes!("../../tests/fixtures/pages.pdf");

/// The pages of PDFs the page draws, each with where it is drawn (pixels).
pub(crate) fn pages_drawn(page: &Page) -> Vec<(u32, Rect)> {
    page.list()
        .items
        .iter()
        .filter_map(|item| match item {
            Paint::Image(picture) if picture.src.starts_with("sha256:") => {
                Some((picture.page, picture.rect))
            }
            _ => None,
        })
        .collect()
}

/// The page opened on a project with `spec.pdf` (`bytes`) in front, and the first page drawn.
pub(crate) fn open_pdf(bytes: &[u8], size: (f32, f32)) -> Result<(Project, Page), Error> {
    let project = lantern();
    project.write_bytes("spec.pdf", bytes);
    let session = project.session(&["spec.pdf", "README.md"]);
    recording(&session);
    let mut page = project.open_holding(&session, size, &[bytes])?;
    wait(&mut page, |page| !pages_drawn(page).is_empty())?;
    page.advance(0.3)?;
    Ok((project, page))
}

/// The numbers of the pages that are drawn.
fn numbers(page: &Page) -> Vec<u32> {
    let mut drawn: Vec<u32> = pages_drawn(page)
        .iter()
        .map(|(number, _)| *number)
        .collect();
    drawn.sort_unstable();
    drawn.dedup();
    drawn
}

/// The pages are drawn one under the other as wide as the tab, with room between; only the page in
/// view and the one beside it are made; the status line says which page is in view.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_pdf_shows_its_pages_one_under_the_other_as_wide_as_its_tab() -> Result<(), Error> {
    let (_project, mut page) = open_pdf(PAGES, WIDE)?;
    let room = page.bounds("pdf:spec.pdf").expect("the box of the pages");
    let drawn = pages_drawn(&page);
    assert_eq!(numbers(&page), [1, 2], "{drawn:?}");
    let first = drawn.iter().find(|(n, _)| *n == 1).expect("page 1").1;
    let second = drawn.iter().find(|(n, _)| *n == 2).expect("page 2").1;
    assert!(
        (first.w - (room.w - 80.0)).abs() < 1.5 && (first.x - (room.x + 40.0)).abs() < 1.5,
        "as wide as the tab, 40 points in from each side: {first:?} in {room:?}"
    );
    assert!(
        (first.h / first.w - 792.0 / 612.0).abs() < 0.01,
        "in the page's own proportions: {first:?}"
    );
    assert!(
        (second.y - (first.y + first.h + 16.0)).abs() < 1.5,
        "16 points between: {first:?} {second:?}"
    );
    let words = page_words(&page);
    for said in ["Page 1 of 3", "Fit Width", "PDF"] {
        assert!(shows(&page, said), "{said}: {words:?}");
    }
    assert!(
        shows(&page, &format!("{:.1} KB", PAGES.len() as f64 / 1024.0)),
        "{words:?}"
    );

    // Scrolled to the second, it is the page in view, the third is made, the first still beside it.
    page.scroll_to("pdf:spec.pdf", first.h + 32.0 + 40.0)?;
    wait(&mut page, |page| shows(page, "Page 2 of 3"))?;
    assert_eq!(numbers(&page), [1, 2, 3]);
    page.scroll_to("pdf:spec.pdf", 0.0)?;
    wait(&mut page, |page| shows(page, "Page 1 of 3"))?;
    assert_eq!(numbers(&page), [1, 2]);
    Ok(())
}

/// In an 80×24 pane the sidebar is not there and the status line has only the right-hand side.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_pdf_in_a_small_pane_is_as_wide_as_the_pane() -> Result<(), Error> {
    let (_project, page) = open_pdf(PAGES, PANE)?;
    let room = page.bounds("pdf:spec.pdf").expect("the box of the pages");
    let first = pages_drawn(&page)
        .iter()
        .find(|(n, _)| *n == 1)
        .expect("page 1")
        .1;
    assert!(
        (first.w - (PANE.0 - 80.0)).abs() < 1.5 && room.w > PANE.0 - 2.0,
        "{first:?} in {room:?}"
    );
    assert!(shows(&page, "Page 1 of 3") && shows(&page, "Fit Width"));
    assert!(
        !shows(&page, "find"),
        "no hints in a pane this narrow: {:?}",
        page_words(&page)
    );
    Ok(())
}

/// Where the page puts the middle of the element `name`.
fn middle(page: &Page, name: &str) -> Result<Point, Error> {
    let rect = page
        .bounds(name)
        .ok_or_else(|| Error::NoSuch(name.to_owned()))?;
    Ok(Point::new(rect.x + rect.w / 2.0, rect.y + rect.h / 2.0))
}

/// The person presses the left button at `at` as the `count`th of a run of clicks, and lets go.
fn click_count(page: &mut Page, at: Point, count: u32) -> Result<(), Error> {
    for n in 1..=count {
        page.pointer(Input::Press {
            at,
            button: Button::Left,
            mods: Mods::default(),
            count: n,
        })?;
        page.pointer(Input::Release {
            at,
            button: Button::Left,
        })?;
    }
    Ok(())
}

/// The box of the selected words of a line of a page, once they are on the page.
fn selection_of(page: &Page, page_number: u32, line: u32) -> Option<Rect> {
    page.bounds(&format!("pdf-selection:{page_number}:{line}"))
}

/// The zoom keys scale the pages a step at a time, the status line says the percentage, and Fit Width
/// (⌘0, or the status line) goes back to the tab's width; the page in view stays in view.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn zooming_scales_the_pages_and_fit_width_goes_back() -> Result<(), Error> {
    let (_project, mut page) = open_pdf(PAGES, WIDE)?;
    let fit = pages_drawn(&page)[0].1.w;
    page.press("cmd+0")?;
    assert!(shows(&page, "Fit Width"));

    // Out from the width of the tab, to the step below the scale it is at, and on down to 100%.
    page.press("cmd+-")?;
    wait(&mut page, |page| !shows(page, "Fit Width"))?;
    for _ in 0..3 {
        if shows(&page, "100%") {
            break;
        }
        page.press("cmd+-")?;
        page.advance(0.3)?;
    }
    wait(&mut page, |page| shows(page, "100%"))?;
    let hundred = pages_drawn(&page)
        .iter()
        .find(|(n, _)| *n == 1)
        .expect("page 1")
        .1;
    assert!(
        (hundred.w - 612.0).abs() < 1.5 && hundred.w < fit,
        "a point of the page to a point of the tab: {hundred:?}"
    );
    // In, to the step above.
    page.press("cmd+=")?;
    wait(&mut page, |page| shows(page, "125%"))?;
    page.advance(0.3)?;
    let larger = pages_drawn(&page)
        .iter()
        .find(|(n, _)| *n == 1)
        .expect("page 1")
        .1;
    assert!((larger.w - 765.0).abs() < 1.5, "{larger:?}");
    // Larger than the tab, the pages scroll across too.
    page.press("cmd+=")?;
    wait(&mut page, |page| shows(page, "150%"))?;
    page.advance(0.3)?;
    let larger = pages_drawn(&page)
        .iter()
        .find(|(n, _)| *n == 1)
        .expect("page 1")
        .1;
    let room = page.bounds("pdf:spec.pdf").expect("the box");
    assert!(larger.w + 80.0 > room.w, "{larger:?} in {room:?}");

    // The status line's own: back to the width of the tab.
    page.click("status-zoom")?;
    wait(&mut page, |page| shows(page, "Fit Width"))?;
    let back = pages_drawn(&page)
        .iter()
        .find(|(n, _)| *n == 1)
        .expect("page 1")
        .1;
    assert!((back.w - fit).abs() < 1.5, "{back:?} against {fit}");

    // The zoom is the PDF's: the command is listed for it, and for nothing else.
    page.press("cmd+k")?;
    page.type_text(">zoom")?;
    wait(&mut page, |page| shows(page, "Zoom In"))?;
    page.press("escape")?;
    Ok(())
}

/// The zoom stops at the last step the terminal can draw the page at (it draws no picture past 4096
/// pixels): a Letter page stops at 200%, not at a "300%" it would be drawn at 259% of; the status line
/// says the scale the page is at, and one more step is none.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_zoom_stops_where_the_terminal_draws_no_larger_a_page() -> Result<(), Error> {
    let (_project, mut page) = open_pdf(PAGES, WIDE)?;
    page.press("cmd+0")?;
    let first = |page: &Page| {
        pages_drawn(page)
            .iter()
            .find(|(n, _)| *n == 1)
            .expect("page 1")
            .1
    };
    for _ in 0..14 {
        page.press("cmd+=")?;
        page.advance(0.3)?;
    }
    let drawn = first(&page);
    assert!(
        drawn.h <= 2049.0 && (drawn.w - 2.0 * 612.0).abs() < 1.5,
        "as large as the terminal draws: {drawn:?}"
    );
    let words = format!("{}%", (drawn.w / 612.0 * 100.0).round() as i32);
    assert!(
        shows(&page, &words),
        "the status line says the scale the page is at ({words})"
    );
    // One more step is none: the page and the words stay.
    page.press("cmd+=")?;
    page.advance(0.3)?;
    assert_eq!(first(&page), drawn);
    assert!(shows(&page, &words));
    // Out again: the step below.
    page.press("cmd+-")?;
    wait(&mut page, |page| shows(page, "150%"))?;
    Ok(())
}

/// ⌃G (and the status line's page) asks for a page number and goes there; Esc puts the view back.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn go_to_page_scrolls_to_its_top() -> Result<(), Error> {
    let (_project, mut page) = open_pdf(PAGES, WIDE)?;
    page.press("ctrl+g")?;
    wait(&mut page, |page| shows(page, "Type a page number"))?;
    page.type_text("3")?;
    wait(&mut page, |page| shows(page, "Go to page 3"))?;
    page.press("enter")?;
    wait(&mut page, |page| shows(page, "Page 3 of 3"))?;
    let room = page.bounds("pdf:spec.pdf").expect("the box");
    let third = page.bounds("pdf-page:3").expect("page 3 is made");
    assert!(
        third.y >= room.y && third.y + third.h <= room.y + room.h,
        "the last page, whole in the tab (it cannot go further up): {third:?} in {room:?}"
    );
    // A page that has room under it goes to the top of the tab.
    page.press("ctrl+g")?;
    page.type_text("2")?;
    page.press("enter")?;
    wait(&mut page, |page| shows(page, "Page 2 of 3"))?;
    let second = page.bounds("pdf-page:2").expect("page 2 is made");
    assert!(
        second.y >= room.y && second.y < room.y + 40.0,
        "its top is at the top of the tab: {second:?} in {room:?}"
    );
    // A click on the page in the status line asks too; Esc leaves the view where it was.
    page.click("status-page")?;
    wait(&mut page, |page| shows(page, "Type a page number"))?;
    page.type_text("1")?;
    wait(&mut page, |page| shows(page, "Page 1 of 3"))?;
    page.press("escape")?;
    wait(&mut page, |page| shows(page, "Page 2 of 3"))?;
    Ok(())
}

/// The keys of the pages scroll them: Page Down by a view, End to the last page, Home back.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_keys_scroll_the_pages() -> Result<(), Error> {
    let (_project, mut page) = open_pdf(PAGES, WIDE)?;
    page.press("end")?;
    wait(&mut page, |page| shows(page, "Page 3 of 3"))?;
    page.press("home")?;
    wait(&mut page, |page| shows(page, "Page 1 of 3"))?;
    page.press("page_down")?;
    page.press("page_down")?;
    wait(&mut page, |page| shows(page, "Page 2 of 3"))?;
    Ok(())
}

/// ⌘F opens the bar over the tab, with no Replace and no Select all matches; what is typed is looked
/// for in the words of every page, the marks are on the pages, and ⏎ and ⇧⏎ go from match to match,
/// scrolling to the page it is on.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn searching_a_pdf_marks_the_matches_on_its_pages_and_goes_through_them() -> Result<(), Error> {
    let (_project, mut page) = open_pdf(PAGES, WIDE)?;
    page.press("cmd+f")?;
    assert!(page.bounds("find-bar").is_some());
    assert!(
        page.bounds("find-expand").is_none() && page.bounds("find-all").is_none(),
        "no replacing, no select all"
    );
    page.type_text("needle")?;
    wait(&mut page, |page| shows(page, "1 of 2"))?;
    assert!(methods(&page).contains(&"pdf-find".to_owned()));
    let first = page.bounds("pdf-match:1").expect("a mark on page 1");
    let on_first = page.bounds("pdf-page:1").expect("page 1");
    assert!(
        first.x >= on_first.x
            && first.x + first.w <= on_first.x + on_first.w
            && first.y >= on_first.y
            && first.y + first.h <= on_first.y + on_first.h,
        "{first:?} on {on_first:?}"
    );
    // Next: the match on the second page, scrolled into view and marked as the current one.
    page.press("enter")?;
    wait(&mut page, |page| shows(page, "2 of 2"))?;
    let second = page.bounds("pdf-match:2").expect("a mark on page 2");
    let room = page.bounds("pdf:spec.pdf").expect("the box");
    assert!(
        second.y >= room.y && second.y + second.h <= room.y + room.h,
        "in view: {second:?} in {room:?}"
    );
    // And around the end, back, with Shift.
    page.press("enter")?;
    wait(&mut page, |page| shows(page, "1 of 2"))?;
    page.press("shift+enter")?;
    wait(&mut page, |page| shows(page, "2 of 2"))?;

    // A pattern: the options are the editor's.
    page.click("find-case")?;
    wait(&mut page, |page| shows(page, "1 of 1"))?;
    page.click("find-case")?;
    page.click("find-regex")?;
    wait(&mut page, |page| shows(page, " of 2"))?;
    // Nothing, and not a pattern.
    page.fill("find-query", "zebra")?;
    wait(&mut page, |page| shows(page, "No results"))?;
    page.fill("find-query", "(")?;
    wait(&mut page, |page| shows(page, "Not a pattern"))?;

    // Esc closes the bar, and the marks go.
    page.press("escape")?;
    assert!(page.bounds("find-bar").is_none());
    assert!(page.bounds("pdf-match:1").is_none() && page.bounds("pdf-match:2").is_none());
    Ok(())
}

/// A PDF of pictures has no words: the bar says so, the arrows are off, and the pointer stays an
/// arrow over the pages.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_pdf_with_no_text_says_so_in_the_bar() -> Result<(), Error> {
    let bytes = pdf_of_pages(&[(300.0, 200.0), (300.0, 200.0)]);
    let (_project, mut page) = open_pdf(&bytes, WIDE)?;
    page.press("cmd+f")?;
    page.type_text("total")?;
    wait(&mut page, |page| shows(page, "No text in this PDF"))?;
    assert!(!shows(&page, "No results"));
    let at = middle(&page, "pdf-page:1")?;
    page.pointer(Input::Move(at))?;
    assert_eq!(page.cursor(), Some(Cursor::Arrow));
    Ok(())
}

/// The pointer selects the words of the pages: a drag, a double click for a word, a triple click for a
/// line, ⌘A for all of them; the status line counts the words; ⌘C and the right button's Copy put them
/// on the pasteboard; Esc lets go; the pointer is a text pointer over the words and an arrow elsewhere.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn selecting_the_words_of_a_pdf_and_copying_them() -> Result<(), Error> {
    let (_project, mut page) = open_pdf(PAGES, WIDE)?;
    // All of it: ⌘A reads the pages it spans, and says how many words.
    page.press("cmd+a")?;
    wait(&mut page, |page| shows(page, "10 words selected"))?;
    assert!(
        shows(&page, "copy") && shows(&page, "clear"),
        "{:?}",
        page_words(&page)
    );
    let line = selection_of(&page, 1, 2).expect("the second line of page 1 is marked");
    let on_page = page.bounds("pdf-page:1").expect("page 1");
    assert!(
        line.x >= on_page.x && line.y > on_page.y && line.y + line.h < on_page.y + on_page.h,
        "{line:?} on {on_page:?}"
    );
    page.press("cmd+c")?;
    assert_eq!(
        page.pasteboard(),
        Some("Hello world\na needle in a haystack\nSecond page\nNeedle")
    );
    page.press("escape")?;
    assert!(!shows(&page, "words selected"));

    // The pointer over a line of words is the text pointer, and elsewhere on the page the arrow.
    let over = Point::new(line.x + line.w / 2.0, line.y + line.h / 2.0);
    page.pointer(Input::Move(over))?;
    assert_eq!(page.cursor(), Some(Cursor::Text));
    page.pointer(Input::Move(Point::new(
        on_page.x + on_page.w - 8.0,
        on_page.y + on_page.h - 8.0,
    )))?;
    assert_eq!(page.cursor(), Some(Cursor::Arrow));

    // A double click is a word: "needle" is the third to the eighth character of its line of 22.
    let in_needle = Point::new(line.x + line.w * 5.0 / 22.0, over.y);
    click_count(&mut page, in_needle, 2)?;
    wait(&mut page, |page| shows(page, "1 word selected"))?;
    page.press("cmd+c")?;
    assert_eq!(page.pasteboard(), Some("needle"));
    // A triple click is the line.
    click_count(&mut page, in_needle, 3)?;
    wait(&mut page, |page| shows(page, "5 words selected"))?;
    page.press("cmd+c")?;
    assert_eq!(page.pasteboard(), Some("a needle in a haystack"));

    // A drag from the start of the line to its middle.
    page.press("escape")?;
    page.drag_between(
        Point::new(line.x + 1.0, over.y),
        Point::new(line.x + line.w * 8.5 / 22.0, over.y),
    )?;
    wait(&mut page, |page| shows(page, "2 words selected"))?;
    page.press("cmd+c")?;
    assert_eq!(page.pasteboard(), Some("a needle"));

    // The right button: Copy and Select All, with their keys.
    page.right_click("pdf-page:1")?;
    let menu = page.menu().expect("a menu");
    let titles: Vec<String> = menu
        .entries
        .iter()
        .filter_map(|entry| match entry {
            pencere_view::MenuEntry::Item { title, .. } => Some(title.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(titles, ["Copy", "Select All"]);
    page.pick_titled("Select All")?;
    wait(&mut page, |page| shows(page, "10 words selected"))?;
    Ok(())
}

/// A PDF that is being read says so; one past 8 MiB says so with its size and offers to close; a
/// terminal that does not draw a page of a PDF is told nothing of the file.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_pdf_too_large_to_preview_says_so_in_its_tabs_place() -> Result<(), Error> {
    let mut bytes = pdf_of_pages(&[(300.0, 200.0)]);
    bytes.resize(9 * 1024 * 1024, b' ');
    let project = lantern();
    project.write_bytes("scans.pdf", &bytes);
    let session = project.session(&["README.md"]);
    let recorder = recording(&session);
    let mut page = project.open(&session, WIDE)?;
    page.click("row:scans.pdf")?;
    wait(&mut page, |page| shows(page, "Too large to preview"))?;
    let words = page_words(&page);
    assert!(
        words
            .iter()
            .any(|word| word.starts_with("Too large to preview · ") && word.ends_with("MB")),
        "{words:?}"
    );
    assert!(shows(&page, "Close Tab") && pages_drawn(&page).is_empty());
    assert!(recorder.attached().is_empty());
    Ok(())
}

/// A PDF the program cannot read is a file like any other: the placeholder of a file that is not text.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_pdf_that_is_none_opens_as_a_file_that_is_not_text() -> Result<(), Error> {
    let project = lantern();
    project.write("notes.pdf", "this is text");
    project.write_bytes("broken.pdf", b"%PDF-1.4\n\x00\x01 broken");
    let session = project.session(&["README.md"]);
    recording(&session);
    let mut page = project.open(&session, WIDE)?;
    page.click("row:notes.pdf")?;
    wait(&mut page, |page| {
        crate::page_tests::text(page) == "this is text"
    })?;
    page.click("row:broken.pdf")?;
    wait(&mut page, |page| shows(page, "broken.pdf is not text"))?;
    assert!(pages_drawn(&page).is_empty());
    Ok(())
}

/// Switching tabs keeps where each PDF was scrolled, its zoom and its selection.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_pdf_tab_is_as_it_was_left_when_it_comes_back() -> Result<(), Error> {
    let (_project, mut page) = open_pdf(PAGES, WIDE)?;
    page.press("end")?;
    wait(&mut page, |page| shows(page, "Page 3 of 3"))?;
    page.click("tab:README.md")?;
    wait(&mut page, |page| pages_drawn(page).is_empty())?;
    page.click("tab:spec.pdf")?;
    wait(&mut page, |page| !pages_drawn(page).is_empty())?;
    page.advance(0.3)?;
    assert!(shows(&page, "Page 3 of 3"), "{:?}", page_words(&page));
    Ok(())
}

/// A PDF of pages of the sizes given (points), with nothing drawn on them; each page has contents, an
/// empty stream they share (the reader does not count a page with none).
pub(crate) fn pdf_of_pages(sizes: &[(f32, f32)]) -> Vec<u8> {
    fn object(out: &mut Vec<u8>, offsets: &mut Vec<usize>, body: &str) {
        offsets.push(out.len());
        let number = offsets.len();
        out.extend(format!("{number} 0 obj\n{body}\nendobj\n").bytes());
    }
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    object(&mut out, &mut offsets, "<< /Type /Catalog /Pages 2 0 R >>");
    let kids: Vec<String> = (0..sizes.len())
        .map(|at| format!("{} 0 R", at + 4))
        .collect();
    let pages = format!(
        "<< /Type /Pages /Count {} /Kids [{}] >>",
        sizes.len(),
        kids.join(" ")
    );
    object(&mut out, &mut offsets, &pages);
    object(
        &mut out,
        &mut offsets,
        "<< /Length 0 >>\nstream\n\nendstream",
    );
    for (width, height) in sizes {
        let page = format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {width} {height}] /Contents 3 0 R >>"
        );
        object(&mut out, &mut offsets, &page);
    }
    let table = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1).bytes());
    for offset in &offsets {
        out.extend(format!("{offset:010} 00000 n \n").bytes());
    }
    out.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{table}\n%%EOF\n",
            offsets.len() + 1
        )
        .bytes(),
    );
    out
}

/// A run of words on a page of a PDF: its font (1 Times, 2 Times bold, 3 Courier, 4 Helvetica), its
/// size, and where its left end stands (points from the page's bottom left corner).
pub(crate) struct Run {
    pub(crate) font: u8,
    pub(crate) size: f32,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) text: String,
}

/// A PDF whose pages are `(width, height, runs)`, each run set in one of the four standard fonts
/// (which the file does not carry): words to select, to search and to see.
pub(crate) fn pdf_of_runs(pages: &[(f32, f32, Vec<Run>)]) -> Vec<u8> {
    fn object(out: &mut Vec<u8>, offsets: &mut Vec<usize>, body: &str) {
        offsets.push(out.len());
        let number = offsets.len();
        out.extend(format!("{number} 0 obj\n{body}\nendobj\n").bytes());
    }
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    object(&mut out, &mut offsets, "<< /Type /Catalog /Pages 2 0 R >>");
    // Page `n` (from 0) is the object 7 + 2n, its contents the one after it.
    let kids: Vec<String> = (0..pages.len())
        .map(|at| format!("{} 0 R", 7 + 2 * at))
        .collect();
    object(
        &mut out,
        &mut offsets,
        &format!(
            "<< /Type /Pages /Count {} /Kids [{}] >>",
            pages.len(),
            kids.join(" ")
        ),
    );
    for font in ["Times-Roman", "Times-Bold", "Courier", "Helvetica"] {
        object(
            &mut out,
            &mut offsets,
            &format!("<< /Type /Font /Subtype /Type1 /BaseFont /{font} >>"),
        );
    }
    for (width, height, runs) in pages {
        let page = offsets.len() + 1;
        object(
            &mut out,
            &mut offsets,
            &format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {width} {height}] /Contents {} 0 R \
                 /Resources << /Font << /F1 3 0 R /F2 4 0 R /F3 5 0 R /F4 6 0 R >> >> >>",
                page + 1
            ),
        );
        let mut content = String::new();
        for run in runs {
            let text = run
                .text
                .replace('\\', "\\\\")
                .replace('(', "\\(")
                .replace(')', "\\)");
            content.push_str(&format!(
                "BT /F{} {} Tf {} {} Td ({text}) Tj ET\n",
                run.font, run.size, run.x, run.y
            ));
        }
        object(
            &mut out,
            &mut offsets,
            &format!(
                "<< /Length {} >>\nstream\n{content}endstream",
                content.len()
            ),
        );
    }
    let table = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1).bytes());
    for offset in &offsets {
        out.extend(format!("{offset:010} 00000 n \n").bytes());
    }
    out.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{table}\n%%EOF\n",
            offsets.len() + 1
        )
        .bytes(),
    );
    out
}
