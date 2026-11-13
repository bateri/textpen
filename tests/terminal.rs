//! textpen in a terminal that hosts nothing: it draws the editor with the characters of the terminal it is
//! started in, takes the keys, saves what is typed, and gives the terminal back as it was. The terminal is a
//! pty (`common::terminal`) that answers what a terminal answers and shows what is written to it.

// The screen is the `terminal` feature's: the build without it has none to start.
#![cfg(feature = "terminal")]
// A test that cannot go on fails where it stands, helpers included.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::{Duration, Instant};

use common::terminal::{Start, Terminal, count, plain, sent, squeezed};
use common::{Pane, pdf_of_pages, pdf_of_text, png_of, program};

const LIMIT: Duration = Duration::from_secs(60);

/// A project with `a.txt` in it and a PDF, and textpen run on `file` in a terminal of 100 by 30 that
/// hosts nothing (and draws pictures where `graphics`).
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

/// Waits until `shown` says of what the program wrote that the file is on the screen, or what a person
/// would see of it going wrong is: the text of the file, or that it is not text.
fn until_shown(terminal: &mut Terminal, shown: impl Fn(&[u8]) -> bool) {
    let end = Instant::now() + LIMIT;
    while Instant::now() < end {
        terminal.until_with(0, Duration::from_millis(200), |_| false);
        let text = terminal.text();
        if shown(&terminal.raw) || text.contains("is not text") || text.contains("%PDF-") {
            break;
        }
    }
}

/// Whether the program has sent a picture as large as a page or a photograph is (the tabs' icons are small).
fn a_large_picture(raw: &[u8]) -> bool {
    sent(raw)
        .iter()
        .any(|picture| picture.width >= 96 && picture.height >= 96)
}

/// What the person must not see of a PDF or a picture opened: its bytes read as text.
fn assert_not_read_as_text(terminal: &Terminal) {
    let text = terminal.text();
    assert!(
        !text.contains("%PDF-"),
        "the PDF was read as text: {}",
        squeezed(&text)
    );
    assert!(
        !text.contains("is not text"),
        "the picture was not opened: {}",
        squeezed(&text)
    );
}

fn a_picture() -> Vec<u8> {
    // Every cell has a different colour above and below, so that it takes a half block to draw it.
    png_of(240, 240, |x, y| {
        [
            u8::try_from(x).unwrap_or(0),
            u8::try_from(y).unwrap_or(0),
            128,
        ]
    })
}

#[test]
fn a_pdf_opened_in_a_live_terminal_is_a_picture_of_its_page_where_the_terminal_draws_pictures() {
    let pane = Pane::new();
    pane.write("doc.pdf", pdf_of_pages(&[(612.0, 792.0), (612.0, 792.0)]));
    let mut terminal = start(&pane, "doc.pdf", true, &[("PENCERE_TUI", "1")]);
    until_shown(&mut terminal, a_large_picture);
    assert_not_read_as_text(&terminal);
    assert!(
        a_large_picture(&terminal.raw),
        "no page was sent, only {:?}: {}",
        sent(&terminal.raw),
        squeezed(&terminal.text())
    );
    terminal.type_bytes(b"\x03");
    assert_eq!(terminal.ended(LIMIT), Some(130));
}

#[test]
fn a_pdf_opened_in_a_live_terminal_that_draws_no_pictures_is_the_sentence_and_not_its_bytes() {
    let pane = Pane::new();
    pane.write("doc.pdf", pdf_of_pages(&[(612.0, 792.0)]));
    let mut terminal = start(&pane, "doc.pdf", false, &[("PENCERE_TUI", "1")]);
    until_shown(&mut terminal, |raw| {
        plain(raw).contains("does not show PDFs")
    });
    assert_not_read_as_text(&terminal);
    assert!(
        terminal.text().contains("This terminal does not show PDFs"),
        "{}",
        squeezed(&terminal.text())
    );
    assert!(sent(&terminal.raw).is_empty(), "{:?}", sent(&terminal.raw));
    terminal.type_bytes(b"\x03");
    assert_eq!(terminal.ended(LIMIT), Some(130));
}

#[test]
fn a_png_opened_in_a_live_terminal_is_a_picture_where_the_terminal_draws_pictures() {
    let pane = Pane::new();
    pane.write("red.png", a_picture());
    let mut terminal = start(&pane, "red.png", true, &[("PENCERE_TUI", "1")]);
    until_shown(&mut terminal, a_large_picture);
    assert_not_read_as_text(&terminal);
    assert!(
        a_large_picture(&terminal.raw),
        "no picture was sent, only {:?}: {}",
        sent(&terminal.raw),
        squeezed(&terminal.text())
    );
    terminal.type_bytes(b"\x03");
    assert_eq!(terminal.ended(LIMIT), Some(130));
}

