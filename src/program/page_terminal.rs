//! The page as `pencere-tui` runs it where no terminal hosts it: in the cells of a plain terminal, on a
//! keyboard that is not a Mac's (Ctrl is the main modifier, there is no ⌘) and, where the terminal has
//! no graphics protocol, with pictures in half blocks. What the keys of a Mac do, the keys of Linux
//! do; what a terminal cannot send is not a key of the page; and a PDF is drawn where the terminal
//! draws pictures and is a sentence where it does not.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pencere_drive::headless::{Error, Headless, InProcess};
use pencere_tui::Cells;
use pencere_view::{Drawing, Key, KeyConvention, Shortcut};

use super::{spec_pdf, with_changed_server, with_git};
use crate::options;
use crate::page_pdf::pages_drawn;
use crate::page_tests::{
    Page, Project, go_to, lantern, limits, page_words, recording, shown, shows, text, wait,
};

const WITHIN: Duration = Duration::from_secs(20);

/// A column of the cells the page is laid out on, points, and a row.
const COLUMN: f32 = 8.0;
const ROW: f32 = 18.0;

/// The page of a terminal of `cols` by `rows` that hosts nothing, on a Linux keyboard: with the
/// graphics protocol (`kitty`) or without it (the page told so, as the screen tells it).
fn open_on(
    program: InProcess,
    (cols, rows): (u16, u16),
    kitty: bool,
    files: &[&[u8]],
) -> Result<Page, Error> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |since| since.as_secs_f64());
    let mut opening = Headless::app(env!("CARGO_MANIFEST_DIR"))
        .size(f32::from(cols) * COLUMN, f32::from(rows) * ROW)
        .measure(Cells::new(COLUMN, ROW))
        .keyboard(KeyConvention::Linux)
        .draws(if kitty {
            vec![Drawing::Cells]
        } else {
            vec![Drawing::Cells, Drawing::CoarseImages]
        })
        .wall_clock(now, 0)
        .limits(limits(None));
    for bytes in files {
        opening = opening.attached(bytes);
    }
    let mut page = opening.peer(program).open()?;
    page.wait_for(WITHIN, |page| {
        shown(page).is_some()
            || shows(page, "Go to File")
            || shows(page, "This terminal does not show PDFs")
            || !pages_drawn(page).is_empty()
    })?;
    page.advance(0.5)?;
    Ok(page)
}

fn open(project: &Project, tabs: &[&str], size: (u16, u16)) -> Result<Page, Error> {
    let session = project.session(tabs);
    let program = InProcess::new(options(&session, &[], &Arc::new(Mutex::new(None))));
    let mut page = open_on(program, size, true, &[])?;
    wait(&mut page, |page| shown(page).is_some() || tabs.is_empty())?;
    page.advance(1.0)?;
    Ok(page)
}

const SIZES: [(u16, u16); 2] = [(80, 24), (160, 48)];

