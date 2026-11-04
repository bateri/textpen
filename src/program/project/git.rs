//! What git says of a project: whether it is a repository, the files it lists, and its branch and
//! changed files. git is asked with a time limit, and a repository on a slow disk or a drive that
//! went away is given up on (the caller does without); what it said is kept for a moment, so that
//! the callers who ask at once do not each start a git.
//!
//! Plain values in and out: this file knows nothing of the page.

use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use serde::Serialize;

/// How long git may take before it is given up on (the files are then walked).
const GIT_TIMEOUT: Duration = Duration::from_secs(15);

/// The most bytes of a file git keeps that are read for the marks of the gutter.
const INDEX_LIMIT: usize = super::diff::TEXT_LIMIT + super::diff::TEXT_LIMIT / 4;

/// How much of what git writes on its error output is kept, for the words of a failure.
const STDERR_LIMIT: u64 = 16 * 1024;

/// What `run` says of output past the limit it was given.
const SAID_TOO_MUCH: &str = "git said too much";
/// What `run` says of a git that failed and said nothing: `--quiet` asked it not to.
const SAID_NOTHING: &str = "git failed";

/// How long what git said of the renames is used when a file is looked for under its old name.
const RENAME_AGE: Duration = Duration::from_millis(2500);

/// The most bytes of ignored paths told to the page: a pattern can match thousands of files in folders
/// that are kept, and what git says of the project goes in one message with the rest of what it says.
const IGNORED_BYTES: usize = 64 * 1024;

/// The most bytes of paths told with how many lines each file differs by, for the same reason.
const STATS_BYTES: usize = 64 * 1024;

/// How long one asking starts counting a file's lines for: what is past this is counted at the next, and
/// what was counted is not read again until it changes.
const COUNT_TIME: Duration = Duration::from_millis(200);

/// What `compare::blocks` may fill with the words that differ while lines are counted.
const COUNT_BLOCKS_BYTES: usize = 64 * 1024;

/// What git says of a file that is not as it was committed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GitState {
    Modified,
    Added,
    Untracked,
    Deleted,
    Renamed,
    Conflict,
}

/// The branch and the changed files of a repository, by their path from the project's root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GitInfo {
    pub branch: String,
    pub ahead: u64,
    pub behind: u64,
    pub changes: BTreeMap<String, GitState>,
    /// When git's index file was last written, milliseconds since 1970 (0 where it cannot be said):
    /// what is staged or unstaged moves it, and the lines marked as differing from the index with it.
    pub index: u64,
    /// The commit the project is at (`# branch.oid`): empty before the first commit. A commit moves
    /// it, and the files shown against the last commit are compared again.
    pub head: String,
    /// For a file git has renamed or copied, the path it had, both under the project's root.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub renames: BTreeMap<String, String>,
    /// What git ignores, from the project's root: a folder that matches stands for all that is in it,
    /// which is not named. Those past `IGNORED_BYTES` are not told.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ignored: Vec<String>,
    /// How many lines each changed file adds to and takes from the last commit, as it is on the disk, by
    /// the blocks a comparison shows; all of them added for one git does not keep. None for one that is not
    /// text, one that is too long to compare, or where no line differs; those past `STATS_BYTES` are not
    /// told, and those not counted yet are told at a later asking.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub stats: BTreeMap<String, [u64; 2]>,
}

/// The last commit: its id and the first line of its message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Commit {
    pub id: String,
    pub subject: String,
}

/// A file as the last commit keeps it.
pub struct Committed {
    /// Its path in the commit, from the project's root: the one it had before git renamed it.
    pub path: String,
    /// Its blob; none where the commit has no such file (the text is then empty).
    pub id: Option<String>,
    /// Its text as textpen holds a buffer's: line ends `\n`, no byte order mark.
    pub text: Arc<String>,
}

/// What the last commit keeps of a file, for comparing.
pub enum Kept {
    /// A path outside the project, or a project git cannot say of.
    Outside,
    /// Git could not read the commit, for the words it gives: not the same as a file the commit has
    /// not, which would make every line of the file look new.
    Failed(String),
    /// Not text.
    Binary,
    /// Longer than is read.
    TooLarge,
    Text(Committed),
}

