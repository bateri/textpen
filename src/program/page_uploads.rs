//! The files that change hands, as the person does it to the page: dragged over the tree and dropped on
//! it, and what the page shows while the program receives them. The page and the program's handlers
//! run in one process (as `page_tests` runs them); the terminal that moves the bytes is a double that
//! holds a file where a test says (`Double::hold_at`), so that the page can be looked at half way.
//! What crosses a real terminal is held by `tests/uploads.rs`.
//!
//! Each opens the interpreter (compiled once a run): `make test-page` runs them, in `make check`.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use pencere_drive::headless::{Error, InProcess};
use pencere_view::{Button, FileEntry, FileSource, Input, Mods, Panel, Point};

use crate::options;
use crate::page_tests::{
    Page, Project, WIDE, lantern, open_patiently, page_words, pictures_drawn, png, recording,
    shown, shows,
};
use crate::state::session::Session;
use crate::state::transfers::Transfer;
use crate::state::transfers::double::Double;

/// The double that moves the files in place of the terminal.
pub(crate) fn moving(session: &Session) -> Arc<Double> {
    let double = Arc::new(Double::default());
    session
        .transfers
        .using(Arc::clone(&double) as Arc<dyn Transfer>);
    double
}

/// How long the host lets a program that owes the page an answer take to give it, in these tests: a
/// file held half way is a call that is not answered, and the page is looked at meanwhile.
const PATIENCE: Duration = Duration::from_millis(1000);

/// The page opened on the program's handlers for `session`, that gives up waiting for an answer
/// sooner than the usual minute.
pub(crate) fn open_quick(session: &Arc<Session>, size: (f32, f32)) -> Result<Page, Error> {
    open_quick_in(session, size, None, PATIENCE)
}

/// As [`open_quick`], drawn in `fonts` (for the pictures of docs/) and giving `patience`.
pub(crate) fn open_quick_in(
    session: &Arc<Session>,
    size: (f32, f32),
    fonts: Option<&pencere_drive::shot::Fonts>,
    patience: Duration,
) -> Result<Page, Error> {
    // The kit's interpreter is compiled by the first page of a run, which takes longer than the
    // patience: a page is opened with the usual one first.
    static COMPILED: OnceLock<()> = OnceLock::new();
    COMPILED.get_or_init(|| {
        let project = Project::new(&[("a.txt", "")]);
        let session = project.session(&[]);
        drop(project.open(&session, WIDE));
    });
    let program = InProcess::new(options(session, &[], &Arc::new(Mutex::new(None))));
    open_patiently(program, size, fonts, 0.0, &[], Some(patience))
}

/// A call that is still out makes the host wait for its answer and give up: the page was told
/// all the same, and what it said since is in what it shows.
pub(crate) fn through<T>(done: Result<T, Error>) -> Result<(), Error> {
    match done {
        Ok(_) | Err(Error::Slow { .. }) => Ok(()),
        Err(other) => Err(other),
    }
}

/// The person clicks the element named `name`: a press and its release, each told to the page
/// whether or not the host has had its answer (a call out makes the host give up waiting).
pub(crate) fn click(page: &mut Page, name: &str) -> Result<(), Error> {
    let at = page
        .bounds(name)
        .ok_or_else(|| Error::NoSuch(name.to_owned()))?;
    let at = Point::new(at.x + at.w / 2.0, at.y + at.h / 2.0);
    page.page_mut().input(Input::Press {
        at,
        button: Button::Left,
        mods: Mods::default(),
        count: 1,
    });
    through(page.settle())?;
    page.page_mut().input(Input::Release {
        at,
        button: Button::Left,
    });
    through(page.settle())
}

/// Waits for the page to come to `done`, with files held half way: as `wait` does, the host's patience
/// for an answer not counted against it.
#[track_caller]
pub(crate) fn wait(page: &mut Page, mut done: impl FnMut(&Page) -> bool) -> Result<(), Error> {
    let at = std::panic::Location::caller();
    let began = Instant::now();
    loop {
        if done(page) {
            return Ok(());
        }
        if began.elapsed() > Duration::from_secs(20) {
            eprintln!(
                "{at}: waited in vain; the page shows {:?}",
                page_words(page)
            );
            return Err(Error::Slow {
                what: "the page did not come to what was waited for",
                waited: began.elapsed(),
            });
        }
        through(page.tick())?;
    }
}