/// A file is typed into, saved, undone, found in and gone between by the keys of a Linux keyboard
/// (Ctrl is what ⌘ is on a Mac, ⌥ with a number is a Mac's ⌃ with one), at 80×24 and wide.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_linux_keyboard_edits_saves_goes_to_files_finds_and_changes_tabs_with_ctrl() -> Result<(), Error>
{
    for size in SIZES {
        let project = lantern();
        let session = project.session(&["src/server.ts", "README.md"]);
        let program = InProcess::new(options(&session, &[], &Arc::new(Mutex::new(None))));
        let mut page = open_on(program, size, true, &[])?;
        wait(&mut page, |page| shown(page).is_some())?;
        let before = text(&page);
        let editor = shown(&page).expect("an editor");
        page.caret_to(&editor, 0)?;
        page.type_text("héllo ")?;
        page.advance(0.5)?;
        assert!(text(&page).starts_with("héllo "), "{size:?}");
        // Save: Ctrl+S.
        page.press("ctrl+s")?;
        wait(&mut page, |_| {
            project.read("src/server.ts").starts_with("héllo ")
        })?;
        // Undo is the editor's own Ctrl+Z, and Redo its Ctrl+Y (a Mac's ⇧⌘Z is Ctrl+Z to a terminal
        // that cannot send Shift with Ctrl).
        page.press("ctrl+z")?;
        page.advance(0.5)?;
        assert_eq!(text(&page), before, "{size:?}");
        page.press("ctrl+y")?;
        page.advance(0.5)?;
        assert!(text(&page).starts_with("héllo "), "{size:?}");
        page.press("ctrl+z")?;
        page.advance(0.5)?;
        page.press("ctrl+s")?;
        wait(&mut page, |_| project.read("src/server.ts") == before)?;
        // The palette: Ctrl+K lists the files, Esc comes back.
        page.press("ctrl+k")?;
        page.advance(0.5)?;
        assert!(
            page.bounds("palette-query").is_some(),
            "{size:?} {:?}",
            page_words(&page)
        );
        page.type_text("README")?;
        page.advance(0.5)?;
        page.press("enter")?;
        wait(&mut page, |page| text(page).starts_with("# Lantern"))?;
        // A tab by its number: Alt+1 (the first, server.ts), Alt+2.
        page.press("alt+1")?;
        wait(&mut page, |page| text(page) == before)?;
        page.press("alt+2")?;
        wait(&mut page, |page| text(page).starts_with("# Lantern"))?;
        // Find: Ctrl+F opens the bar, the options go with Alt (Alt+C case).
        page.press("ctrl+f")?;
        page.type_text("lantern")?;
        page.advance(0.5)?;
        assert!(page.bounds("find-bar").is_some(), "{size:?}");
        page.press("alt+c")?;
        page.advance(0.3)?;
        page.press("escape")?;
        // The comment is the editor's own, Ctrl+/ (and what a terminal sends for it, Ctrl+_).
        page.press("alt+1")?;
        wait(&mut page, |page| text(page) == before)?;
        page.caret_to(&editor, 0)?;
        page.press("ctrl+/")?;
        page.advance(0.5)?;
        assert!(
            text(&page).starts_with("//"),
            "{size:?}: {:?}",
            &text(&page)[..20.min(text(&page).len())]
        );
        page.press("ctrl+_")?;
        page.advance(0.5)?;
        assert_eq!(text(&page), before, "{size:?}");
        // Settings and the sheet of keys name the keys as a Linux keyboard writes them.
        page.press("ctrl+shift+/")?;
        page.advance(0.5)?;
        let words = page_words(&page);
        assert!(
            words.iter().any(|word| word.contains("Ctrl+")),
            "{size:?}: {words:?}"
        );
        assert!(
            words
                .iter()
                .all(|word| !word.contains('⌘') && !word.contains('⌥') && !word.contains('⇧')),
            "{size:?}: {words:?}"
        );
    }
    Ok(())
}

