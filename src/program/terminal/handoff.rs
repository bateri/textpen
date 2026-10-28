//! The pane given away. In a terminal that shows no views it is given to the person's own `$VISUAL`
//! or `$EDITOR` instead of the editor. (Giving the pane to a shell and taking it back waits for the
//! pencere library to hand a view over and take it again.)
//!
//! Plain values in and out: this file knows nothing of the page.

use std::process::Command;

use crate::project::paths;

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
/// directory `root`. The status it ended with.
fn run_own_editor(root: &str, files: &[String]) -> u8 {
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
    let status = Command::new("sh")
        .arg("-c")
        .arg(format!("{editor} \"$@\""))
        .arg(&editor)
        .args(&targets)
        .current_dir(root)
        .status();
    match status {
        Ok(status) => status
            .code()
            .and_then(|code| u8::try_from(code).ok())
            .unwrap_or(1),
        Err(_) => 127,
    }
}

/// What stands in for a terminal that shows no views, at the start: the person's own editor, or,
/// where there is no terminal at all (a script, a pipe), the words that say so. The status the
/// program ends with.
pub fn fallback(no_terminal: bool, root: &str, files: &[String]) -> u8 {
    if no_terminal {
        eprintln!("textpen: needs a terminal to edit in");
        return 1;
    }
    run_own_editor(root, files)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
