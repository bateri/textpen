//! The page as `pencere-tui` runs it where no terminal hosts it: drawn in pixels for a terminal that speaks
//! the Kitty graphics protocol, on a keyboard that is not a Mac's (Ctrl is the main modifier, there is no
//! ⌘). What the keys of a Mac do, the keys of Linux do, and what a terminal cannot send is not a key of
//! the page.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pencere_drive::headless::{Error, Headless, InProcess};
use pencere_view::{Drawing, Key, KeyConvention, Shortcut};

use super::{spec_pdf, with_changed_server, with_git};
use crate::options;
use crate::page_pdf::pages_drawn;
use crate::page_tests::{
    Page, Project, go_to, lantern, limits, page_words, pictures_drawn, recording, shown, text, wait,
};

const WITHIN: Duration = Duration::from_secs(20);

/// The windows a terminal of 80 by 24 cells and one of 160 by 48 give the page, points.
const SIZES: [(f32, f32); 2] = [(640.0, 432.0), (1280.0, 864.0)];

/// The page of a terminal of the wider size, with the files it was given to attach.
fn open(project: &Project, tabs: &[&str]) -> Result<Page, Error> {
    let session = project.session(tabs);
    let program = InProcess::new(options(&session, &[], &Arc::new(Mutex::new(None))));
    let mut page = open_in_pixels(program, SIZES[1], KeyConvention::Linux, &[])?;
    wait(&mut page, |page| shown(page).is_some() || tabs.is_empty())?;
    page.advance(1.0)?;
    Ok(page)
}

/// The page of a window of `width` by `height` points on the keyboard `keyboard`, drawn in pixels: what
/// a terminal that draws pictures tells it (no cells, and the keyboard of a Linux or a Mac).
fn open_in_pixels(
    program: InProcess,
    (width, height): (f32, f32),
    keyboard: KeyConvention,
    files: &[&[u8]],
) -> Result<Page, Error> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |since| since.as_secs_f64());
    let mut opening = Headless::app(env!("CARGO_MANIFEST_DIR"))
        .size(width, height)
        .keyboard(keyboard)
        .draws(vec![Drawing::Shadow])
        .wall_clock(now, 0)
        .limits(limits(None));
    for bytes in files {
        opening = opening.attached(bytes);
    }
    let mut page = opening.peer(program).open()?;
    page.wait_for(WITHIN, |page| {
        shown(page).is_some() || !pictures_drawn(page).is_empty()
    })?;
    page.advance(0.5)?;
    Ok(page)
}

/// What the editor's menu at the caret says the keys of the line `title` are.
fn editor_menu_keys(page: &mut Page, title: &str) -> Result<Option<Shortcut>, Error> {
    let editor = shown(page).expect("an editor");
    page.caret_to(&editor, 2)?;
    page.press("shift+f10")?;
    let keys = page.menu().and_then(|menu| {
        menu.entries.iter().find_map(|entry| match entry {
            pencere_view::MenuEntry::Item {
                title: line, keys, ..
            } if line == title => Some(*keys),
            _ => None,
        })
    });
    page.press("escape")?;
    Ok(keys.flatten())
}

/// The comment is ⌘/ where the keyboard has a ⌘ and Ctrl+/ where it has none: the editor's menu names the
/// key the keyboard has, and the key does it. A terminal that draws the page in pixels is the second (it
/// sends ⌃⇧ and a character for ⌘ and a character, and ⌃⇧/ is the page's own for the help).
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_comment_is_ctrl_slash_on_a_keyboard_with_no_command_key_and_command_slash_on_a_mac()
-> Result<(), Error> {
    for (keyboard, menu, key) in [
        (KeyConvention::Linux, "ctrl+/", "ctrl+/"),
        (KeyConvention::Mac, "cmd+/", "cmd+/"),
    ] {
        let project = lantern();
        let session = project.session(&["src/lib/format.ts"]);
        let program = InProcess::new(options(&session, &[], &Arc::new(Mutex::new(None))));
        let mut page = open_in_pixels(program, (1200.0, 800.0), keyboard, &[])?;
        let says = editor_menu_keys(&mut page, "Toggle Line Comment")?;
        assert_eq!(
            says,
            Shortcut::parse(menu),
            "{keyboard:?}: the menu's Toggle Line Comment"
        );
        let editor = shown(&page).expect("an editor");
        page.caret_to(&editor, 0)?;
        page.press(key)?;
        page.advance(0.5)?;
        assert!(
            text(&page).starts_with("// const currency"),
            "{keyboard:?}: {:?}",
            text(&page)
        );
        page.press(key)?;
        page.advance(0.5)?;
        assert!(
            text(&page).starts_with("const currency"),
            "{keyboard:?}: {:?}",
            text(&page)
        );
    }
    Ok(())
}