/// The person gives the page files on the middle of the element named `name`.
pub(crate) fn give(
    page: &mut Page,
    name: &str,
    source: FileSource,
    files: &[(&str, &[u8])],
) -> Result<(), Error> {
    through(page.give_files(name, source, files))
}

/// The handle the host makes of the `n`th file the person gives the page.
pub(crate) fn handle(n: u64) -> String {
    format!("{n:032x}")
}

/// Files `count` are dragged over the page at `at`.
pub(crate) fn over_at(page: &mut Page, at: Point, count: usize) -> Result<(), Error> {
    page.page_mut().files_over(at, count);
    page.settle()
}

/// Files are dragged over the middle of the element named `name`, wherever what is over the page
/// (the mark of the place they would go to) lies: the pointer is not where a name is reached.
pub(crate) fn over_row(page: &mut Page, name: &str, count: usize) -> Result<(), Error> {
    let at = page
        .bounds(name)
        .ok_or_else(|| Error::NoSuch(name.to_owned()))?;
    over_at(
        page,
        Point::new(at.x + at.w / 2.0, at.y + at.h / 2.0),
        count,
    )
}

/// The files on the disk of the project's folder, parts and all.
pub(crate) fn left_in(project: &Project, dir: &str) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(project.root().join(dir))
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

fn media_type_of(name: &str) -> &'static str {
    match name.rsplit('.').next() {
        Some("png") => "image/png",
        Some("txt") => "text/plain",
        _ => "application/octet-stream",
    }
}

pub(crate) fn entry(n: u64, name: &str, bytes: &[u8], path: Option<&str>) -> FileEntry {
    FileEntry {
        handle: Some(handle(n)),
        name: name.to_owned(),
        size: bytes.len() as u64,
        media_type: media_type_of(name).to_owned(),
        path: path.map(str::to_owned),
        folder: false,
    }
}

/// Files dragged over the tree mark the folder they would go into and name it; a file's row is its
/// folder, the tree's empty room and the top of the project are the project's own folder; they
/// leave without a trace.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn files_dragged_over_the_tree_mark_the_folder_they_would_go_into_and_name_it() -> Result<(), Error>
{
    let project = lantern();
    let session = project.session(&[]);
    let mut page = project.open(&session, WIDE)?;
    page.click("row:src")?;
    crate::page_tests::wait(&mut page, |page| page.bounds("row:src/server.ts").is_some())?;

    // A folder's row is that folder.
    over_row(&mut page, "row:src", 2)?;
    assert!(shows(&page, "Upload to src/"), "{:?}", page_words(&page));
    assert!(
        page.bounds("drop-mark-tree").is_some(),
        "the place is named"
    );
    // A file's row is its folder.
    over_row(&mut page, "row:src/server.ts", 1)?;
    assert!(shows(&page, "Upload to src/"), "{:?}", page_words(&page));
    assert!(!shows(&page, "Upload to lantern/"));
    // A file at the top, and the room under the rows, are the project's own folder.
    over_row(&mut page, "row:README.md", 1)?;
    assert!(
        shows(&page, "Upload to lantern/"),
        "{:?}",
        page_words(&page)
    );
    over_row(&mut page, "row:src", 1)?;
    assert!(shows(&page, "Upload to src/"));
    let tree = page.bounds("tree").expect("the tree");
    over_at(
        &mut page,
        Point::new(tree.x + 20.0, tree.y + tree.h - 6.0),
        1,
    )?;
    assert!(
        shows(&page, "Upload to lantern/"),
        "{:?}",
        page_words(&page)
    );
    // They go, and nothing is left of it.
    page.drag_files_away()?;
    assert!(!shows(&page, "Upload to"), "{:?}", page_words(&page));
    assert!(page.bounds("drop-mark-tree").is_none());
    Ok(())
}