/// git, with its time limit and a limit on what it may say. The output is bytes: names are ended by
/// NUL and given exactly.
fn run(args: &[&str], cwd: &str, max_output: usize) -> Result<Vec<u8>, String> {
    let mut child = Command::new("git")
        // A repository's own config names a command for `core.fsmonitor`, which `status` runs: a
        // project that was only unpacked here would run it as soon as it is opened.
        .args(["-c", "core.fsmonitor=false"])
        .args(args)
        .current_dir(cwd)
        // git status refreshes the index when it can, which takes its lock: a person running git
        // in a shell beside the editor would find it taken.
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|why| format!("git could not be run: {why}"))?;
    let deadline = Instant::now() + GIT_TIMEOUT;
    let limit = u64::try_from(max_output)
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    let output = reader(child.stdout.take(), limit);
    let errors = reader(child.stderr.take(), STDERR_LIMIT);
    // The output ends when git closes it, which is when it ends; a git that hangs is killed, and
    // what reads its output is left to finish when the pipe closes.
    let out = match output.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
        Ok(out) => out,
        Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => {
            end(&mut child);
            return Err("git took too long".to_owned());
        }
    };
    if out.len() > max_output {
        end(&mut child);
        return Err(SAID_TOO_MUCH.to_owned());
    }
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(5)),
            Ok(None) => {
                end(&mut child);
                return Err("git took too long".to_owned());
            }
            Err(why) => {
                end(&mut child);
                return Err(format!("git failed: {why}"));
            }
        }
    };
    if !status.success() {
        let said = errors
            .recv_timeout(Duration::from_millis(100))
            .unwrap_or_default();
        let said = String::from_utf8_lossy(&said);
        let said = said.trim();
        return Err(if said.is_empty() {
            SAID_NOTHING.to_owned()
        } else {
            format!("git failed: {said}")
        });
    }
    Ok(out)
}

/// What a pipe of git's says, up to `limit` bytes, read on a thread of its own.
fn reader(pipe: Option<impl Read + Send + 'static>, limit: u64) -> mpsc::Receiver<Vec<u8>> {
    let (sender, receiver) = mpsc::channel();
    if let Some(pipe) = pipe {
        thread::spawn(move || {
            let mut out = Vec::new();
            // What could not be read is what was read until then.
            let _ = pipe.take(limit).read_to_end(&mut out);
            let _ = sender.send(out);
        });
    }
    receiver
}

fn end(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// The text git gave, as UTF-8 and without the space around it.
fn text_of(out: &[u8]) -> String {
    String::from_utf8_lossy(out).trim().to_owned()
}

fn state_of(xy: &str) -> GitState {
    let mut letters = xy.chars();
    let (x, y) = (letters.next(), letters.next());
    if xy.contains('R') {
        GitState::Renamed
    } else if x == Some('D') || y == Some('D') {
        GitState::Deleted
    } else if x == Some('A') {
        GitState::Added
    } else {
        GitState::Modified
    }
}

/// The parts of a record from the `skip`th space on, which is a path (that may have spaces in it).
fn rest_of(record: &str, skip: usize) -> Option<&str> {
    record.splitn(skip + 1, ' ').nth(skip)
}

/// The records of `git status --porcelain=v2 --branch -z`: what `read` makes of them. `prefix` is
/// where the project's root is in the repository, as git names it.
fn parse_status(out: &[u8], prefix: &str) -> GitInfo {
    let mut info = GitInfo {
        branch: String::new(),
        ahead: 0,
        behind: 0,
        changes: BTreeMap::new(),
        index: 0,
        head: String::new(),
        renames: BTreeMap::new(),
        ignored: Vec::new(),
        stats: BTreeMap::new(),
    };
    let mut ignored_bytes = 0;
    let mut renames = BTreeMap::new();
    let mut head = String::new();
    // git names paths from the top of the repository; the page names them from the root, and is
    // not told of what lies outside it. A name that is not UTF-8 is left out, as a listing leaves it.
    let mut note = |path: &str, state: GitState| {
        if let Some(rest) = path.strip_prefix(prefix) {
            info.changes.insert(rest.to_owned(), state);
        }
    };
    let mut branch = String::new();
    let (mut ahead, mut behind) = (0, 0);
    let mut records = out
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty());
    while let Some(record) = records.next() {
        let Ok(record) = std::str::from_utf8(record) else {
            continue;
        };
        if let Some(head) = record.strip_prefix("# branch.head ") {
            branch = head.to_owned();
        } else if let Some(oid) = record.strip_prefix("# branch.oid ") {
            if branch.is_empty() {
                branch = oid.chars().take(7).collect();
            }
            if oid != "(initial)" {
                oid.clone_into(&mut head);
            }
        } else if let Some(counts) = record.strip_prefix("# branch.ab ") {
            let mut counts = counts.split(' ').map(|count| {
                count
                    .trim_start_matches(['+', '-'])
                    .parse::<u64>()
                    .unwrap_or(0)
            });
            ahead = counts.next().unwrap_or(0);
            behind = counts.next().unwrap_or(0);
        } else if record.starts_with("1 ") {
            if let (Some(xy), Some(path)) = (record.get(2..4), rest_of(record, 8)) {
                note(path, state_of(xy));
            }
        } else if record.starts_with("2 ") {
            // The path it had follows, as a record of its own.
            let from = records
                .next()
                .and_then(|from| std::str::from_utf8(from).ok());
            if let (Some(xy), Some(path)) = (record.get(2..4), rest_of(record, 9)) {
                note(path, state_of(xy));
                if let (Some(to), Some(from)) = (
                    path.strip_prefix(prefix),
                    from.and_then(|from| from.strip_prefix(prefix)),
                ) {
                    renames.insert(to.to_owned(), from.to_owned());
                }
            }
        } else if record.starts_with("u ") {
            if let Some(path) = rest_of(record, 10) {
                note(path, GitState::Conflict);
            }
        } else if let Some(path) = record.strip_prefix("? ") {
            note(path, GitState::Untracked);
        } else if let Some(path) = record.strip_prefix("! ") {
            // The project's own folder ignored names nothing in it: the project is what was opened.
            let path = path.trim_end_matches('/');
            if let Some(rest) = path.strip_prefix(prefix).filter(|rest| !rest.is_empty()) {
                ignored_bytes += rest.len();
                if ignored_bytes <= IGNORED_BYTES {
                    info.ignored.push(rest.to_owned());
                }
            }
        }
    }
    info.branch = branch;
    info.ahead = ahead;
    info.behind = behind;
    info.head = head;
    info.renames = renames;
    info
}

