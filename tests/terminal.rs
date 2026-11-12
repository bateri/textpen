//! textpen in a terminal that hosts nothing: it draws the editor with the characters of the terminal it is
//! started in, takes the keys, saves what is typed, and gives the terminal back as it was. The terminal is a
//! pty (`common::terminal`) that answers what a terminal answers and shows what is written to it.

// The screen is the `terminal` feature's: the build without it has none to start.
#![cfg(feature = "terminal")]
// A test that cannot go on fails where it stands, helpers included.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::{Duration, Instant};

use common::terminal::{Start, Terminal, count, plain};
use common::{Pane, pdf_of_pages, program};

const LIMIT: Duration = Duration::from_secs(60);

/// A project with `a.txt` in it and a PDF, and textpen run on `file` in a terminal of 100 by 30 that
/// hosts nothing (and draws pictures where `graphics`).
fn start(pane: &Pane, file: &str, graphics: bool, extra: &[(&str, &str)]) -> Terminal {
    let home = pane.dir.path().to_string_lossy().into_owned();
    let state = pane.path("state").to_string_lossy().into_owned();
    let config = pane.path("config").to_string_lossy().into_owned();
    let mut env = vec![
        ("HOME", home.as_str()),
        ("XDG_STATE_HOME", state.as_str()),
        ("XDG_CONFIG_HOME", config.as_str()),
        ("SHELL", "/usr/bin/true"),
        ("TERM", "xterm-256color"),
        ("COLORTERM", "truecolor"),
    ];
    env.extend_from_slice(extra);
    Terminal::start(&Start {
        program: program(),
        args: &[file],
        dir: pane.dir.path(),
        env: &env,
        env_remove: &[
            "SSH_CONNECTION",
            "SSH_TTY",
            "PENCERE_TUI",
            "VISUAL",
            "EDITOR",
        ],
        size: (100, 30),
        graphics,
    })
}

/// Waits until `what` is true of the file `name`, which the program writes.
fn file_becomes(pane: &Pane, name: &str, what: impl Fn(&str) -> bool) -> bool {
    let end = Instant::now() + LIMIT;
    while Instant::now() < end {
        if std::fs::read_to_string(pane.path(name)).is_ok_and(|text| what(&text)) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

#[test]
fn a_terminal_that_hosts_nothing_gets_the_editor_drawn_in_its_cells_and_is_the_persons_again() {
    let pane = Pane::new();
    pane.write("a.txt", "world\n");
    pane.write("b.txt", "other\n");
    // No host answers, and nothing asks for the screen: the program waits for a host a moment, and draws.
    let mut terminal = start(&pane, "a.txt", false, &[]);
    assert!(terminal.until("world", LIMIT), "{}", terminal.text());
    assert_eq!(count(&terminal.raw, b"\x1b[?1049h"), 1);

    // Typed text is in the file's text, and Ctrl+S saves it.
    terminal.type_bytes("héllo ".as_bytes());
    assert!(terminal.until("llo", LIMIT), "{}", terminal.text());
    terminal.type_bytes(b"\x13");
    assert!(
        file_becomes(&pane, "a.txt", |text| text == "héllo world\n"),
        "{:?}",
        std::fs::read_to_string(pane.path("a.txt"))
    );

    // The palette (Ctrl+K) lists the files, and a file chosen by its name is opened.
    let from = terminal.raw.len();
    terminal.type_bytes(b"\x0b");
    assert!(
        terminal.until_since(from, "Files", LIMIT),
        "{}",
        plain(&terminal.raw[from..])
    );
    terminal.type_bytes(b"b.tx");
    let from = terminal.raw.len();
    terminal.type_bytes(b"\r");
    assert!(
        terminal.until_since(from, "other", LIMIT),
        "{}",
        plain(&terminal.raw[from..])
    );

    // The find bar (Ctrl+F) says what it found.
    let from = terminal.raw.len();
    terminal.type_bytes(b"\x06");
    assert!(
        terminal.until_since(from, "Aa", LIMIT),
        "{}",
        plain(&terminal.raw[from..])
    );
    // The page gives the field the keyboard as it draws it: what is typed once it has it is the field's,
    // and the bar says there are no results for it. A key typed before that is the editor's.
    let from = terminal.raw.len();
    let end = Instant::now() + LIMIT;
    let mut found = false;
    while !found && Instant::now() < end {
        terminal.type_bytes(b"z");
        found = terminal.until_since(from, "results", Duration::from_millis(300));
    }
    assert!(found, "{}", plain(&terminal.raw[from..]));
    terminal.type_bytes(b"\x1b");
    // An Escape and what follows in one read would be one key (Alt with it).
    std::thread::sleep(Duration::from_millis(300));

    // Ctrl+C ends it as a signal does, and the terminal is given back.
    terminal.type_bytes(b"\x03");
    assert_eq!(terminal.ended(LIMIT), Some(130), "{}", terminal.text());
    let raw = &terminal.raw;
    let entered = raw
        .windows(8)
        .position(|window| window == b"\x1b[?1049h")
        .expect("the alternate screen was entered");
    let left = raw
        .windows(8)
        .rposition(|window| window == b"\x1b[?1049l")
        .expect("the alternate screen was left");
    assert!(entered < left);
    let cursor = raw
        .windows(6)
        .rposition(|window| window == b"\x1b[?25h")
        .expect("the cursor was shown again");
    assert!(cursor > entered, "the cursor was given back");
    // What was typed and saved is what the file holds; nothing is left to ask about.
    assert_eq!(pane.read("a.txt"), "héllo world\n");
}

#[test]
fn a_pdf_is_drawn_where_the_terminal_draws_pictures_and_is_a_sentence_where_it_does_not() {
    let pane = Pane::new();
    pane.write("a.txt", "world\n");
    pane.write("doc.pdf", pdf_of_pages(&[(612.0, 792.0), (612.0, 792.0)]));
    // A terminal with the Kitty graphics protocol is sent the pages as pictures (`PENCERE_TUI=1` asks for the
    // screen without the wait for a host).
    let mut terminal = start(&pane, "doc.pdf", true, &[("PENCERE_TUI", "1")]);
    assert!(terminal.until("pdf", LIMIT), "{}", terminal.text());
    let end = Instant::now() + LIMIT;
    while count(&terminal.raw, b"\x1b_G") < 2 && Instant::now() < end {
        terminal.until_since(0, "\u{ffff}", Duration::from_millis(200));
    }
    assert!(
        count(&terminal.raw, b"\x1b_G") >= 2,
        "no picture was sent: {}",
        terminal.text()
    );
    assert!(
        !terminal.text().contains("does not show PDFs"),
        "{}",
        terminal.text()
    );
    terminal.type_bytes(b"\x03");
    assert_eq!(terminal.ended(LIMIT), Some(130));

    // One that has no graphics protocol is told so, in a sentence, and the file is not asked for.
    let mut terminal = start(&pane, "doc.pdf", false, &[("PENCERE_TUI", "1")]);
    assert!(terminal.until("PDFs", LIMIT), "{}", terminal.text());
    assert_eq!(count(&terminal.raw, b"\x1b_Ga="), 0);
    terminal.type_bytes(b"\x03");
    assert_eq!(terminal.ended(LIMIT), Some(130));
}
