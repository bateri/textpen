//! textpen in a terminal that hosts nothing and draws pictures: it draws the page as one picture of the
//! window, takes the keys, saves what is typed, and gives the terminal back as it was. The terminal is a
//! pty (`common::terminal`) that answers what a terminal that draws pictures answers and keeps what is
//! written to it; what it shows is read from the pixels sent, and what the page did from the files it
//! wrote. (A terminal that draws no pictures is given the person's own editor: `e2e.rs`.)

// The screen is the `terminal` feature's: the build without it has none to start.
#![cfg(feature = "terminal")]
// A test that cannot go on fails where it stands, helpers included.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::{Duration, Instant};

use common::terminal::{Start, Terminal, count, pixels_that, plain, sent};
use common::{Pane, pdf_of_pages, pdf_of_text, png_of, program};

const LIMIT: Duration = Duration::from_secs(60);

/// A project with `a.txt` in it and a PDF, and textpen run on `file` in a terminal of 100 by 30 that
/// hosts nothing (and answers as one that draws pictures where `graphics`: where it does not, textpen
/// leaves the pane to the person's editor).
fn start(pane: &Pane, file: &str, graphics: bool, extra: &[(&str, &str)]) -> Terminal {
    start_sized(pane, file, graphics, extra, (100, 30))
}