/// A changed file as its lines were counted: the commit against, its size and time on the disk (none: it
/// is not there), and the lines it adds and takes away (none: it cannot be compared).
struct Counted {
    head: String,
    disk: Option<(u64, Option<SystemTime>)>,
    lines: Option<[u64; 2]>,
}

/// A file's bytes as textpen holds a buffer's text: none for one that is not UTF-8 text.
fn normalized(bytes: Vec<u8>) -> Option<String> {
    if super::tree::is_binary(&bytes) {
        return None;
    }
    let text = String::from_utf8(bytes).ok()?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    Some(text.replace("\r\n", "\n").replace('\r', "\n"))
}

/// A path under the project's root that git can be asked of.
fn under_root(path: &str) -> bool {
    !(path.is_empty() || path.starts_with('/') || path.split('/').any(|part| part == ".."))
}

/// When git's index was last written: the time of the file `git rev-parse --git-path index` names,
/// in milliseconds since 1970; 0 where git or the file cannot say.
fn index_written(root: &str) -> u64 {
    let Ok(said) = run(&["rev-parse", "--git-path", "index"], root, 1 << 16) else {
        return 0;
    };
    let named = text_of(&said);
    let path = std::path::Path::new(root).join(named.trim());
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .map_or(0, super::ms_of)
}

fn read(root: &str) -> Result<GitInfo, String> {
    // Where the root is in the repository, as git names it: a path git gives is made the page's by
    // taking this off, whatever links lie between the two (/var and /private/var).
    let prefix = text_of(&run(&["rev-parse", "--show-prefix"], root, 1 << 20)?);
    let out = run(
        &[
            "status",
            "--porcelain=v2",
            "--branch",
            "-z",
            "--untracked-files=all",
            // A folder that matches is named and not gone through, so this costs little in a
            // project with a large `target` or `node_modules`.
            "--ignored=matching",
        ],
        root,
        32 << 20,
    )?;
    let mut info = parse_status(&out, &prefix);
    info.index = index_written(root);
    if info.branch == "(detached)" {
        if let Ok(short) = run(&["rev-parse", "--short", "HEAD"], root, 1 << 20) {
            info.branch = text_of(&short);
        }
    }
    Ok(info)
}

/// What git said, and when.
struct Said {
    at: Instant,
    /// The `forgotten` count when it was said: what was said before the files changed is not used.
    forgotten: u64,
    value: Option<GitInfo>,
}

