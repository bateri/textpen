//! The pane given away and taken back. When the person asks for a shell, the program lends the
//! terminal to it: the view closes, the shell runs in the project's directory as the terminal's
//! foreground job, and when it ends the view is shown again where it was. What is not saved lives in
//! the program, so it crosses that gap. In a terminal that shows no views the pane is given to the
//! person's own `$VISUAL` or `$EDITOR` instead, the same way.
//!
//! Plain values in and out: this file knows nothing of the page. How the shell ended, and a
//! hand-off the terminal could not make, are kept here as plain values for api/ to tell the page.

use std::io::{self, BufRead, Write};
use std::process::{Command, ExitStatus};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use pencere::{HandOffResult, NoView, NoViewReason, ViewControl};

use crate::project::paths;

/// The shell integrations a person's rc file loads whatever the terminal, turned off in the shell
/// textpen starts: their prompt marks carry no identity of the terminal's own integration, and a
/// short-lived shell inside textpen has no prompt the terminal needs to know of — its marks would only
/// move the terminal's idea of the pane (bateri's phase and its dock) back and forth. iTerm2's script
/// is skipped when its own guard is set; WezTerm's has a switch for it. `TEXTPEN_SHELL` tells the
/// shell, and the person's prompt, that it is inside textpen.
const SHELL_ENV: [(&str, &str); 3] = [
    ("ITERM_SHELL_INTEGRATION_INSTALLED", "Yes"),
    ("WEZTERM_SHELL_SKIP_ALL", "1"),
    ("TEXTPEN_SHELL", "1"),
];

/// textpen's name as the shell's first line shows it.
const BAR: &str = "\x1b[7m textpen \x1b[0m ";

/// How the shell the pane was lent to ended: its status (none when a signal ended it), how long
/// it had the pane, and why it could not start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Returned {
    pub status: Option<i32>,
    pub ms: u64,
    pub error: Option<String>,
}

/// Why the terminal could not lend its pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failed {
    /// It cannot close a view and show it again.
    Unsupported,
    /// The view could not be closed just then.
    Closing,
}

/// What asking for a shell came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asked {
    /// The pane goes to the shell once the answer is out.
    Scheduled,
    /// The pane is lent already.
    Busy,
    /// This terminal cannot lend it.
    Unavailable,
}

#[derive(Debug, Default)]
struct Lending {
    /// A shell has the pane, or is about to.
    busy: bool,
    /// The pane is being lent, or a command had it once: a view that does not come back is not
    /// replaced with an editor.
    lent: bool,
    /// A command had the pane once.
    had_pane: bool,
    /// The terminal said it cannot lend its pane.
    unsupported: bool,
    returned: Option<Returned>,
    failed: Option<Failed>,
}