/// A file dropped on a folder is a faint row there at once, with how far it has got at its end; when
/// the last byte is in the tree lists the file and the row is the file's own. Where the program is
/// handed the file's path it copies the file from there, else the bytes come through the terminal.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_file_dropped_on_a_folder_is_a_faint_row_that_says_how_far_it_has_got_until_it_is_whole()
-> Result<(), Error> {
    let project = lantern();
    let session = project.session(&[]);
    let double = moving(&session);
    let mut page = open_quick(&session, WIDE)?;
    // The folder is closed: the drop opens it.
    let bytes: Vec<u8> = (0..100).collect();
    double.give(&handle(1), &bytes);
    *double.piece.lock().unwrap() = 21;
    double.hold_at(42);
    give(
        &mut page,
        "row:src",
        FileSource::Drop,
        &[("data.bin", &bytes)],
    )?;
    wait(&mut page, |page| shows(page, "42%"))?;
    assert!(
        page.bounds("row:src/data.bin").is_some(),
        "the row is in the folder, which is open: {:?}",
        page_words(&page)
    );
    assert!(shows(&page, "data.bin"));
    assert!(!project.exists("src/data.bin"), "not a file yet");
    // The part the bytes go to is on the disk, and is not a file of the tree.
    assert!(
        left_in(&project, "src")
            .iter()
            .any(|name| name.ends_with(".pencere-part")),
        "{:?}",
        left_in(&project, "src")
    );
    assert!(!shows(&page, "pencere-part"), "{:?}", page_words(&page));
    double.go_on();
    wait(&mut page, |_| project.exists("src/data.bin"))?;
    assert_eq!(
        std::fs::read(project.root().join("src/data.bin")).unwrap(),
        bytes
    );
    // The row is the file's own: no percent, and the tree lists it once.
    wait(&mut page, |page| !shows(page, "%"))?;
    assert!(page.bounds("row:src/data.bin").is_some());
    assert_eq!(double.routes(), [None], "through the terminal");
    assert_eq!(
        left_in(&project, "src")
            .iter()
            .filter(|n| n.contains("data.bin"))
            .count(),
        1
    );

    // A file of this machine, handed with its path, is copied from there.
    let source = project.root().parent().unwrap().join("elsewhere.txt");
    std::fs::write(&source, b"from the person's disk").unwrap();
    let given = entry(
        2,
        "elsewhere.txt",
        b"from the person's disk",
        Some(&source.to_string_lossy()),
    );
    let at = page.bounds("row:src").expect("the folder");
    page.page_mut().give(
        Point::new(at.x + 20.0, at.y + at.h / 2.0),
        FileSource::Drop,
        vec![given],
    );
    through(page.settle())?;
    wait(&mut page, |_| project.exists("src/elsewhere.txt"))?;
    assert_eq!(project.read("src/elsewhere.txt"), "from the person's disk");
    assert_eq!(
        double.routes().last().cloned().flatten().as_deref(),
        Some(source.to_str().unwrap()),
        "from the path"
    );
    Ok(())
}

/// A name that is taken is asked about once: Replace, or Keep Both (`foto 2.png`, then `foto 3.png`);
/// Esc gives the file up; nothing is written over a file unasked.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_name_that_is_taken_is_asked_about_once_and_nothing_is_written_over_unasked()
-> Result<(), Error> {
    let project = lantern();
    project.write("src/foto.png", "mine");
    let session = project.session(&[]);
    let double = moving(&session);
    let mut page = open_quick(&session, WIDE)?;
    click(&mut page, "row:src")?;
    wait(&mut page, |page| page.bounds("row:src/foto.png").is_some())?;

    // Esc: the file is not uploaded and nothing is asked of the terminal.
    double.give(&handle(1), b"theirs one");
    give(
        &mut page,
        "row:src",
        FileSource::Drop,
        &[("foto.png", b"theirs one")],
    )?;
    wait(&mut page, |page| shows(page, "Keep Both"))?;
    assert!(shows(&page, "Replace"), "{:?}", page_words(&page));
    assert!(
        !project.exists("src/foto 2.png"),
        "nothing is done before the answer"
    );
    through(page.press("escape"))?;
    wait(&mut page, |page| !shows(page, "Keep Both"))?;
    assert_eq!(project.read("src/foto.png"), "mine");
    assert!(double.routes().is_empty(), "no byte was asked for");
    assert!(
        page.bounds("row:src/foto.png").is_some(),
        "the file that is there stays"
    );

    // Keep Both: the new file has a name beside the old one.
    double.give(&handle(2), b"theirs two");
    give(
        &mut page,
        "row:src",
        FileSource::Drop,
        &[("foto.png", b"theirs two")],
    )?;
    wait(&mut page, |page| shows(page, "Keep Both"))?;
    click(&mut page, "dialog-action:2")?;
    wait(&mut page, |_| project.exists("src/foto 2.png"))?;
    assert_eq!(project.read("src/foto 2.png"), "theirs two");
    assert_eq!(project.read("src/foto.png"), "mine");

    // Again: the next free name, and Enter is Keep Both.
    double.give(&handle(3), b"theirs three");
    give(
        &mut page,
        "row:src",
        FileSource::Drop,
        &[("foto.png", b"theirs three")],
    )?;
    wait(&mut page, |page| shows(page, "Keep Both"))?;
    assert!(shows(&page, "foto 3.png"), "{:?}", page_words(&page));
    through(page.press("enter"))?;
    wait(&mut page, |_| project.exists("src/foto 3.png"))?;
    assert_eq!(project.read("src/foto 3.png"), "theirs three");

    // Replace: the old file is the new one.
    double.give(&handle(4), b"theirs four");
    give(
        &mut page,
        "row:src",
        FileSource::Drop,
        &[("foto.png", b"theirs four")],
    )?;
    wait(&mut page, |page| shows(page, "Replace"))?;
    click(&mut page, "dialog-action:1")?;
    wait(&mut page, |_| project.read("src/foto.png") == "theirs four")?;
    assert_eq!(
        project.read("src/foto 2.png"),
        "theirs two",
        "the others are left"
    );
    Ok(())
}