/// git, asked about one project.
pub struct Git {
    root: String,
    repository: Mutex<Option<bool>>,
    /// Held while git is asked, so that the callers who ask at once wait for one answer.
    said: Mutex<Option<Said>>,
    forgotten: AtomicU64,
    /// The text of the last blob read, by its id: a file compared again as it is typed in is not read
    /// from git every time.
    blobs: Mutex<Option<(String, Arc<String>)>>,
    /// The changed files whose lines were counted, by their path.
    counted: Mutex<HashMap<String, Counted>>,
}

impl Git {
    pub fn new(root: &str) -> Self {
        Self {
            root: root.to_owned(),
            repository: Mutex::new(None),
            said: Mutex::new(None),
            forgotten: AtomicU64::new(0),
            blobs: Mutex::new(None),
            counted: Mutex::new(HashMap::new()),
        }
    }

    /// Whether the project is in a repository git can read. Asked once.
    pub fn is_repository(&self) -> bool {
        let mut known = self
            .repository
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        *known.get_or_insert_with(|| {
            run(&["rev-parse", "--is-inside-work-tree"], &self.root, 1 << 20).is_ok()
        })
    }

    /// The files git lists (tracked, and not tracked but not ignored), none when git cannot say.
    pub fn files(&self) -> Option<Vec<String>> {
        if !self.is_repository() {
            return None;
        }
        let out = run(
            &[
                "ls-files",
                "-z",
                "--cached",
                "--others",
                "--exclude-standard",
            ],
            &self.root,
            64 << 20,
        )
        .ok()?;
        let mut seen = std::collections::HashSet::new();
        Some(
            out.split(|byte| *byte == 0)
                .filter(|name| !name.is_empty())
                .filter_map(|name| std::str::from_utf8(name).ok())
                .filter(|name| seen.insert(*name))
                .map(str::to_owned)
                .collect(),
        )
    }

    /// The text git keeps of `path` (relative to the project's root) in its index, as textpen holds a
    /// buffer's: line ends `\n`, no byte order mark. None for a file outside the root, one git does
    /// not keep (untracked, new in the index's absence, in a submodule), one that is not text, or
    /// where git cannot say. What is in the index and not what was committed: a change that is
    /// staged is not a difference, as VS Code's.
    pub fn index_text(&self, path: &str) -> Option<String> {
        if !under_root(path) || !self.is_repository() {
            return None;
        }
        let bytes = run(&["show", &format!(":./{path}")], &self.root, INDEX_LIMIT).ok()?;
        normalized(bytes)
    }

    /// The blob the last commit keeps `path` as, none where it keeps no file of that name or there is
    /// no commit yet; words where git could not read the commit. `ls-tree`, as `rev-parse HEAD:./path`
    /// says nothing and fails alike for a path that is not there and a tree that cannot be read.
    fn blob_of(&self, path: &str) -> Result<Option<String>, String> {
        let said = run(
            &[
                "--literal-pathspecs",
                "ls-tree",
                "-z",
                "HEAD",
                "--",
                &format!("./{path}"),
            ],
            &self.root,
            1 << 16,
        );
        match said {
            Ok(said) => {
                let line = text_of(&said);
                let mut fields = line.split([' ', '\t']);
                let (_, kind, id) = (fields.next(), fields.next(), fields.next());
                Ok(id.filter(|_| kind == Some("blob")).map(str::to_owned))
            }
            Err(why) if self.has_commit()? => Err(why),
            Err(_) => Ok(None),
        }
    }

    /// Whether there is a commit yet; words where git could not say.
    fn has_commit(&self) -> Result<bool, String> {
        match run(
            &["rev-parse", "--verify", "--quiet", "HEAD"],
            &self.root,
            1 << 16,
        ) {
            Ok(_) => Ok(true),
            Err(why) if why == SAID_NOTHING => Ok(false),
            Err(why) => Err(why),
        }
    }