/// The keys the page gives its commands on a Linux keyboard (the page's own, by `mod`): in each of the
/// states it can be in, what is done with one is one command, and not one is a key a terminal cannot
/// tell from another (Ctrl+C is the interrupt, Ctrl+I is Tab, Ctrl+J and Ctrl+M are Enter, Ctrl+H is
/// Backspace, Ctrl+[ is Esc), and no command is given ⌃⇧Z or ⌃⌥C or ⌃⌥Z, which a terminal with no Kitty
/// keyboard protocol sends as ⌃Z and ⌃C.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_keys_of_the_commands_are_one_for_each_and_none_a_terminal_cannot_send_on_a_linux_keyboard()
-> Result<(), Error> {
    let mut states: Vec<(String, Page)> = Vec::new();

    let project = lantern();
    with_git(&project);
    let mut page = open(
        &project,
        &["src/server.ts", "src/lib/format.ts", "README.md"],
        (160, 48),
    )?;
    go_to(&mut page, "src/server.ts:19:5")?;
    states.push(("the editor".to_owned(), page));
    let project_two = lantern();
    with_git(&project_two);
    let mut page = open(&project_two, &["src/server.ts", "README.md"], (160, 48))?;
    page.press("ctrl+f")?;
    page.type_text("order")?;
    page.advance(0.4)?;
    page.press("ctrl+alt+f")?;
    page.advance(0.4)?;
    states.push(("the find bar with the replacement".to_owned(), page));
    let project_three = lantern();
    with_changed_server(&project_three);
    let mut page = open(&project_three, &["src/server.ts"], (160, 48))?;
    page.press("ctrl+shift+d")?;
    page.advance(1.5)?;
    states.push(("the comparison".to_owned(), page));
    let project_four = lantern();
    project_four.write_bytes("docs/spec.pdf", &spec_pdf());
    let session = project_four.session(&["docs/spec.pdf", "README.md"]);
    recording(&session);
    let program = InProcess::new(options(&session, &[], &Arc::new(Mutex::new(None))));
    let mut page = open_on(program, (160, 48), true, &[&spec_pdf()])?;
    wait(&mut page, |page| !pages_drawn(page).is_empty())?;
    page.press("ctrl+f")?;
    page.type_text("attach")?;
    page.advance(1.0)?;
    states.push(("a PDF being searched".to_owned(), page));

    let blind = |shortcut: &Shortcut| -> Option<&'static str> {
        let (mods, key) = (shortcut.mods, shortcut.key);
        if !mods.control || mods.command {
            return None;
        }
        let letter = match key {
            Key::Character(c) => Some(c.to_ascii_lowercase()),
            _ => None,
        };
        match (letter, mods.shift, mods.alt) {
            (Some('c'), false, false) => Some("the interrupt"),
            (Some('i'), false, false) => Some("Tab"),
            (Some('j' | 'm'), false, false) => Some("Enter"),
            (Some('h'), false, false) => Some("Backspace"),
            (Some('['), false, false) => Some("Esc"),
            (Some('z' | 'c'), _, true) => {
                Some("Ctrl+Z or Ctrl+C, the keys a terminal sends for Ctrl with Alt")
            }
            (Some('z'), true, _) => Some("Ctrl+Z, the key a terminal sends for Ctrl with Shift"),
            _ => None,
        }
    };

    let mut found: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut commands_seen = 0;
    for (state, page) in &states {
        let laid = page.laid().expect("a page");
        let commands = laid.commands();
        commands_seen += commands.len();
        let mut by_key: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for command in &commands {
            for shortcut in &command.keys {
                if let Some(why) = blind(shortcut) {
                    found
                        .entry(format!("{:?} is {why}", shortcut))
                        .or_default()
                        .push(command.title.clone());
                }
                if command.enabled {
                    by_key
                        .entry(format!("{shortcut:?}"))
                        .or_default()
                        .push(command.title.clone());
                }
            }
        }
        for (key, owners) in by_key {
            if owners.len() > 1 {
                found
                    .entry(format!("{state}: {key} does {}", owners.join(" and ")))
                    .or_default();
            }
        }
    }
    assert!(
        commands_seen > 100,
        "only {commands_seen} commands were looked at"
    );
    assert!(found.is_empty(), "{found:#?}");
    Ok(())
}

/// A PDF is drawn page by page where the terminal has a graphics protocol, and where it has none the tab
/// says it shows no PDFs, in the words it always has, and the file is not asked for.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_pdf_is_drawn_where_the_terminal_has_graphics_and_is_a_sentence_where_it_has_none()
-> Result<(), Error> {
    for size in SIZES {
        let bytes = spec_pdf();
        let project = lantern();
        project.write_bytes("docs/spec.pdf", &bytes);
        let session = project.session(&["docs/spec.pdf", "README.md"]);
        let recorder = recording(&session);
        let program = InProcess::new(options(&session, &[], &Arc::new(Mutex::new(None))));
        let mut page = open_on(program, size, true, &[&bytes])?;
        wait(&mut page, |page| !pages_drawn(page).is_empty())?;
        assert!(
            !shows(&page, "This terminal does not show PDFs"),
            "{size:?}"
        );
        assert!(!recorder.attached().is_empty(), "{size:?}");
        drop(page);

        let project = lantern();
        project.write_bytes("docs/spec.pdf", &bytes);
        let session = project.session(&["docs/spec.pdf", "README.md"]);
        let recorder = recording(&session);
        let program = InProcess::new(options(&session, &[], &Arc::new(Mutex::new(None))));
        let mut page = open_on(program, size, false, &[&bytes])?;
        wait(&mut page, |page| {
            shows(page, "This terminal does not show PDFs")
        })?;
        assert!(pages_drawn(&page).is_empty(), "{size:?}");
        assert!(recorder.attached().is_empty(), "{size:?}");
    }
    Ok(())
}