/// The hint of the button that fits a picture to its tab names the key that does it on the keyboard the
/// page is on: Ctrl+0 where there is no ⌘ (and the terminal's pixels are that), ⌘0 on a Mac. It was the
/// ⌘ key written for every keyboard, which a Linux one has as "Super" and a terminal never sends.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_hint_of_the_fit_button_names_the_key_the_keyboard_has() -> Result<(), Error> {
    let art = super::lantern_art();
    for (keyboard, hint) in [
        (KeyConvention::Linux, "Fit to the tab Ctrl+0"),
        (KeyConvention::Mac, "Fit to the tab ⌘0"),
    ] {
        let project = lantern();
        project.write_bytes("assets/lantern.png", &art);
        let session = project.session(&["assets/lantern.png"]);
        recording(&session);
        let program = InProcess::new(options(&session, &[], &Arc::new(Mutex::new(None))));
        let mut page = open_in_pixels(program, (1200.0, 800.0), keyboard, &[&art])?;
        page.advance(1.5)?;
        let tips: Vec<_> = page.outline().tips.iter().map(|tip| &tip.words).collect();
        assert!(
            tips.iter().any(|words| *words == hint),
            "{keyboard:?}: {hint:?} in {tips:?}"
        );
    }
    Ok(())
}

/// A file is typed into, saved, undone, found in and gone between by the keys of a Linux keyboard
/// (Ctrl is what ⌘ is on a Mac, ⌥ with a number is a Mac's ⌃ with one), in a small window and a wide one.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_linux_keyboard_edits_saves_goes_to_files_finds_and_changes_tabs_with_ctrl() -> Result<(), Error>
{
    for size in SIZES {
        let project = lantern();
        let session = project.session(&["src/server.ts", "README.md"]);
        let program = InProcess::new(options(&session, &[], &Arc::new(Mutex::new(None))));
        let mut page = open_in_pixels(program, size, KeyConvention::Linux, &[])?;
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
        // The sheet of keys names the keys as a Linux keyboard writes them, and it is Ctrl+Shift+/ where the
        // terminal sends the key so, and Ctrl+? where it says the character the keys make (a US keyboard).
        for help in ["ctrl+shift+/", "ctrl+?"] {
            page.press(help)?;
            page.advance(0.5)?;
            let words = page_words(&page);
            assert!(
                words.iter().any(|word| word.contains("Ctrl+")),
                "{size:?} {help}: {words:?}"
            );
            assert!(
                words
                    .iter()
                    .all(|word| !word.contains('⌘') && !word.contains('⌥') && !word.contains('⇧')),
                "{size:?} {help}: {words:?}"
            );
            page.press("escape")?;
            page.advance(0.3)?;
        }
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
    )?;
    go_to(&mut page, "src/server.ts:19:5")?;
    states.push(("the editor".to_owned(), page));
    let project_two = lantern();
    with_git(&project_two);
    let mut page = open(&project_two, &["src/server.ts", "README.md"])?;
    page.press("ctrl+f")?;
    page.type_text("order")?;
    page.advance(0.4)?;
    page.press("ctrl+alt+f")?;
    page.advance(0.4)?;
    states.push(("the find bar with the replacement".to_owned(), page));
    let project_three = lantern();
    with_changed_server(&project_three);
    let mut page = open(&project_three, &["src/server.ts"])?;
    page.press("ctrl+shift+d")?;
    page.advance(1.5)?;
    states.push(("the comparison".to_owned(), page));
    let project_four = lantern();
    project_four.write_bytes("docs/spec.pdf", &spec_pdf());
    let session = project_four.session(&["docs/spec.pdf", "README.md"]);
    recording(&session);
    let program = InProcess::new(options(&session, &[], &Arc::new(Mutex::new(None))));
    let mut page = open_in_pixels(program, SIZES[1], KeyConvention::Linux, &[&spec_pdf()])?;
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