    /// What the last commit (HEAD) keeps of `path` (relative to the project's root), staged or not:
    /// the file under the name git renamed it from where there is no file of this name in the commit.
    /// Where the commit keeps none the text is empty and the blob none.
    pub fn committed(&self, path: &str) -> Kept {
        if !under_root(path) || !self.is_repository() {
            return Kept::Outside;
        }
        let mut at = path.to_owned();
        let mut blob = match self.blob_of(path) {
            Ok(blob) => blob,
            Err(why) => return Kept::Failed(why),
        };
        if blob.is_none() {
            let renamed = self
                .status(RENAME_AGE)
                .and_then(|info| info.renames.get(path).cloned());
            if let Some(from) = renamed {
                match self.blob_of(&from) {
                    Ok(Some(found)) => {
                        at = from;
                        blob = Some(found);
                    }
                    Ok(None) => {}
                    Err(why) => return Kept::Failed(why),
                }
            }
        }
        let Some(id) = blob else {
            return Kept::Text(Committed {
                path: at,
                id: None,
                text: Arc::new(String::new()),
            });
        };
        let mut kept = self.blobs.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((known, text)) = kept.as_ref() {
            if *known == id {
                return Kept::Text(Committed {
                    path: at,
                    id: Some(id),
                    text: Arc::clone(text),
                });
            }
        }
        match run(&["cat-file", "blob", &id], &self.root, INDEX_LIMIT) {
            Ok(bytes) => match normalized(bytes) {
                Some(text) => {
                    let text = Arc::new(text);
                    *kept = Some((id.clone(), Arc::clone(&text)));
                    Kept::Text(Committed {
                        path: at,
                        id: Some(id),
                        text,
                    })
                }
                None => Kept::Binary,
            },
            Err(why) if why == SAID_TOO_MUCH => Kept::TooLarge,
            Err(why) => Kept::Failed(why),
        }
    }

    /// The last commit, none before the first or where git cannot say.
    pub fn last_commit(&self) -> Option<Commit> {
        if !self.is_repository() {
            return None;
        }
        let said = run(
            &[
                "log",
                "-1",
                "--no-show-signature",
                "--encoding=UTF-8",
                "--format=%H%x00%s",
                "HEAD",
            ],
            &self.root,
            1 << 20,
        )
        .ok()?;
        let said = String::from_utf8_lossy(&said);
        let (id, subject) = said.trim_end().split_once('\0')?;
        Some(Commit {
            id: id.to_owned(),
            subject: subject.to_owned(),
        })
    }

    /// What git says of the project: its branch and changed files, none outside a repository or
    /// when git cannot say. What it said less than `max_age` ago is not asked again.
    pub fn status(&self, max_age: Duration) -> Option<GitInfo> {
        let mut said = self.said.lock().unwrap_or_else(PoisonError::into_inner);
        let forgotten = self.forgotten.load(Ordering::SeqCst);
        if let Some(known) = said.as_ref() {
            if known.forgotten == forgotten && known.at.elapsed() < max_age {
                return known.value.clone();
            }
        }
        if !self.is_repository() {
            return None;
        }
        let value = read(&self.root).ok().map(|mut info| {
            info.stats = self.stats(&info);
            info
        });
        *said = Some(Said {
            at: Instant::now(),
            forgotten,
            value: value.clone(),
        });
        value
    }

    /// How many lines each changed file adds and takes away. Not by `git diff`, which runs the clean
    /// filter a repository's config names on the files on the disk: a project that was only unpacked here
    /// would run it as soon as it is opened. The commit's text is read from git and the file's from the
    /// disk, as a comparison reads them, so the two say the same.
    fn stats(&self, info: &GitInfo) -> BTreeMap<String, [u64; 2]> {
        let began = Instant::now();
        let mut counted = self.counted.lock().unwrap_or_else(PoisonError::into_inner);
        counted.retain(|path, _| info.changes.contains_key(path));
        let mut stats = BTreeMap::new();
        let mut told = 0;
        for (path, state) in &info.changes {
            told += path.len() + 24;
            if told > STATS_BYTES {
                break;
            }
            if !under_root(path) {
                continue;
            }
            let file = std::path::Path::new(&self.root).join(path);
            // A link, a folder and a submodule are no text to count.
            let disk = match std::fs::symlink_metadata(&file) {
                Ok(meta) if meta.is_file() => Some((meta.len(), meta.modified().ok())),
                Err(why) if why.kind() == std::io::ErrorKind::NotFound => None,
                _ => continue,
            };
            let known = counted
                .get(path)
                .filter(|known| known.head == info.head && known.disk == disk);
            let lines = if let Some(known) = known {
                known.lines
            } else if began.elapsed() > COUNT_TIME {
                continue;
            } else {
                let lines = self.lines_of(path, *state, info, disk.is_some());
                counted.insert(
                    path.clone(),
                    Counted {
                        head: info.head.clone(),
                        disk,
                        lines,
                    },
                );
                lines
            };
            if let Some(lines) = lines.filter(|lines| *lines != [0, 0]) {
                stats.insert(path.clone(), lines);
            }
        }
        stats
    }