/// Where a picture is drawn in half blocks the buttons that are pictures are characters: the arrows of
/// the find bar, its close button, and the marks of the tabs and the tree's folders are signs a person
/// can read, and the busy mark is the three dots, not a picture that stands still.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn where_pictures_are_coarse_the_buttons_say_what_they_do_in_characters() -> Result<(), Error> {
    let project = lantern();
    let session = project.session(&["src/server.ts"]);
    let program = InProcess::new(options(&session, &[], &Arc::new(Mutex::new(None))));
    let mut page = open_on(program, (160, 48), false, &[])?;
    wait(&mut page, |page| shown(page).is_some())?;
    page.press("ctrl+f")?;
    page.type_text("order")?;
    page.advance(0.5)?;
    let words = page_words(&page);
    for sign in ["↑", "↓", "×"] {
        assert!(words.iter().any(|word| word == sign), "{sign} in {words:?}");
    }
    Ok(())
}

/// The editor's right-click menu names the keys of a keyboard that has no ⌘: not one of its lines
/// says Super (the logo key, which a terminal never sends), and Cut, Copy and Paste say none rather than
/// say another's.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_editors_menu_names_no_key_a_terminal_cannot_send_on_a_linux_keyboard() -> Result<(), Error> {
    let project = lantern();
    let session = project.session(&["src/server.ts"]);
    let program = InProcess::new(options(&session, &[], &Arc::new(Mutex::new(None))));
    let mut page = open_on(program, (160, 48), true, &[])?;
    wait(&mut page, |page| shown(page).is_some())?;
    let editor = shown(&page).expect("an editor");
    page.caret_to(&editor, 2)?;
    page.press("shift+f10")?;
    let menu = page.menu().expect("a menu at the caret");
    let mut seen = 0;
    for entry in &menu.entries {
        if let pencere_view::MenuEntry::Item {
            title,
            keys: Some(keys),
            ..
        } = entry
        {
            seen += 1;
            assert!(
                !keys.mods.command,
                "{title:?} says {keys:?}, a key with the logo key in it"
            );
        }
    }
    assert!(seen > 0, "{:?}", menu.entries);
    Ok(())
}

/// The Open and Save panels of a terminal that hosts nothing take a path of the machine the program is on,
/// and the program has no way to read what they name or to write where they say: the page does not
/// offer to upload to the project or to download from it (the menu of a row, the palette).
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_persons_files_are_not_offered_in_the_cells_of_a_terminal_that_hosts_nothing()
-> Result<(), Error> {
    let project = lantern();
    let mut page = open(&project, &[], (160, 48))?;
    wait(&mut page, |page| page.bounds("row:src").is_some())?;
    let titles = |page: &Page| -> Vec<String> {
        page.menu()
            .map(|menu| {
                menu.entries
                    .iter()
                    .filter_map(|entry| match entry {
                        pencere_view::MenuEntry::Item { title, .. } => Some(title.clone()),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    // A folder's menu has its own entries, and not the person's files'.
    page.right_click("row:src")?;
    let folder = titles(&page);
    assert!(folder.iter().any(|title| title == "New File"), "{folder:?}");
    assert!(
        folder.iter().all(|title| !title.contains("Upload")),
        "{folder:?}"
    );
    page.press("escape")?;
    // A file's, the same.
    page.click("row:src")?;
    wait(&mut page, |page| page.bounds("row:src/server.ts").is_some())?;
    page.right_click("row:src/server.ts")?;
    let file = titles(&page);
    assert!(file.iter().any(|title| title == "Rename"), "{file:?}");
    assert!(
        file.iter().all(|title| !title.contains("Download")),
        "{file:?}"
    );
    page.press("escape")?;
    // And the palette lists a command of the page (it is there to be found) but neither of these.
    page.press("f1")?;
    page.type_text("Upload")?;
    page.advance(0.5)?;
    assert!(!shows(&page, "Upload Files"), "{:?}", page_words(&page));
    page.type_text("")?;
    Ok(())
}