/// Cancel (on the row under the pointer) gives the file up: the row goes and no half file is left.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn cancel_on_the_row_gives_the_file_up_and_leaves_no_half_file() -> Result<(), Error> {
    let project = lantern();
    let session = project.session(&[]);
    let double = moving(&session);
    let mut page = open_quick(&session, WIDE)?;
    let bytes = vec![1_u8; 100];
    double.give(&handle(1), &bytes);
    *double.piece.lock().unwrap() = 10;
    double.hold_at(30);
    give(
        &mut page,
        "row:src",
        FileSource::Drop,
        &[("big.bin", &bytes)],
    )?;
    wait(&mut page, |page| shows(page, "30%"))?;
    // The button is there under the pointer only.
    assert!(!shows(&page, "Cancel"), "{:?}", page_words(&page));
    through(page.hover("row:src/big.bin"))?;
    wait(&mut page, |page| shows(page, "Cancel"))?;
    click(&mut page, "upload-cancel:1")?;
    wait(&mut page, |page| page.bounds("row:src/big.bin").is_none())?;
    double.go_on();
    wait(&mut page, |_| double.going() == 0)?;
    assert!(!project.exists("src/big.bin"));
    assert!(
        left_in(&project, "src")
            .iter()
            .all(|name| !name.contains("big")),
        "no part is left: {:?}",
        left_in(&project, "src")
    );
    Ok(())
}

/// A folder dropped is told about once and not uploaded; the files dropped with it are.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_folder_is_not_uploaded_and_the_files_dropped_with_it_are() -> Result<(), Error> {
    let project = lantern();
    let session = project.session(&[]);
    let double = moving(&session);
    let mut page = project.open(&session, WIDE)?;
    double.give(&handle(1), b"a file");
    let at = page.bounds("row:src").expect("a folder");
    let folder = |name: &str| FileEntry {
        handle: None,
        name: name.to_owned(),
        size: 0,
        media_type: String::new(),
        path: None,
        folder: true,
    };
    page.page_mut().give(
        Point::new(at.x + 20.0, at.y + at.h / 2.0),
        FileSource::Drop,
        vec![
            folder("photos"),
            entry(1, "one.txt", b"a file", None),
            folder("more"),
        ],
    );
    page.settle()?;
    crate::page_tests::wait(&mut page, |page| {
        shows(page, "Folders can't be uploaded yet")
    })?;
    crate::page_tests::wait(&mut page, |_| project.exists("src/one.txt"))?;
    assert_eq!(
        page_words(&page)
            .iter()
            .filter(|word| word.contains("Folders can't"))
            .count(),
        1,
        "told once"
    );
    assert!(!project.exists("src/photos") && !project.exists("src/more"));
    Ok(())
}