    /// The lines `path` adds to and takes from the last commit; none for what cannot be compared.
    fn lines_of(
        &self,
        path: &str,
        state: GitState,
        info: &GitInfo,
        on_disk: bool,
    ) -> Option<[u64; 2]> {
        let old = if state == GitState::Untracked {
            String::new()
        } else {
            let from = info.renames.get(path).map_or(path, String::as_str);
            match self.blob_of(from).ok()? {
                Some(id) => {
                    normalized(run(&["cat-file", "blob", &id], &self.root, INDEX_LIMIT).ok()?)?
                }
                None => String::new(),
            }
        };
        let new = if on_disk {
            let file = std::path::Path::new(&self.root).join(path);
            let len = std::fs::symlink_metadata(&file).ok()?.len();
            if len > u64::try_from(super::diff::TEXT_LIMIT).unwrap_or(u64::MAX) {
                return None;
            }
            normalized(std::fs::read(file).ok()?)?
        } else {
            String::new()
        };
        let blocks = super::compare::blocks(&old, &new, COUNT_BLOCKS_BYTES).ok()?;
        let (mut added, mut removed) = (0, 0);
        for block in blocks {
            added += u64::try_from(block.new_count).unwrap_or(0);
            removed += u64::try_from(block.old_count).unwrap_or(0);
        }
        Some([added, removed])
    }