#[test]
fn a_png_opened_in_a_live_terminal_that_draws_no_pictures_is_half_blocks() {
    let pane = Pane::new();
    pane.write("red.png", a_picture());
    pane.write("a.txt", "world\n");
    let half = |text: &str| {
        text.chars()
            .filter(|c| matches!(c, '\u{2580}' | '\u{2584}'))
            .count()
    };
    // The page itself has none of them: what is found below is the picture's.
    let mut terminal = start(&pane, "a.txt", false, &[("PENCERE_TUI", "1")]);
    assert!(
        terminal.until("world", LIMIT),
        "{}",
        squeezed(&terminal.text())
    );
    assert_eq!(half(&terminal.text()), 0, "{}", squeezed(&terminal.text()));
    terminal.type_bytes(b"\x03");
    assert_eq!(terminal.ended(LIMIT), Some(130));

    let mut terminal = start(&pane, "red.png", false, &[("PENCERE_TUI", "1")]);
    until_shown(&mut terminal, |raw| half(&plain(raw)) >= 100);
    assert_not_read_as_text(&terminal);
    assert!(
        half(&terminal.text()) >= 100,
        "the picture is not in half blocks: {}",
        squeezed(&terminal.text())
    );
    assert!(sent(&terminal.raw).is_empty(), "{:?}", sent(&terminal.raw));
    terminal.type_bytes(b"\x03");
    assert_eq!(terminal.ended(LIMIT), Some(130));
}

#[test]
fn a_pdf_open_when_the_pane_is_given_to_a_shell_is_drawn_again_when_the_shell_ends() {
    let pane = Pane::new();
    pane.write("doc.pdf", pdf_of_pages(&[(612.0, 792.0), (612.0, 792.0)]));
    // `SHELL` is `true`: the shell the pane is given to ends at once, and the view comes back.
    let mut terminal = start(&pane, "doc.pdf", true, &[("PENCERE_TUI", "1")]);
    until_shown(&mut terminal, a_large_picture);
    assert_not_read_as_text(&terminal);
    assert!(a_large_picture(&terminal.raw), "{:?}", sent(&terminal.raw));
    let from = terminal.raw.len();
    // Ctrl+Alt+T, the shell's chord where the keyboard is not a Mac's.
    terminal.type_bytes(b"\x1b\x14");
    let end = Instant::now() + LIMIT;
    while Instant::now() < end {
        terminal.until_with(0, Duration::from_millis(200), |_| false);
        // The screen was given up and taken again, and the page is drawn: the PDF's page is sent anew.
        if count(&terminal.raw[from..], b"\x1b[?1049h") >= 1
            && a_large_picture(&terminal.raw[from..])
        {
            break;
        }
    }
    let after = &terminal.raw[from..];
    assert!(
        count(after, b"\x1b[?1049h") >= 1,
        "the pane was not given back: {}",
        squeezed(&plain(after))
    );
    assert!(
        a_large_picture(after),
        "the PDF is not drawn again, only {:?}: {}",
        sent(after),
        squeezed(&plain(after))
    );
    assert_not_read_as_text(&terminal);
    terminal.type_bytes(b"\x03");
    assert_eq!(terminal.ended(LIMIT), Some(130));
}

/// What the program writes for a PDF and a picture opened, in terminals of two sizes, with the Kitty graphics
/// protocol and without it, written as the stream the terminal was sent to the folder `LIVE_FRAMES` names
/// (`NAME.raw`), for a person to look at: it tests nothing.
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
        for graphics in [true, false] {
            for file in ["doc.pdf", "red.png"] {
                let pane = Pane::new();
                pane.write("a.txt", "world\n");
                pane.write("red.png", a_picture());
                pane.write("doc.pdf", pdf_of_text(&lines, 16));
                let mut terminal =
                    start_sized(&pane, file, graphics, &[("PENCERE_TUI", "1")], (cols, rows));
                if file == "doc.pdf" && graphics {
                    until_shown(&mut terminal, a_large_picture);
                } else if file == "doc.pdf" {
                    until_shown(&mut terminal, |raw| {
                        plain(raw).contains("does not show PDFs")
                    });
                } else if graphics {
                    until_shown(&mut terminal, a_large_picture);
                } else {
                    until_shown(&mut terminal, |raw| plain(raw).contains('\u{2580}'));
                }
                // Let it draw the rest: the page's own parts come a moment after the picture.
                for _ in 0..8 {
                    terminal.until_with(0, Duration::from_millis(250), |_| false);
                }
                let name = format!(
                    "{}-{cols}x{rows}{}",
                    if file == "doc.pdf" { "pdf" } else { "png" },
                    if graphics { "" } else { "-nopics" }
                );
                std::fs::write(out.join(format!("{name}.raw")), &terminal.raw).expect("the stream");
                terminal.type_bytes(b"\x03");
                terminal.ended(LIMIT);
            }
        }
    }
}

/// A tmux server of its own, on a socket of its own, which is killed when this goes. Only the one this
/// started is ever spoken to.
struct Tmux {
    socket: String,
}