/// A picture dropped alone is shown in a tab from the file the person gave, before the program has
/// all of it; when it has, the tab shows the picture the program holds.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_picture_dropped_is_shown_from_the_file_given_before_it_is_whole() -> Result<(), Error> {
    let project = lantern();
    let session = project.session(&[]);
    let recorder = recording(&session);
    let double = moving(&session);
    let mut page = open_quick(&session, WIDE)?;
    let picture = png(64, 32, |x, y| [x as u8 * 3, y as u8 * 6, 90]);
    page.attach(&picture);
    double.give(&handle(1), &picture);
    *double.piece.lock().unwrap() = 40;
    double.hold_at(80);
    give(
        &mut page,
        "row:src",
        FileSource::Drop,
        &[("logo.png", &picture)],
    )?;
    wait(&mut page, |page| page.bounds("tab:src/logo.png").is_some())?;
    let drawn = format!("file:{}", handle(1));
    wait(&mut page, |page| {
        page.list()
            .items
            .iter()
            .any(|item| matches!(item, pencere_view::Paint::Image(image) if image.src == drawn))
    })?;
    assert!(shows(&page, "%"), "{:?}", page_words(&page));
    assert!(!project.exists("src/logo.png"));
    double.go_on();
    wait(&mut page, |_| project.exists("src/logo.png"))?;
    // The tab goes on to the picture the program holds.
    wait(&mut page, |page| {
        pictures_drawn(page)
            .iter()
            .any(|(name, _)| name.starts_with("sha256:"))
    })?;
    assert_eq!(recorder.attached().len(), 1);
    assert!(shown(&page).is_none(), "a picture, not text");
    Ok(())
}

/// An editor takes a file of this machine where it is, as it did; where the program is on another
/// machine (the file has no path) it is uploaded into the folder of the file shown, and the editor
/// says so as the file is dragged over it.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn an_editor_opens_a_file_of_this_machine_and_uploads_one_that_is_not() -> Result<(), Error> {
    let project = lantern();
    let session = project.session(&["src/server.ts"]);
    let double = moving(&session);
    let mut page = project.open(&session, WIDE)?;
    let editor = shown(&page).expect("an editor");
    let in_editor = |page: &Page| {
        let at = page.bounds(&editor).expect("the editor");
        Point::new(at.x + 60.0, at.y + 60.0)
    };

    // Here: nothing is said of uploading, and the file is opened.
    let at = in_editor(&page);
    over_at(&mut page, at, 1)?;
    assert!(!shows(&page, "Upload to"), "{:?}", page_words(&page));
    let outside = project.root().parent().unwrap().join("note.txt");
    std::fs::write(&outside, b"a note").unwrap();
    page.page_mut().give(
        at,
        FileSource::Drop,
        vec![entry(
            1,
            "note.txt",
            b"a note",
            Some(&outside.to_string_lossy()),
        )],
    );
    page.settle()?;
    let path: PathBuf = outside.clone();
    crate::page_tests::wait(&mut page, |page| {
        page.bounds(&format!("tab:{}", path.to_string_lossy()))
            .is_some()
    })?;
    assert!(double.routes().is_empty(), "opened where it is, not copied");

    // On another machine: named, and uploaded beside the file shown.
    session.pretend_remote(true);
    let mut page = project.open(&session, WIDE)?;
    let editor = shown(&page).expect("an editor");
    let at = {
        let at = page.bounds(&editor).expect("the editor");
        Point::new(at.x + 60.0, at.y + 60.0)
    };
    over_at(&mut page, at, 1)?;
    assert!(shows(&page, "Upload to src/"), "{:?}", page_words(&page));
    double.give(&handle(1), b"sent over");
    page.page_mut().give(
        at,
        FileSource::Drop,
        vec![entry(1, "sent.txt", b"sent over", None)],
    );
    page.settle()?;
    crate::page_tests::wait(&mut page, |_| project.exists("src/sent.txt"))?;
    assert_eq!(project.read("src/sent.txt"), "sent over");
    assert!(!shows(&page, "Upload to"));
    Ok(())
}