    /// Forgets what git said: the files changed.
    pub fn forget(&self) {
        self.forgotten.fetch_add(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A repository with no commit yet.
    fn repository() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        git_in(&root, &["init", "-q"]);
        (dir, root)
    }

    fn git_in(root: &std::path::Path, args: &[&str]) {
        let made = Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .current_dir(root)
            .output()
            .unwrap();
        assert!(made.status.success(), "{args:?}: {made:?}");
    }

    /// What differs from the last commit, staged or not, whatever else differs; all the lines of a file
    /// git does not keep; nothing for what is not text or what no line of differs in.
    #[test]
    fn a_change_says_how_many_lines_it_adds_and_takes_away() {
        let (_dir, root) = repository();
        let write = |name: &str, text: &[u8]| std::fs::write(root.join(name), text).unwrap();
        write("changed.txt", b"a\nb\nc\n");
        write("staged.txt", b"a\n");
        write("gone.txt", b"a\nb\n");
        write("moved.txt", b"a\nb\nc\nd\ne\nf\ng\nh\n");
        write("renamed.txt", b"a\nb\nc\nd\ne\nf\ng\nh\n");
        write("picture.bin", b"\0\0");
        git_in(&root, &["add", "."]);
        git_in(&root, &["commit", "-qm", "one"]);
        write("changed.txt", b"a\nB\nc\nd\n");
        write("staged.txt", b"a\nb\n");
        git_in(&root, &["add", "staged.txt"]);
        std::fs::remove_file(root.join("gone.txt")).unwrap();
        git_in(&root, &["mv", "moved.txt", "there.txt"]);
        git_in(&root, &["mv", "renamed.txt", "here.txt"]);
        write("here.txt", b"a\nb\nc\nd\ne\nf\ng\nh\ni\n");
        write("picture.bin", b"\0\0\0");
        write("new.txt", b"a\nb\nc");
        write("empty.txt", b"");
        write("new.bin", b"\0");
        let git = Git::new(root.to_str().unwrap());
        let stats = git.status(Duration::ZERO).unwrap().stats;
        let stats: Vec<(&str, [u64; 2])> = stats
            .iter()
            .map(|(path, counts)| (path.as_str(), *counts))
            .collect();
        assert_eq!(
            stats,
            [
                ("changed.txt", [2, 1]),
                ("gone.txt", [0, 2]),
                ("here.txt", [1, 0]),
                ("new.txt", [3, 0]),
                ("staged.txt", [1, 0]),
            ]
        );
    }

    /// Before the first commit what is staged is what differs, and what is not is new as ever.
    #[test]
    fn before_the_first_commit_a_change_says_its_lines_too() {
        let (_dir, root) = repository();
        std::fs::write(root.join("staged.txt"), "a\nb\n").unwrap();
        git_in(&root, &["add", "staged.txt"]);
        std::fs::write(root.join("new.txt"), "a\n").unwrap();
        let git = Git::new(root.to_str().unwrap());
        let stats = git.status(Duration::ZERO).unwrap().stats;
        assert_eq!(stats.get("staged.txt"), Some(&[2, 0]));
        assert_eq!(stats.get("new.txt"), Some(&[1, 0]));
    }

    /// A project git keeps under a folder of a repository says what is in it, by its own paths.
    #[test]
    fn a_project_inside_a_repository_counts_its_own_files() {
        let (_dir, root) = repository();
        std::fs::create_dir(root.join("sub")).unwrap();
        std::fs::write(root.join("sub/a.txt"), "a\n").unwrap();
        std::fs::write(root.join("b.txt"), "a\n").unwrap();
        git_in(&root, &["add", "."]);
        git_in(&root, &["commit", "-qm", "one"]);
        std::fs::write(root.join("sub/a.txt"), "a\nb\n").unwrap();
        std::fs::write(root.join("b.txt"), "a\nb\n").unwrap();
        let git = Git::new(root.join("sub").to_str().unwrap());
        let stats = git.status(Duration::ZERO).unwrap().stats;
        assert_eq!(stats.len(), 1);
        assert_eq!(stats.get("a.txt"), Some(&[1, 0]));
    }

    /// The commands a repository's config can name for a diff or for a file on the disk are not run when
    /// lines are counted.
    #[test]
    fn counting_lines_runs_no_command_a_repository_names() {
        use std::os::unix::fs::PermissionsExt;
        let (_dir, root) = repository();
        let marker = root.join("ran");
        let hook = root.join("hook.sh");
        std::fs::write(
            &hook,
            format!("#!/bin/sh\ntouch '{}'\ncat \"$1\"\n", marker.display()),
        )
        .unwrap();
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(root.join(".gitattributes"), "a.txt diff=evil filter=evil\n").unwrap();
        std::fs::write(root.join("a.txt"), "a\n").unwrap();
        git_in(&root, &["add", "a.txt", ".gitattributes"]);
        git_in(&root, &["commit", "-qm", "one"]);
        let command = hook.to_str().unwrap();
        git_in(&root, &["config", "diff.external", command]);
        git_in(&root, &["config", "diff.evil.textconv", command]);
        git_in(&root, &["config", "diff.evil.command", command]);
        git_in(&root, &["config", "filter.evil.clean", command]);
        std::fs::write(root.join("a.txt"), "a\nb\n").unwrap();
        let git = Git::new(root.to_str().unwrap());
        let stats = git.status(Duration::ZERO).unwrap().stats;
        assert_eq!(stats.get("a.txt"), Some(&[1, 0]));
        assert!(!marker.exists(), "the repository's own command was run");
    }

    #[test]
    fn only_so_many_ignored_paths_are_told() {
        let long = "x".repeat(40_000);
        let out = format!("! {long}a\0! {long}b\0! short\0");
        let info = parse_status(out.as_bytes(), "");
        assert_eq!(info.ignored, [format!("{long}a")]);
    }

    #[test]
    fn the_records_of_git_status_are_the_branch_and_the_changes_under_the_root() {
        let out = [
            "# branch.oid 0123456789abcdef",
            "# branch.head trunk",
            "# branch.ab +2 -3",
            "1 .M N... 100644 100644 100644 a b sub/kept file.txt",
            "1 A. N... 000000 100644 100644 a b sub/new.txt",
            "1 D. N... 100644 000000 000000 a b sub/gone.txt",
            "2 R. N... 100644 100644 100644 a b R100 sub/to.txt",
            "sub/from.txt",
            "2 R. N... 100644 100644 100644 a b R100 sub/away.txt",
            "elsewhere/from.txt",
            "u UU N... 100644 100644 100644 100644 a b c sub/both.txt",
            "? sub/untracked.txt",
            "? outside.txt",
            "! sub/target/",
            "! sub/deep/one.log",
            "! elsewhere/two.log",
            "! sub/",
            "",
        ]
        .join("\0");
        let info = parse_status(out.as_bytes(), "sub/");
        assert_eq!(info.ignored, ["target", "deep/one.log"]);
        assert_eq!(info.branch, "trunk");
        assert_eq!((info.ahead, info.behind), (2, 3));
        assert_eq!(info.head, "0123456789abcdef");
        // Where a file was renamed from, if that is under the root as well.
        let renames: Vec<(&str, &str)> = info
            .renames
            .iter()
            .map(|(to, from)| (to.as_str(), from.as_str()))
            .collect();
        assert_eq!(renames, [("to.txt", "from.txt")]);
        let changes: Vec<(&str, GitState)> = info
            .changes
            .iter()
            .map(|(path, state)| (path.as_str(), *state))
            .collect();
        assert_eq!(
            changes,
            [
                ("away.txt", GitState::Renamed),
                ("both.txt", GitState::Conflict),
                ("gone.txt", GitState::Deleted),
                ("kept file.txt", GitState::Modified),
                ("new.txt", GitState::Added),
                ("to.txt", GitState::Renamed),
                ("untracked.txt", GitState::Untracked),
            ]
        );
    }

    #[test]
    fn a_branch_with_no_name_is_its_commit() {
        let info = parse_status(b"# branch.oid 0123456789abcdef\0", "");
        assert_eq!(info.branch, "0123456");
    }

    #[test]
    fn a_project_with_no_commit_has_no_head() {
        let info = parse_status(b"# branch.oid (initial)\0# branch.head main\0", "");
        assert_eq!((info.branch.as_str(), info.head.as_str()), ("main", ""));
    }

    /// A repository that was unpacked here, not cloned, brings its own config: what it names to run
    /// is not run by asking git about it.
    #[test]
    fn a_command_a_repository_names_for_git_to_run_is_not_run() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let git = |args: &[&str]| {
            let made = Command::new("git")
                .args(args)
                .current_dir(&root)
                .output()
                .unwrap();
            assert!(made.status.success(), "{args:?}: {made:?}");
        };
        git(&["init", "-q"]);
        let marker = root.join("ran");
        let hook = root.join("hook.sh");
        std::fs::write(&hook, format!("#!/bin/sh\ntouch '{}'\n", marker.display())).unwrap();
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        git(&["config", "core.fsmonitor", hook.to_str().unwrap()]);
        std::fs::write(root.join("a.txt"), "a").unwrap();
        let info = read(root.to_str().unwrap()).unwrap();
        assert!(info.changes.contains_key("a.txt"));
        assert!(!marker.exists(), "the repository's own command was run");
    }