struct Shared {
    lending: Mutex<Lending>,
    /// A hand-off is over.
    settled: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Lending> {
        self.lending.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The pane, lent to a shell or the person's own editor, and taken back.
pub struct Desk {
    control: ViewControl,
    root: String,
    shared: Arc<Shared>,
}

/// The shell in the project's directory, with a first line that says where it is and how to come
/// back, which it writes once the view is gone and the terminal is its own.
fn shell_command(root: &str, shell: &str) -> Command {
    let banner = format!(
        "{BAR}shell in \x1b[1m{root}\x1b[0m\x1b[0m  \x1b[2mexit or ⌃D goes back to the editor\x1b[0m"
    );
    let mut command = Command::new("/bin/sh");
    command
        .args(["-c", "printf \"%s\\n\" \"$1\"; exec \"$2\"", "sh"])
        .arg(banner)
        .arg(shell)
        .current_dir(root);
    for (name, value) in SHELL_ENV {
        command.env(name, value);
    }
    command
}

/// How a command that had the pane ended, as the page is told it.
fn returned_of(status: &io::Result<ExitStatus>, ms: u64, shell: &str) -> Returned {
    match status {
        Ok(status) => Returned {
            status: status.code(),
            ms,
            // The shell's own word for a command it could not find or run.
            error: matches!(status.code(), Some(126 | 127))
                .then(|| format!("could not start {shell}")),
        },
        Err(why) => Returned {
            status: None,
            ms,
            error: Some(format!("could not start {shell}: {why}")),
        },
    }
}

/// What a hand-off came to, kept for the page.
fn settle(lending: &mut Lending, result: &HandOffResult, ms: u64, shell: &str) {
    match result {
        HandOffResult::Returned(status)
        | HandOffResult::NoView {
            status: Some(status),
            ..
        } => {
            lending.had_pane = true;
            lending.returned = Some(returned_of(status, ms, shell));
        }
        // The view was closed for the shell, and did not come back.
        HandOffResult::NoView { status: None, .. } => lending.had_pane = true,
        // Nothing was lent: the view is as it was.
        HandOffResult::Unsupported => {
            lending.unsupported = true;
            lending.failed = Some(Failed::Unsupported);
            lending.lent = lending.had_pane;
        }
        _ => {
            lending.failed = Some(Failed::Closing);
            lending.lent = lending.had_pane;
        }
    }
    lending.busy = false;
}

/// Whether an editor command is textpen itself, which is passed over so that `EDITOR=textpen` does
/// not start textpen again.
fn is_textpen(editor: &str) -> bool {
    let ends_word = |rest: &str| rest.is_empty() || rest.starts_with(char::is_whitespace);
    editor.strip_prefix("textpen").is_some_and(ends_word)
        || editor
            .match_indices("/textpen")
            .any(|(at, found)| editor.get(at + found.len()..).is_some_and(ends_word))
}

/// The editor to run: `$VISUAL`, `$EDITOR` or vi, textpen passed over.
fn own_editor(variable: &dyn Fn(&str) -> Option<String>) -> String {
    ["VISUAL", "EDITOR"]
        .iter()
        .filter_map(|name| variable(name))
        .find(|editor| !editor.trim().is_empty() && !is_textpen(editor.trim()))
        .unwrap_or_else(|| "vi".to_owned())
}

/// The person's own editor on `files` (absolute; the project when there are none), run as git runs
/// it (`sh -c '$EDITOR "$@"'`) so that an editor given with options works, in the project's
/// directory `root`.
fn own_editor_command(root: &str, files: &[String]) -> Command {
    let editor = own_editor(&|name| std::env::var(name).ok());
    let targets: Vec<String> = if files.is_empty() {
        vec![".".to_owned()]
    } else {
        files
            .iter()
            .map(|file| match paths::relative(root, file) {
                relative if relative.is_empty() => file.clone(),
                relative => relative,
            })
            .collect()
    };
    let mut command = Command::new("sh");
    command
        .arg("-c")
        .arg(format!("{editor} \"$@\""))
        .arg(&editor)
        .args(&targets)
        .current_dir(root);
    command
}

impl Desk {
    /// The desk of the project at `root`.
    pub fn new(root: &str) -> Self {
        Self {
            control: ViewControl::new(),
            root: root.to_owned(),
            shared: Arc::new(Shared {
                lending: Mutex::new(Lending::default()),
                settled: Condvar::new(),
            }),
        }
    }

    /// The hand on the view that the desk lends the pane with, for `serve`.
    pub fn control(&self) -> ViewControl {
        self.control.clone()
    }

    /// Whether the pane can be lent: until the terminal says it cannot.
    pub fn available(&self) -> bool {
        !self.shared.lock().unsupported
    }

    /// Whether the pane is being lent to a shell, or a shell had it once.
    pub fn lent(&self) -> bool {
        self.shared.lock().lent
    }

    /// The hand-off under way came to `result` (for the tests of what the terminal cannot show at
    /// will: a terminal that cannot lend its pane).
    #[cfg(test)]
    pub fn settled(&self, result: &HandOffResult) {
        settle(&mut self.shared.lock(), result, 0, "sh");
        self.shared.settled.notify_all();
    }

    /// Lends the pane to the person's shell (`$SHELL`, or `/bin/sh`), in the project's directory,
    /// after `before` (the drafts written, for a hangup while the shell has the pane). Asked on the
    /// thread of a call, the pane goes once the call's answer is out.
    pub fn shell(&self, before: impl FnOnce()) -> Asked {
        {
            let mut lending = self.shared.lock();
            if lending.unsupported {
                return Asked::Unavailable;
            }
            if lending.busy {
                return Asked::Busy;
            }
            lending.busy = true;
            lending.lent = true;
        }
        before();
        let shell = std::env::var("SHELL")
            .ok()
            .filter(|shell| !shell.is_empty())
            .unwrap_or_else(|| "/bin/sh".to_owned());
        let started = Instant::now();
        let handoff = self.control.hand_off(shell_command(&self.root, &shell));
        let shared = Arc::clone(&self.shared);
        let waiting = thread::Builder::new()
            .name("textpen-shell".to_owned())
            .spawn(move || {
                let result = handoff.wait();
                let ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
                settle(&mut shared.lock(), &result, ms, &shell);
                shared.settled.notify_all();
            });
        if waiting.is_err() {
            // The hand-off goes on; only how it ends is not known.
            self.shared.lock().busy = false;
        }
        Asked::Scheduled
    }

    /// How the shell ended, once: waits up to `patience` for a shell that has the pane, since the
    /// view shown again may ask before the library has said.
    pub fn take_returned(&self, patience: Duration) -> Option<Returned> {
        let lending = self.shared.lock();
        let (mut lending, _) = self
            .shared
            .settled
            .wait_timeout_while(lending, patience, |lending| lending.busy)
            .unwrap_or_else(PoisonError::into_inner);
        lending.returned.take()
    }

    /// Why the terminal could not lend its pane, once.
    pub fn take_failed(&self) -> Option<Failed> {
        self.shared.lock().failed.take()
    }

    /// What stands in for a terminal that shows no views: the person's own editor, in the pane as a
    /// shell would run it; or, where there is no terminal at all (a script, a pipe), the words that
    /// say so. The status the program ends with; none after the pane was lent to a shell, when the
    /// view that did not come back is not replaced with another program.
    pub fn fallback(&self, no_terminal: bool, files: &[String]) -> Option<u8> {
        if self.lent() {
            return None;
        }
        if no_terminal {
            eprintln!("textpen: needs a terminal to edit in");
            return Some(1);
        }
        let ended = self
            .control
            .hand_off(own_editor_command(&self.root, files))
            .wait();
        Some(match ended {
            HandOffResult::Returned(Ok(status)) => status
                .code()
                .and_then(|code| u8::try_from(code).ok())
                .unwrap_or(1),
            HandOffResult::Returned(Err(_)) => 127,
            _ => 1,
        })
    }
}

/// The view did not come back after the shell: the terminal did not answer, or would not show it.
/// What is open is still the program's, so the person chooses on the terminal: once more, or quit
/// (keeping what is not saved as drafts). Whether to try again.
pub fn try_again(no_view: &NoView) -> bool {
    let why = match (no_view.reason, &no_view.message) {
        (NoViewReason::NoHost | NoViewReason::Timeout, _) => {
            "the terminal did not answer".to_owned()
        }
        (_, Some(message)) => format!("the terminal said {message}"),
        (NoViewReason::Refused, None) => "the terminal would not show it".to_owned(),
        _ => "the terminal could not show it".to_owned(),
    };
    let mut out = io::stdout().lock();
    let _ = write!(
        out,
        "\x1b[7m textpen \x1b[0m the editor did not come back: {why}.\n\x1b[2m         What is not saved is kept. ⏎ tries again, q and ⏎ quits.\x1b[0m "
    );
    let _ = out.flush();
    let mut answer = String::new();
    match io::stdin().lock().read_line(&mut answer) {
        Ok(0) | Err(_) => false,
        Ok(_) => !answer.trim().eq_ignore_ascii_case("q"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;

    #[test]
    fn the_own_editor_is_visual_then_editor_then_vi_with_textpen_passed_over() {
        let of = |pairs: &'static [(&'static str, &'static str)]| {
            own_editor(&move |name: &str| {
                pairs
                    .iter()
                    .find(|(key, _)| *key == name)
                    .map(|(_, value)| (*value).to_owned())
            })
        };
        assert_eq!(of(&[("VISUAL", "nano"), ("EDITOR", "vim")]), "nano");
        assert_eq!(of(&[("VISUAL", "textpen"), ("EDITOR", "cat -n")]), "cat -n");
        assert_eq!(
            of(&[("VISUAL", "/usr/local/bin/textpen --x"), ("EDITOR", " ")]),
            "vi"
        );
        assert_eq!(of(&[("EDITOR", "textpenx")]), "textpenx");
        assert_eq!(of(&[]), "vi");
    }

    #[test]
    fn how_the_shell_ended_is_kept_and_a_terminal_that_cannot_lend_is_not_asked_again() {
        let mut lending = Lending {
            busy: true,
            ..Lending::default()
        };
        settle(
            &mut lending,
            &HandOffResult::Returned(Ok(ExitStatus::from_raw(3 << 8))),
            40,
            "/bin/zsh",
        );
        assert_eq!(
            lending.returned.take(),
            Some(Returned {
                status: Some(3),
                ms: 40,
                error: None
            })
        );
        assert!(!lending.busy);
        settle(
            &mut lending,
            &HandOffResult::Returned(Ok(ExitStatus::from_raw(127 << 8))),
            5,
            "/no/shell",
        );
        assert_eq!(
            lending.returned.take().and_then(|returned| returned.error),
            Some("could not start /no/shell".to_owned())
        );
        // A signal's end has no status.
        settle(
            &mut lending,
            &HandOffResult::Returned(Ok(ExitStatus::from_raw(libc::SIGKILL))),
            5,
            "sh",
        );
        assert_eq!(
            lending.returned.take().map(|returned| returned.status),
            Some(None)
        );
        assert!(lending.had_pane);
        // A hand-off that lent nothing leaves the pane as lent as it was.
        let mut fresh = Lending {
            busy: true,
            lent: true,
            ..Lending::default()
        };
        settle(&mut fresh, &HandOffResult::Busy, 0, "sh");
        assert_eq!(fresh.failed.take(), Some(Failed::Closing));
        assert!(!fresh.lent && !fresh.busy);
        lending.lent = true;
        settle(&mut lending, &HandOffResult::Unavailable, 0, "sh");
        assert!(lending.lent, "a shell had the pane before");
        settle(&mut lending, &HandOffResult::Unsupported, 0, "sh");
        assert_eq!(lending.failed, Some(Failed::Unsupported));
        assert!(lending.unsupported);
        let desk = Desk::new("/");
        desk.shared.lock().unsupported = true;
        assert_eq!(desk.shell(|| panic!("nothing is done")), Asked::Unavailable);
        assert!(!desk.available());
    }

    #[test]
    fn the_shell_says_where_it_is_and_is_told_it_is_inside_textpen() {
        let command = shell_command("/work/project", "/bin/zsh");
        let args: Vec<String> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args[0], "-c");
        assert!(
            args[3].contains("shell in \x1b[1m/work/project"),
            "{args:?}"
        );
        assert_eq!(args[4], "/bin/zsh");
        assert_eq!(
            command
                .get_current_dir()
                .map(|dir| dir.to_string_lossy().into_owned()),
            Some("/work/project".to_owned())
        );
        let env: Vec<(String, String)> = command
            .get_envs()
            .filter_map(|(name, value)| {
                Some((
                    name.to_string_lossy().into_owned(),
                    value?.to_string_lossy().into_owned(),
                ))
            })
            .collect();
        assert!(env.contains(&("TEXTPEN_SHELL".to_owned(), "1".to_owned())));
        assert!(env.contains(&("WEZTERM_SHELL_SKIP_ALL".to_owned(), "1".to_owned())));
    }
}