/// The right-click menu of a folder's row and the palette's command, both "Upload Files" (no dots),
/// ask for the Open panel; what is chosen in it goes into the folder that was chosen in the tree.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn upload_files_asks_for_the_open_panel_from_the_trees_menu_and_the_palette() -> Result<(), Error> {
    let project = lantern();
    let session = project.session(&[]);
    let double = moving(&session);
    let mut page = project.open(&session, WIDE)?;
    crate::page_tests::wait(&mut page, |page| page.bounds("row:src").is_some())?;

    // The folder's menu: the person's move, so the panel is up, for more than one file.
    page.right_click("row:src")?;
    assert!(
        page.menu().is_some_and(|menu| {
            let titles: Vec<String> = menu
                .entries
                .iter()
                .filter_map(|entry| match entry {
                    pencere_view::MenuEntry::Item { title, .. } => Some(title.clone()),
                    _ => None,
                })
                .collect();
            titles.iter().any(|title| title == "Upload Files")
                && titles.iter().all(|title| !title.contains("Download"))
        }),
        "{:?}",
        page.menu()
    );
    page.pick_titled("Upload Files")?;
    assert_eq!(page.panel(), Some(&Panel::Pick { multiple: true }));
    double.give(&handle(1), b"through the menu");
    page.choose_files(&[("menu.txt", b"through the menu")])?;
    crate::page_tests::wait(&mut page, |_| project.exists("src/menu.txt"))?;
    assert_eq!(project.read("src/menu.txt"), "through the menu");

    // The palette: the folder chosen in the tree is where they go.
    crate::page_tests::wait(&mut page, |page| page.bounds("row:src/lib").is_some())?;
    page.click("row:src/lib")?;
    crate::page_tests::wait(&mut page, |page| {
        page.bounds("row:src/lib/format.ts").is_some()
    })?;
    page.press("f1")?;
    page.type_text("Upload Files")?;
    assert!(shows(&page, "Upload Files"), "{:?}", page_words(&page));
    page.press("enter")?;
    assert_eq!(page.panel(), Some(&Panel::Pick { multiple: true }));
    double.give(&handle(2), b"through the palette");
    page.choose_files(&[("palette.txt", b"through the palette")])?;
    crate::page_tests::wait(&mut page, |_| project.exists("src/lib/palette.txt"))?;
    assert_eq!(project.read("src/lib/palette.txt"), "through the palette");

    // Turned away: nothing goes, and nothing is left asked.
    page.right_click("row:src")?;
    page.pick_titled("Upload Files")?;
    page.choose_files(&[])?;
    page.advance(0.5)?;
    assert!(!project.exists("src/lib/menu.txt") && !project.exists("src/other.txt"));
    assert!(page.bounds("upload-cancel:3").is_none());
    Ok(())
}

/// A file's menu and the palette say "Download" (no dots), and ask for the Save panel with the file's
/// name; the file is written where the person chose, and a toast says so. A folder has none.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn download_asks_for_the_save_panel_with_the_files_name_and_says_when_it_is_done()
-> Result<(), Error> {
    let project = lantern();
    let session = project.session(&[]);
    let double = moving(&session);
    let mut page = project.open(&session, WIDE)?;
    crate::page_tests::wait(&mut page, |page| page.bounds("row:README.md").is_some())?;

    // A file's menu.
    page.right_click("row:README.md")?;
    page.pick_titled("Download")?;
    assert_eq!(
        page.panel(),
        Some(&Panel::Save {
            name: "README.md".to_owned(),
            media_type: None
        })
    );
    let place = page.choose_place(Some("README copy.md"))?.expect("a place");
    crate::page_tests::wait(&mut page, |page| shows(page, "Downloaded README copy.md"))?;
    assert_eq!(
        double.saved.lock().unwrap().get(&place).cloned(),
        Some(project.read("README.md").into_bytes()),
        "what the program sent is the file"
    );

    // The palette, for the file chosen in the tree; turned away, nothing is written.
    page.click("row:README.md")?;
    page.press("f1")?;
    page.type_text("Download")?;
    page.press("enter")?;
    assert!(matches!(page.panel(), Some(Panel::Save { name, .. }) if name == "README.md"));
    let saved_before = double.saved.lock().unwrap().len();
    assert_eq!(page.choose_place(None)?, None);
    page.advance(0.5)?;
    assert_eq!(double.saved.lock().unwrap().len(), saved_before);

    // A folder has no Download, and a file has no Upload Files, in the palette.
    page.click("row:src")?;
    page.press("f1")?;
    page.type_text("Download")?;
    assert!(
        !page_words(&page).iter().any(|word| word == "Download"),
        "{:?}",
        page_words(&page)
    );
    page.press("escape")?;
    page.click("row:README.md")?;
    page.press("f1")?;
    page.type_text("Upload Files")?;
    assert!(
        !page_words(&page).iter().any(|word| word == "Upload Files"),
        "{:?}",
        page_words(&page)
    );
    Ok(())
}

