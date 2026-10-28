//! The command line: what textpen was asked to open. `+line`, `path:line:column`, the directory to
//! open as the project; the help, the version and the complaints go to the terminal as text.
//!
//! Plain values in and out: this file knows nothing of the page.

use std::path::Path;

use crate::project::paths;

const USAGE: &str = "\
usage: textpen [options] [+line] [path[:line[:column]] ...]

A code editor in the terminal pane: a file tree, tabs, search, and the
terminal's own colours and font, with the keyboard or the mouse.

  path                 a file to open, or the directory to open as the project
                       (the current directory by default, with the files that
                       were open there last time)
  +line, path:line     put the cursor on that line
  -h, --help           this text
  -v, --version        the version

In a terminal that does not show pencere views it runs $VISUAL or $EDITOR.";

/// Where the cursor goes: a file (absolute), a line from 1 and a column from 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub path: String,
    pub line: u64,
    pub column: Option<u64>,
}

/// What the command line asks.
#[derive(Debug, PartialEq, Eq)]
pub struct Line {
    /// The project's directory, absolute.
    pub root: String,
    /// The files to open, absolute; none for the files that were open there last time.
    pub files: Vec<String>,
    /// Where the cursor goes, when the command line named a line.
    pub focus: Option<Place>,
}

/// What the program does about its command line.
#[derive(Debug, PartialEq, Eq)]
pub enum Asked {
    Run(Line),
    /// Nothing to show: the help or the version was printed, or a complaint. The status to end with.
    Exit(u8),
}

/// A path named on the command line, with the line and column it may have.
struct Named {
    path: String,
    line: Option<u64>,
    column: Option<u64>,
}

/// `src/a.rs:12:4` is the file at a line and column, unless a file has that very name.
fn split_place(arg: &str, exists: &dyn Fn(&str) -> bool) -> Named {
    let number = |text: &str| -> Option<u64> {
        (!text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()))
            .then(|| text.parse().ok())
            .flatten()
    };
    let plain = || Named {
        path: arg.to_owned(),
        line: None,
        column: None,
    };
    if exists(arg) {
        return plain();
    }
    let mut parts = arg.rsplitn(3, ':');
    let (last, middle, first) = (parts.next(), parts.next(), parts.next());
    match (first, middle.and_then(number), last.and_then(number)) {
        (Some(path), Some(line), Some(column)) if !path.is_empty() => Named {
            path: path.to_owned(),
            line: Some(line),
            column: Some(column),
        },
        _ => match arg.rsplit_once(':') {
            Some((path, line)) if !path.is_empty() && number(line).is_some() => Named {
                path: path.to_owned(),
                line: number(line),
                column: None,
            },
            _ => plain(),
        },
    }
}

fn is_directory(path: &str) -> bool {
    Path::new(path).is_dir()
}

/// What `args` (without the program's name) ask, in the directory `cwd` (absolute). The help, the
/// version and a command line that is not understood are printed here.
pub fn read(args: impl IntoIterator<Item = String>, cwd: &str) -> Asked {
    let exists = |path: &str| Path::new(&paths::resolve(cwd, path)).exists();
    let mut given = Vec::new();
    let mut focus: Option<Named> = None;
    let mut line = None;
    let mut rest = false;
    for arg in args {
        let plus = arg.strip_prefix('+').filter(|digits| {
            !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
        });
        match arg.as_str() {
            "-h" | "--help" if !rest => {
                println!("{USAGE}");
                return Asked::Exit(0);
            }
            "-v" | "--version" if !rest => {
                println!("{}", env!("CARGO_PKG_VERSION"));
                return Asked::Exit(0);
            }
            "--dev" if !rest => {
                eprintln!(
                    "textpen: --dev is for `pencere dev` in textpen's own directory, which builds the program and shows the view again when its files change"
                );
                return Asked::Exit(2);
            }
            "--" if !rest => rest = true,
            _ if !rest && plus.is_some() => line = plus.and_then(|digits| digits.parse().ok()),
            _ if !rest && arg.starts_with('-') && arg != "-" => {
                eprintln!("textpen: unknown option {arg}\n\n{USAGE}");
                return Asked::Exit(2);
            }
            _ => {
                let named = split_place(&arg, &exists);
                if focus.is_none() && (named.line.is_some() || line.is_some()) {
                    focus = Some(Named {
                        path: named.path.clone(),
                        line: named.line.or(line),
                        column: named.column,
                    });
                }
                given.push(named.path);
            }
        }
    }
    let (root, files) = root_of(&given, cwd);
    let focus = focus.filter(|_| !files.is_empty()).and_then(|named| {
        Some(Place {
            path: paths::resolve(cwd, &named.path),
            line: named.line?,
            column: named.column,
        })
    });
    Asked::Run(Line { root, files, focus })
}

