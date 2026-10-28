//! textpen: a code editor in the terminal pane. In a terminal that shows pencere views the editor
//! opens in the pane; in one that does not, the person's own `$EDITOR` does.
//!
//! The program holds the project and the open files; the page (`src/view/`, in pencere's kit) is the
//! editor, drawn by the terminal, and asks the program for what it needs with the calls of
//! `docs/calls.md`. What is not saved lives in the program, so a ⌃C or a dropped connection leaves
//! it as a draft for the next run.
//!
//! This file is the entry and nothing more: it reads the command line, makes the state, gives the
//! calls and the topic of `api/` to `serve`, and ends with the status a shell gives the program.
//!
//!   api/       everything the page talks to the program about; it knows the page
//!   project/   the project on disk: the tree, git, the search; it knows nothing of the page
//!   state/     what the program holds and remembers: open files, drafts, settings; same
//!   terminal/  the command line, who and where, and the pane given to the person's own editor; same

mod api;
mod project;
mod state;
mod terminal;

use std::process::ExitCode;
use std::sync::{Arc, Mutex, PoisonError};

use base64::Engine;
use pencere::{EndedBy, Icon, IconTheme, NoViewReason, Options, Outcome};

use state::session::{Focus, Session};
use terminal::command_line::{self, Asked};
use terminal::handoff;

/// The icons a terminal draws beside the program's name, for a dark and a light background.
const ICONS: [(&[u8], IconTheme); 4] = [
    (
        include_bytes!("../assets/icon-32-dark.png"),
        IconTheme::Dark,
    ),
    (
        include_bytes!("../assets/icon-64-dark.png"),
        IconTheme::Dark,
    ),
    (
        include_bytes!("../assets/icon-32-light.png"),
        IconTheme::Light,
    ),
    (
        include_bytes!("../assets/icon-64-light.png"),
        IconTheme::Light,
    ),
];

/// A PNG as an icon: a `data:` URI, and the size its header says, by which a terminal picks the
/// icon it wants.
fn icon(png: &[u8], theme: IconTheme) -> Icon {
    let mut icon = Icon::new(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    ));
    icon.mime_type = Some("image/png".to_owned());
    icon.theme = Some(theme);
    // After the signature and the length and name of the `IHDR` chunk, which comes first.
    let number = |at: usize| {
        png.get(at..at + 4)
            .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
            .map(u32::from_be_bytes)
    };
    if png.get(12..16) == Some(b"IHDR") {
        if let (Some(width), Some(height)) = (number(16), number(20)) {
            icon.sizes = vec![format!("{width}x{height}")];
        }
    }
    icon
}

fn main() -> ExitCode {
    let cwd = match std::env::current_dir() {
        Ok(dir) => dir.to_string_lossy().into_owned(),
        Err(why) => {
            eprintln!("textpen: the current directory cannot be read: {why}");
            return ExitCode::from(1);
        }
    };
    let args = std::env::args_os()
        .skip(1)
        .map(|arg| arg.to_string_lossy().into_owned());
    let line = match command_line::read(args, &cwd) {
        Asked::Run(line) => line,
        Asked::Exit(status) => return ExitCode::from(status),
    };
    let focus = line.focus.map(|place| Focus {
        path: place.path,
        line: place.line,
        column: place.column,
    });
    let session = Arc::new(Session::new(&line.root, &line.files, focus));

    let mut options = Options::new()
        .app(pencere::app!())
        .program("textpen")
        // What a terminal writes above the view.
        .title("Editor");
    for (png, theme) in ICONS {
        options = options.icon(icon(png, theme));
    }
    let options = api::serve(options, &session);
    // The person's own editor stands in for a terminal that shows no views.
    let fallen_back = Arc::new(Mutex::new(None));
    let options = {
        let status = Arc::clone(&fallen_back);
        let (root, files) = (line.root.clone(), line.files.clone());
        options.fallback(move |why| {
            let ended = handoff::fallback(why.reason == NoViewReason::NoTty, &root, &files);
            *status.lock().unwrap_or_else(PoisonError::into_inner) = Some(ended);
        })
    };

    let outcome = pencere::serve(options);
    // However it ended: what is not saved is kept as drafts, and the page's state for next time.
    session.ending();
    let status = match outcome {
        Ok(Outcome::Closed(closed)) if closed.by == EndedBy::Error => 1,
        Ok(outcome) => fallen_back
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .unwrap_or_else(|| outcome.shell_status()),
        Err(why) => {
            eprintln!("textpen: {why}");
            1
        }
    };
    ExitCode::from(status)
}