/// ⌘V with a picture on the pasteboard, in a Markdown file: the picture goes beside the file as
/// `image.png` (then `image-1.png`, `image-2.png`), and `![](image.png)` is written where the caret
/// is; the preview holds a place for it until it is whole, then draws it. A pasted picture goes
/// nowhere else.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_picture_pasted_in_a_markdown_file_goes_beside_it_and_is_written_into_it() -> Result<(), Error>
{
    let project = lantern();
    project.write("docs/guide.md", "# Guide\n\nThe lantern:\n");
    let session = project.session(&["README.md", "docs/guide.md", "src/server.ts"]);
    let recorder = recording(&session);
    let double = moving(&session);
    let mut page = open_quick(&session, WIDE)?;
    let picture = png(40, 20, |x, y| [x as u8 * 5, y as u8 * 10, 60]);
    page.attach(&picture);
    let paste = |page: &mut Page, n: u64, name: &str, bytes: &[u8]| -> Result<(), Error> {
        double.give(&handle(n), bytes);
        let editor = shown(page).expect("an editor");
        let at = page.bounds(&editor).expect("the editor");
        page.page_mut().give(
            Point::new(at.x + 60.0, at.y + 60.0),
            FileSource::Paste,
            vec![entry(n, name, bytes, None)],
        );
        through(page.settle())
    };

    // README.md, the first in front: the caret is at the start; the picture is named for what it is.
    paste(&mut page, 1, "Screenshot 2026-10-31.png", &picture)?;
    wait(&mut page, |page| {
        crate::page_tests::text(page).starts_with("![](image.png)")
    })?;
    wait(&mut page, |_| project.exists("image.png"))?;
    assert_eq!(
        std::fs::read(project.root().join("image.png")).unwrap(),
        picture
    );
    // The name is taken: the next, and the next.
    paste(&mut page, 2, "image.png", &picture)?;
    wait(&mut page, |_| project.exists("image-1.png"))?;
    paste(&mut page, 3, "image.png", &picture)?;
    wait(&mut page, |_| project.exists("image-2.png"))?;
    wait(&mut page, |page| {
        let text = crate::page_tests::text(page);
        text.contains("![](image-1.png)") && text.contains("![](image-2.png)")
    })?;
    assert!(
        crate::page_tests::text(&page).contains("![](image.png)"),
        "the first is still there: {}",
        crate::page_tests::text(&page)
    );

    // A picture pasted in a file that is no Markdown goes nowhere, and no file is made.
    page.press("ctrl+3")?;
    wait(&mut page, |page| {
        crate::page_tests::text(page).starts_with("import { createServer }")
    })?;
    paste(&mut page, 4, "image.png", &picture)?;
    page.advance(0.5).ok();
    assert!(!project.exists("src/image.png") && !project.exists("image-3.png"));

    // A Markdown file in a folder: beside it, and the preview holds a place until it is whole.
    page.press("ctrl+2")?;
    wait(&mut page, |page| {
        crate::page_tests::text(page).starts_with("# Guide")
    })?;
    page.press("cmd+shift+v")?;
    double.hold_at(100);
    *double.piece.lock().unwrap() = 50;
    paste(&mut page, 5, "image.png", &picture)?;
    wait(&mut page, |page| {
        page.bounds("upload-placeholder:4").is_some()
    })?;
    assert!(
        crate::page_tests::text(&page).contains("![](image.png)"),
        "{}",
        crate::page_tests::text(&page)
    );
    assert!(!project.exists("docs/image.png"), "not whole yet");
    double.go_on();
    wait(&mut page, |_| project.exists("docs/image.png"))?;
    wait(&mut page, |page| {
        page.bounds("upload-placeholder:4").is_none()
            && pictures_drawn(page)
                .iter()
                .any(|(name, _)| name.starts_with("sha256:"))
    })?;
    assert!(!recorder.attached().is_empty());
    Ok(())
}