fn start_sized(
    pane: &Pane,
    file: &str,
    graphics: bool,
    extra: &[(&str, &str)],
    size: (u16, u16),
) -> Terminal {
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
        size,
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

/// Waits until what the program has written is as `shown` says.
fn until_shown(terminal: &mut Terminal, shown: impl Fn(&[u8]) -> bool) {
    let end = Instant::now() + LIMIT;
    while Instant::now() < end {
        terminal.settle(Duration::from_millis(200));
        if shown(&terminal.raw) {
            break;
        }
    }
}

/// Lets the program have `millis` to say what it is going to say.
fn settle(terminal: &mut Terminal, millis: u64) {
    terminal.settle(Duration::from_millis(millis));
}

/// Whether the program has sent a picture as large as a page or a photograph is (the tabs' icons are small).
fn a_large_picture(raw: &[u8]) -> bool {
    sent(raw)
        .iter()
        .any(|picture| picture.width >= 96 && picture.height >= 96)
}

/// How many pixels of what the program sent are the white of a PDF's paper, which the page has nowhere else
/// in so large a field.
fn paper(raw: &[u8]) -> usize {
    pixels_that(raw, |r, g, b| (r, g, b) == (255, 255, 255))
}

/// How many pixels of what the program sent are of the picture `a_picture` makes, whose blue is 128 in
/// every one of its pixels (the page's own colours have no such blue in a field so large).
fn photograph(raw: &[u8]) -> usize {
    pixels_that(raw, |_, _, b| b == 128)
}

/// Pixels that make a page of a PDF or a photograph: more than the page's own parts have of the colour.
const FIELD: usize = 20_000;

fn a_picture() -> Vec<u8> {
    // Every pixel has a different red and green, and the same blue.
    png_of(240, 240, |x, y| {
        [
            u8::try_from(x).unwrap_or(0),
            u8::try_from(y).unwrap_or(0),
            128,
        ]
    })
}

#[test]
fn a_terminal_that_draws_pictures_gets_the_editor_drawn_as_a_picture_and_is_the_persons_again() {
    let pane = Pane::new();
    pane.write("a.txt", "world\n");
    pane.write("b.txt", "other\n");
    // No host answers, and nothing asks for the screen: the program waits for a host a moment, asks the
    // terminal what it draws, and draws the page as one picture of the window: the 100 by 30 cells of 10 by
    // 20 pixels, a cell more each way.
    let mut terminal = start(&pane, "a.txt", true, &[]);
    until_shown(&mut terminal, a_large_picture);
    assert!(
        sent(&terminal.raw)
            .iter()
            .any(|picture| (picture.width, picture.height) == (1010, 620)),
        "{:?}",
        sent(&terminal.raw)
    );
    assert_eq!(count(&terminal.raw, b"\x1b[?1049h"), 1);
    // No text is written for the screen: the picture is the screen.
    assert!(
        !plain(&terminal.raw).contains("world"),
        "{}",
        plain(&terminal.raw)
    );
    // The page's own parts have no PDF's paper and no photograph in them.
    assert!(paper(&terminal.raw) < FIELD && photograph(&terminal.raw) < FIELD);

    // The file is read and the page is drawn with it a moment after the first picture.
    settle(&mut terminal, 3000);
    // Typed text is in the file's text, and Ctrl+S saves it.
    terminal.type_bytes("héllo ".as_bytes());
    settle(&mut terminal, 500);
    terminal.type_bytes(b"\x13");
    assert!(
        file_becomes(&pane, "a.txt", |text| text == "héllo world\n"),
        "{:?}",
        std::fs::read_to_string(pane.path("a.txt"))
    );

    // The palette (Ctrl+K) lists the files, and a file chosen by its name is opened: what is typed
    // after it is that file's.
    terminal.type_bytes(b"\x0b");
    settle(&mut terminal, 1000);
    terminal.type_bytes(b"b.tx");
    settle(&mut terminal, 1000);
    terminal.type_bytes(b"\r");
    settle(&mut terminal, 2000);
    // A press in the text of the editor gives it the keyboard, and Ctrl+Home puts the caret at the start of
    // the file.
    terminal.type_bytes(b"\x1b[<0;600;200M\x1b[<0;600;200m");
    settle(&mut terminal, 500);
    terminal.type_bytes(b"\x1b[1;5H");
    settle(&mut terminal, 300);
    terminal.type_bytes(b"x");
    settle(&mut terminal, 500);
    terminal.type_bytes(b"\x13");
    assert!(
        file_becomes(&pane, "b.txt", |text| text == "xother\n"),
        "{:?} a={:?}",
        std::fs::read_to_string(pane.path("b.txt")),
        std::fs::read_to_string(pane.path("a.txt"))
    );

    // The find bar (Ctrl+F) has the keyboard once it is up: what is typed in it is its own, and Esc
    // gives the editor the keyboard again.
    terminal.type_bytes(b"\x06");
    settle(&mut terminal, 1500);
    terminal.type_bytes(b"z");
    settle(&mut terminal, 500);
    terminal.type_bytes(b"\x1b");
    // An Escape and what follows in one read would be one key (Alt with it).
    settle(&mut terminal, 500);
    terminal.type_bytes(b"q");
    settle(&mut terminal, 500);
    terminal.type_bytes(b"\x13");
    assert!(
        file_becomes(&pane, "b.txt", |text| text == "xqother\n"),
        "{:?}",
        std::fs::read_to_string(pane.path("b.txt"))
    );

    // Ctrl+C ends it as a signal does, and the terminal is given back.
    terminal.type_bytes(b"\x03");
    assert_eq!(terminal.ended(LIMIT), Some(130));
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
    let freed = raw
        .windows(b"\x1b_Ga=d,d=A".len())
        .rposition(|window| window == b"\x1b_Ga=d,d=A")
        .expect("the pictures were given up");
    assert!(
        freed < left,
        "the pictures are given up before the screen is left"
    );
    // What was typed and saved is what the files hold; nothing is left to ask about.
    assert_eq!(pane.read("a.txt"), "héllo world\n");
    assert_eq!(pane.read("b.txt"), "xqother\n");
}

#[test]
fn a_pdf_opened_in_a_live_terminal_is_a_picture_of_its_page_where_the_terminal_draws_pictures() {
    let pane = Pane::new();
    pane.write("doc.pdf", pdf_of_pages(&[(612.0, 792.0), (612.0, 792.0)]));
    let mut terminal = start(&pane, "doc.pdf", true, &[("PENCERE_TUI", "1")]);
    until_shown(&mut terminal, |raw| paper(raw) >= FIELD);
    assert!(
        paper(&terminal.raw) >= FIELD,
        "no page of the PDF was sent, only {:?}",
        sent(&terminal.raw)
    );
    assert!(!plain(&terminal.raw).contains("%PDF-"));
    terminal.type_bytes(b"\x03");
    assert_eq!(terminal.ended(LIMIT), Some(130));
}

#[test]
fn a_png_opened_in_a_live_terminal_is_a_picture_where_the_terminal_draws_pictures() {
    let pane = Pane::new();
    pane.write("red.png", a_picture());
    let mut terminal = start(&pane, "red.png", true, &[("PENCERE_TUI", "1")]);
    until_shown(&mut terminal, |raw| photograph(raw) >= FIELD);
    assert!(
        photograph(&terminal.raw) >= FIELD,
        "no picture was sent, only {:?}",
        sent(&terminal.raw)
    );
    assert!(!plain(&terminal.raw).contains("is not text"));
    terminal.type_bytes(b"\x03");
    assert_eq!(terminal.ended(LIMIT), Some(130));
}

#[test]
fn a_pdf_open_when_the_pane_is_given_to_a_shell_is_drawn_again_when_the_shell_ends() {
    let pane = Pane::new();
    pane.write("doc.pdf", pdf_of_pages(&[(612.0, 792.0), (612.0, 792.0)]));
    // `SHELL` is `true`: the shell the pane is given to ends at once, and the view comes back.
    let mut terminal = start(&pane, "doc.pdf", true, &[("PENCERE_TUI", "1")]);
    until_shown(&mut terminal, |raw| paper(raw) >= FIELD);
    assert!(paper(&terminal.raw) >= FIELD, "{:?}", sent(&terminal.raw));
    let from = terminal.raw.len();
    // Ctrl+Alt+T, the shell's chord where the keyboard is not a Mac's.
    terminal.type_bytes(b"\x1b\x14");
    let end = Instant::now() + LIMIT;
    while Instant::now() < end {
        terminal.settle(Duration::from_millis(200));
        // The screen was given up and taken again, and the page is drawn: the PDF's page is sent anew.
        if count(&terminal.raw[from..], b"\x1b[?1049h") >= 1
            && paper(&terminal.raw[from..]) >= FIELD
        {
            break;
        }
    }
    let after = &terminal.raw[from..];
    assert!(
        count(after, b"\x1b[?1049h") >= 1,
        "the pane was not given back: {:?}",
        sent(after)
    );
    assert!(
        paper(after) >= FIELD,
        "the PDF is not drawn again, only {:?}",
        sent(after)
    );
    terminal.type_bytes(b"\x03");
    assert_eq!(terminal.ended(LIMIT), Some(130));
}

/// What the program writes for a PDF and a picture opened, in terminals of two sizes, written as the
/// stream the terminal was sent to the folder `LIVE_FRAMES` names (`NAME.raw`), for a person to look at: it
/// tests nothing.
#[test]
#[ignore = "frames: writes what the program draws to the folder LIVE_FRAMES names"]
fn the_streams_of_a_pdf_and_a_picture_opened() {
    let Some(out) = std::env::var_os("LIVE_FRAMES") else {
        return;
    };
    let out = std::path::PathBuf::from(out);
    std::fs::create_dir_all(&out).expect("the folder");
    let words = [
        "The program sends an attach: the files, each by",
        "its digest and its length in bytes. The host answers",
        "every attach with a need, whose re is the attach's id,",
        "and the program sends the files that are named in it.",
        "",
        "A file attached is the view's for as long as the view",
        "runs, and is let go when the program says so.",
    ];
    let lines: Vec<String> = words.iter().map(|line| (*line).to_owned()).collect();
    for (cols, rows) in [(160_u16, 48_u16), (80, 24)] {
        for file in ["doc.pdf", "red.png"] {
            let pane = Pane::new();
            pane.write("a.txt", "world\n");
            pane.write("red.png", a_picture());
            pane.write("doc.pdf", pdf_of_text(&lines, 16));
            let mut terminal =
                start_sized(&pane, file, true, &[("PENCERE_TUI", "1")], (cols, rows));
            if file == "doc.pdf" {
                until_shown(&mut terminal, |raw| paper(raw) >= FIELD);
            } else {
                until_shown(&mut terminal, |raw| photograph(raw) >= FIELD);
            }
            // Let it draw the rest: the page's own parts come a moment after the picture.
            for _ in 0..8 {
                terminal.settle(Duration::from_millis(250));
            }
            let name = format!(
                "{}-{cols}x{rows}",
                if file == "doc.pdf" { "pdf" } else { "png" },
            );
            std::fs::write(out.join(format!("{name}.raw")), &terminal.raw).expect("the stream");
            terminal.type_bytes(b"\x03");
            terminal.ended(LIMIT);
        }
    }
}