impl Tmux {
    fn start(dir: &std::path::Path) -> Option<Self> {
        let socket = format!("textpen-test-{}", std::process::id());
        let tmux = Self { socket };
        let started = tmux
            .run(&[
                "-f",
                "/dev/null",
                "new-session",
                "-d",
                "-x",
                "120",
                "-y",
                "40",
                "-s",
                "t",
                "-c",
                &dir.to_string_lossy(),
                "/bin/sh",
            ])
            .is_some();
        started.then_some(tmux)
    }

    fn run(&self, args: &[&str]) -> Option<String> {
        let out = std::process::Command::new("tmux")
            .arg("-L")
            .arg(&self.socket)
            .args(args)
            .output()
            .ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
    }

    fn title(&self) -> String {
        self.run(&["display-message", "-p", "-t", "t", "#{pane_title}"])
            .unwrap_or_default()
            .trim_end()
            .to_owned()
    }

    fn screen(&self) -> String {
        self.run(&["capture-pane", "-p", "-t", "t"])
            .unwrap_or_default()
    }

    fn send(&self, keys: &[&str]) {
        let mut args = vec!["send-keys", "-t", "t"];
        args.extend_from_slice(keys);
        self.run(&args);
    }

    fn until(&self, what: &str) -> bool {
        let end = Instant::now() + LIMIT;
        while Instant::now() < end {
            if self.screen().contains(what) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        false
    }
}

impl Drop for Tmux {
    fn drop(&mut self) {
        // The server leaves its socket file behind when it is killed: it is taken away with it.
        let socket = self
            .run(&["display-message", "-p", "#{socket_path}"])
            .map(|path| path.trim_end().to_owned());
        self.run(&["kill-server"]);
        if let Some(socket) = socket.filter(|path| path.contains(&self.socket)) {
            std::fs::remove_file(socket).ok();
        }
    }
}

/// What the person sees of the title of a tmux pane while textpen is open in it and after it ends: the
/// one it had before. tmux keeps the string of the Kitty graphics query (an APC) as the title, so a
/// pane of textpen was titled `Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA`, and stayed so when textpen ended.
/// `environment` is what the program is run with besides. With the variables of tmux taken away (as an
/// `ssh` into a machine leaves them) and its name for the terminal as well, the terminal is asked what it
/// is and says so, and the question of pictures is not sent; but the program's first question, whether a
/// host is there, is, and tmux keeps that for the title (`whole` false: only the picture question is
/// looked for in the title).
fn assert_the_title_of_a_tmux_pane_is_left_alone(environment: &str, whole: bool) {
    let pane = Pane::new();
    pane.write("a.txt", "hello\n");
    let Some(tmux) = Tmux::start(pane.dir.path()) else {
        eprintln!("tmux is not installed here: nothing was asked of it");
        return;
    };
    let before = tmux.title();
    assert!(!before.is_empty(), "the pane has a title to begin with");
    // A binary other than the one built here, to see the sequence of a build that sends the query.
    let program = std::env::var("TEXTPEN_TEST_BIN").unwrap_or_else(|_| program().to_owned());
    let home = pane.dir.path().to_string_lossy().into_owned();
    tmux.send(&[
        &format!(
            "{environment} HOME={home} XDG_STATE_HOME={home}/state XDG_CONFIG_HOME={home}/config \
             SHELL=/usr/bin/true {program} a.txt; echo ended:$?"
        ),
        "Enter",
    ]);
    assert!(
        tmux.until("a.txt"),
        "the editor is up: {}",
        squeezed(&tmux.screen())
    );
    // The probe is over a moment after the editor draws.
    std::thread::sleep(Duration::from_millis(1500));
    let unmoved = |title: String, when: &str| {
        if whole {
            assert_eq!(title, before, "the title {when}");
        } else {
            assert!(
                !title.contains("a=q"),
                "the title {when} is the question of pictures: {title}"
            );
        }
    };
    unmoved(tmux.title(), "while textpen is open");
    tmux.send(&["C-c"]);
    assert!(
        tmux.until("ended:130"),
        "textpen ended: {}",
        squeezed(&tmux.screen())
    );
    unmoved(tmux.title(), "after textpen ended");
}

#[test]
fn the_title_of_a_tmux_pane_is_left_alone_by_a_textpen_that_opens_and_ends_in_it() {
    // As the pane has it (TMUX, TERM=tmux-256color); with the page forced on, the way the other
    // guards start it; with the variables tmux sets taken away and its name for the terminal kept, as
    // an ssh leaves it; and with that taken away too, so that only the terminal's answer says.
    assert_the_title_of_a_tmux_pane_is_left_alone("env -u PENCERE_TUI", true);
    assert_the_title_of_a_tmux_pane_is_left_alone("env PENCERE_TUI=1", true);
    assert_the_title_of_a_tmux_pane_is_left_alone(
        "env -u PENCERE_TUI -u TMUX -u TMUX_PANE -u TERM_PROGRAM",
        true,
    );
    assert_the_title_of_a_tmux_pane_is_left_alone(
        "env -u PENCERE_TUI -u TMUX -u TMUX_PANE -u TERM_PROGRAM TERM=xterm-256color",
        false,
    );
}