    /// A file the last commit keeps that is too long to read is said to be, one it keeps as text is
    /// read once for as long as it is the same blob, and one it does not keep is empty.
    #[test]
    fn what_the_last_commit_keeps_of_a_file_is_its_text_or_the_word_for_why_not() {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let git = |args: &[&str]| {
            let made = Command::new("git")
                .args(["-c", "user.name=t", "-c", "user.email=t@t"])
                .args(["-c", "commit.gpgsign=false"])
                .args(args)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_SYSTEM", "/dev/null")
                .current_dir(&root)
                .output()
                .unwrap();
            assert!(made.status.success(), "{args:?}: {made:?}");
        };
        git(&["init", "-q"]);
        std::fs::write(root.join("long.txt"), "x".repeat(INDEX_LIMIT + 1)).unwrap();
        std::fs::write(root.join("text.txt"), "\u{feff}one\r\ntwo\r\n").unwrap();
        std::fs::write(root.join("bin.dat"), [1u8, 0, 2]).unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "all"]);
        let project = Git::new(root.to_str().unwrap());
        assert!(matches!(project.committed("long.txt"), Kept::TooLarge));
        assert!(matches!(project.committed("bin.dat"), Kept::Binary));
        assert!(matches!(project.committed("../x"), Kept::Outside));
        assert!(matches!(project.committed("/etc/hosts"), Kept::Outside));
        let Kept::Text(text) = project.committed("text.txt") else {
            panic!("text");
        };
        assert_eq!(
            (text.path.as_str(), text.text.as_str()),
            ("text.txt", "one\ntwo\n")
        );
        let Kept::Text(again) = project.committed("text.txt") else {
            panic!("text");
        };
        assert!(Arc::ptr_eq(&text.text, &again.text), "read from git once");
        let Kept::Text(none) = project.committed("nothing.txt") else {
            panic!("text");
        };
        assert_eq!((none.id, none.text.as_str()), (None, ""));
        let last = project.last_commit().expect("a commit");
        assert_eq!((last.subject.as_str(), last.id.len()), ("all", 40));
    }
}