/// The project and the files to open: the directory given first, or the current one when the files
/// given are in it, or else the directory of the first file. Directories after the first are not
/// files to open.
fn root_of(given: &[String], cwd: &str) -> (String, Vec<String>) {
    let absolute: Vec<String> = given.iter().map(|path| paths::resolve(cwd, path)).collect();
    let files: Vec<String> = absolute
        .iter()
        .skip(usize::from(
            absolute.first().is_some_and(|first| is_directory(first)),
        ))
        .filter(|path| !is_directory(path))
        .cloned()
        .collect();
    let root = match absolute.first() {
        Some(first) if is_directory(first) => first.clone(),
        _ => match files.first() {
            Some(file) if !paths::is_within(file, cwd) => paths::dirname(file),
            _ => paths::normalize(cwd),
        },
    };
    (root, files)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(arg: &str, existing: &[&str]) -> (String, Option<u64>, Option<u64>) {
        let place = split_place(arg, &|path| existing.contains(&path));
        (place.path, place.line, place.column)
    }

    #[test]
    fn a_place_is_a_line_and_a_column_after_the_path_unless_a_file_has_that_name() {
        assert_eq!(
            named("a.rs:12:4", &[]),
            ("a.rs".to_owned(), Some(12), Some(4))
        );
        assert_eq!(named("a.rs:12", &[]), ("a.rs".to_owned(), Some(12), None));
        assert_eq!(named("a:1:2:3", &[]), ("a:1".to_owned(), Some(2), Some(3)));
        assert_eq!(named("x:5", &["x:5"]), ("x:5".to_owned(), None, None));
        assert_eq!(named("a.rs:", &[]), ("a.rs:".to_owned(), None, None));
        assert_eq!(named(":5", &[]), (":5".to_owned(), None, None));
        assert_eq!(named("a.rs:x:4", &[]), ("a.rs:x".to_owned(), Some(4), None));
    }

    fn line(args: &[&str], cwd: &str) -> Line {
        match read(args.iter().map(|arg| (*arg).to_owned()), cwd) {
            Asked::Run(line) => line,
            Asked::Exit(code) => panic!("ended with {code}"),
        }
    }

    #[test]
    fn the_project_is_the_current_directory_unless_the_files_are_elsewhere() {
        let cwd = "/nowhere/here";
        let here = line(&["+7", "a.txt"], cwd);
        assert_eq!(here.root, cwd);
        assert_eq!(here.files, ["/nowhere/here/a.txt"]);
        assert_eq!(
            here.focus,
            Some(Place {
                path: "/nowhere/here/a.txt".to_owned(),
                line: 7,
                column: None,
            })
        );
        let elsewhere = line(&["/nowhere/else/b.txt:3:2", "c.txt"], cwd);
        assert_eq!(elsewhere.root, "/nowhere/else");
        assert_eq!(
            elsewhere.files,
            ["/nowhere/else/b.txt", "/nowhere/here/c.txt"]
        );
        assert_eq!(elsewhere.focus.map(|place| place.column), Some(Some(2)));
        assert_eq!(line(&[], cwd).root, cwd);
        assert_eq!(line(&["--", "-x"], cwd).files, ["/nowhere/here/-x"]);
    }
}
