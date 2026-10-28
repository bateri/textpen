//! The open files. The program holds each one's text, not the page: the page sends what changed, a
//! few hundred milliseconds of typing at a time, and saving is a word to the program rather than the
//! whole file. The text the program holds is also what survives the page — a shell, a restart of
//! the terminal — and, written to a draft, a ⌃C or a dropped connection.
//!
//! Places in a text are code points (`edit`, `slice`, the length of a draft); the text itself is
//! UTF-8, and a place inside a character cannot be named.
//!
//! Plain values in and out: this file knows nothing of the page. An edit made on a version the
//! program no longer holds, or a buffer that is not open, is [`Error::OutOfSync`]; what the disk
//! would not do is [`Error::Failed`], in words for the person.

use std::fs;
use std::io::{self, Read as _};
use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::time::{Duration, SystemTime};

use serde::Serialize;
use serde_json::{Value, json};

use super::later::Later;
use super::store::{key_of, write_private};
use crate::project::tree::{self, Tree};
use crate::project::{ms_of, now_ms, paths, said};

/// How long after the last edit a draft is written.
const DRAFT_DELAY: Duration = Duration::from_millis(1500);

const BOM: char = '\u{feff}';

/// What a call asked of a buffer that could not be done.
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    /// The buffer is not open, or not at the version the call named, or the edit reaches outside
    /// it: the page reads it again.
    OutOfSync(String),
    /// The disk would not do it.
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Eol {
    Lf,
    Crlf,
}

/// A draft an earlier textpen left: when, and how long its text is in code points.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DraftInfo {
    pub at: u64,
    pub length: usize,
}

/// What the page is told of a buffer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Info {
    pub path: String,
    pub version: u64,
    pub dirty: bool,
    pub eol: Eol,
    pub bom: bool,
    /// Milliseconds; 0 for a file not on disk yet.
    pub mtime: u64,
    pub missing: bool,
    pub conflict: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draft: Option<DraftInfo>,
    pub readonly: bool,
}

/// A buffer with its text.
#[derive(Debug)]
pub struct Opened {
    pub info: Info,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Reason {
    Binary,
    TooLarge,
    Directory,
    Unreadable,
}

/// A file that cannot be opened as text, and why.
#[derive(Debug, Serialize)]
pub struct Unopenable {
    pub path: String,
    pub reason: Reason,
    pub size: u64,
    pub message: String,
}

/// A buffer with its text, or why the file is not one.
#[derive(Debug)]
pub enum Opening {
    Opened(Opened),
    Unopenable(Unopenable),
}

impl Opening {
    /// Whether the file opened is one that is not on disk.
    #[cfg(test)]
    fn is_opened_missing(&self) -> bool {
        matches!(self, Self::Opened(opened) if opened.info.missing)
    }
}

/// A piece of a long text, and where the next begins.
#[derive(Debug, Serialize)]
pub struct Slice {
    pub text: String,
    pub end: usize,
}

/// What a save did.
#[derive(Debug, Serialize)]
pub struct Saved {
    pub saved: bool,
    pub info: Info,
}

/// A draft an earlier textpen left.
#[derive(Debug, Clone)]
struct Draft {
    at: u64,
    text: String,
}

/// A file as text, with its line ends made `\n` and what they were remembered.
struct Read {
    text: String,
    eol: Eol,
    bom: bool,
    mtime: Option<SystemTime>,
    missing: bool,
    readonly: bool,
}

#[derive(Debug)]
struct Buffer {
    /// The name it travels by, which is also its name among the buffers.
    path: String,
    absolute: String,
    text: String,
    saved: String,
    version: u64,
    eol: Eol,
    saved_eol: Eol,
    bom: bool,
    /// When the file was last changed as the buffer knows it; none for a file not on disk yet.
    mtime: Option<SystemTime>,
    missing: bool,
    conflict: bool,
    readonly: bool,
    draft: Option<Draft>,
}

impl Buffer {
    fn is_dirty(&self) -> bool {
        self.text != self.saved || self.eol != self.saved_eol
    }

    fn info(&self) -> Info {
        Info {
            path: self.path.clone(),
            version: self.version,
            dirty: self.is_dirty(),
            eol: self.eol,
            bom: self.bom,
            mtime: self.mtime.map_or(0, ms_of),
            missing: self.missing,
            conflict: self.conflict,
            draft: self.draft.as_ref().map(|draft| DraftInfo {
                at: draft.at,
                length: draft.text.chars().count(),
            }),
            readonly: self.readonly,
        }
    }

    fn opened(&self) -> Opened {
        Opened {
            info: self.info(),
            text: self.text.clone(),
        }
    }

    fn reset_from(&mut self, read: Read) {
        self.text.clone_from(&read.text);
        self.saved = read.text;
        self.eol = read.eol;
        self.saved_eol = read.eol;
        self.bom = read.bom;
        self.mtime = read.mtime;
        self.missing = read.missing;
        self.conflict = false;
    }
}

/// The byte at which the code point `place` of `text` begins; the length of the text for its end,
/// none past it.
fn byte_of(text: &str, place: usize) -> Option<usize> {
    text.char_indices()
        .map(|(at, _)| at)
        .chain(std::iter::once(text.len()))
        .nth(place)
}

/// What is left of the buffer name `path` when it is `from` or under it: "" for `from` itself, and
/// `/rest` for a file under it.
fn under<'a>(path: &'a str, from: &str) -> Option<&'a str> {
    if path == from {
        Some("")
    } else {
        path.strip_prefix(from).filter(|rest| rest.starts_with('/'))
    }
}

/// Whether a file can be written to by this program.
fn writable(path: &str) -> bool {
    let Ok(path) = std::ffi::CString::new(path) else {
        return false;
    };
    // SAFETY: the path is a C string of ours, which outlives the call.
    unsafe { libc::access(path.as_ptr(), libc::W_OK) == 0 }
}

fn unopenable(path: &str, reason: Reason, size: u64, message: String) -> Unopenable {
    Unopenable {
        path: path.to_owned(),
        reason,
        size,
        message,
    }
}

/// A file as text, or why it is not one; a file that is not there is an empty text.
fn read_file_text(absolute: &str, name: &str) -> Result<Read, Unopenable> {
    let meta = match fs::metadata(absolute) {
        Ok(meta) => meta,
        // A file that is not there yet: it opens empty and comes to be when it is saved.
        Err(why) if why.kind() == io::ErrorKind::NotFound => {
            return Ok(Read {
                text: String::new(),
                eol: Eol::Lf,
                bom: false,
                mtime: None,
                missing: true,
                readonly: false,
            });
        }
        Err(why) => return Err(unopenable(name, Reason::Unreadable, 0, said(&why))),
    };
    if meta.is_dir() {
        return Err(unopenable(
            name,
            Reason::Directory,
            0,
            format!("{name} is a directory"),
        ));
    }
    // A pipe, a socket or a device is not a file to read to its end: a pipe waits for whoever writes
    // to it, with the lock of every buffer held, and `/dev/zero` has no end.
    if !meta.is_file() {
        return Err(unopenable(
            name,
            Reason::Unreadable,
            0,
            format!("{name} is not a regular file"),
        ));
    }
    let size = meta.len();
    if size > tree::TEXT_LIMIT {
        // Sizes a person reads, to a tenth of a megabyte.
        #[allow(clippy::cast_precision_loss)]
        let (megabytes, limit) = (
            size as f64 / 1024.0 / 1024.0,
            tree::TEXT_LIMIT / 1024 / 1024,
        );
        return Err(unopenable(
            name,
            Reason::TooLarge,
            size,
            format!("{name} is {megabytes:.1} MB; textpen opens files up to {limit} MB"),
        ));
    }
    // No more is read than the limit allows (and one byte more, to know a file grew past it since
    // its size was asked).
    let mut bytes = Vec::new();
    fs::File::open(absolute)
        .and_then(|file| file.take(tree::TEXT_LIMIT + 1).read_to_end(&mut bytes))
        .map_err(|why| unopenable(name, Reason::Unreadable, size, said(&why)))?;
    if bytes.len() as u64 > tree::TEXT_LIMIT {
        return Err(unopenable(
            name,
            Reason::TooLarge,
            bytes.len() as u64,
            format!(
                "{name} is larger than {} MB; textpen opens files up to that",
                tree::TEXT_LIMIT / 1024 / 1024
            ),
        ));
    }
    if tree::is_binary(&bytes) {
        return Err(unopenable(
            name,
            Reason::Binary,
            size,
            format!("{name} is not text"),
        ));
    }
    let Ok(raw) = String::from_utf8(bytes) else {
        return Err(unopenable(
            name,
            Reason::Binary,
            size,
            format!("{name} is not UTF-8 text"),
        ));
    };
    // A byte order mark is taken off, and put back when the file is written.
    let (bom, raw) = match raw.strip_prefix(BOM) {
        Some(rest) => (true, rest),
        None => (false, raw.as_str()),
    };
    let crlf = raw.matches("\r\n").count();
    let lf = raw.matches('\n').count() - crlf;
    Ok(Read {
        text: raw.replace("\r\n", "\n").replace('\r', "\n"),
        eol: if crlf > lf { Eol::Crlf } else { Eol::Lf },
        bom,
        mtime: meta.modified().ok(),
        missing: false,
        readonly: !writable(absolute),
    })
}

/// The open files, in the order they were opened, and the drafts of their changes.
pub struct Buffers {
    project: Arc<Tree>,
    /// Where the drafts are kept.
    drafts: String,
    list: Vec<Buffer>,
    /// The drafts to write, by the absolute path of the file.
    later: Later<String>,
}

/// The buffers, behind the lock every call about them takes. A draft is written by a thread of its
/// own, which takes the lock as a call does.
pub fn shared(project: Arc<Tree>, state_directory: &str) -> Arc<Mutex<Buffers>> {
    let drafts = paths::join(state_directory, "drafts");
    Arc::new_cyclic(|me: &Weak<Mutex<Buffers>>| {
        let me = me.clone();
        let later = Later::new("textpen-drafts", move |absolute: String| {
            if let Some(buffers) = me.upgrade() {
                buffers
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .draft_due(&absolute);
            }
        });
        Mutex::new(Buffers {
            project,
            drafts,
            list: Vec::new(),
            later,
        })
    })
}

impl Buffers {
    fn find(&self, path: &str) -> Option<&Buffer> {
        self.list.iter().find(|buffer| buffer.path == path)
    }

    fn get(&mut self, path: &str) -> Result<&mut Buffer, Error> {
        self.list
            .iter_mut()
            .find(|buffer| buffer.path == path)
            .ok_or_else(|| Error::OutOfSync(format!("{path} is not open")))
    }

    /// The buffer of `path` at `version`.
    fn at(&mut self, path: &str, version: i64) -> Result<&mut Buffer, Error> {
        let buffer = self.get(path)?;
        if i64::try_from(buffer.version).ok() != Some(version) {
            return Err(Error::OutOfSync(format!(
                "{path} is at version {}, not {version}",
                buffer.version
            )));
        }
        Ok(buffer)
    }

    pub fn all(&self) -> Vec<Info> {
        self.list.iter().map(Buffer::info).collect()
    }

    fn draft_file(&self, absolute: &str) -> String {
        paths::join(&self.drafts, &format!("{}.json", key_of(absolute)))
    }

    fn read_draft(&self, absolute: &str) -> Option<Draft> {
        let read: Value =
            serde_json::from_slice(&fs::read(self.draft_file(absolute)).ok()?).ok()?;
        if read.get("path").and_then(Value::as_str) != Some(absolute) {
            return None;
        }
        Some(Draft {
            text: read.get("text")?.as_str()?.to_owned(),
            at: read.get("at").and_then(Value::as_u64).unwrap_or(0),
        })
    }

    fn remove_draft(&self, absolute: &str) {
        let _ = fs::remove_file(self.draft_file(absolute));
    }

    fn write_draft(&self, buffer: &Buffer) {
        let draft = json!({ "path": buffer.absolute, "text": buffer.text, "at": now_ms() });
        let _ = write_private(
            &self.draft_file(&buffer.absolute),
            draft.to_string().as_bytes(),
        );
    }

    /// A draft of `absolute` is due: written if its buffer has changes, removed if it has none.
    fn draft_due(&self, absolute: &str) {
        if let Some(buffer) = self.list.iter().find(|buffer| buffer.absolute == absolute) {
            if buffer.text == buffer.saved {
                self.remove_draft(absolute);
            } else {
                self.write_draft(buffer);
            }
        }
    }

    /// The buffer of a file, read from the disk if it is not open; a missing file opens empty.
    pub fn open(&mut self, path: &str) -> Opening {
        let absolute = self.project.absolute(path);
        let name = self.project.name(&absolute);
        if let Some(open) = self.find(&name) {
            return Opening::Opened(open.opened());
        }
        let read = match read_file_text(&absolute, &name) {
            Ok(read) => read,
            Err(unopenable) => return Opening::Unopenable(unopenable),
        };
        let mut buffer = Buffer {
            path: name,
            text: read.text.clone(),
            saved: read.text,
            version: 1,
            eol: read.eol,
            saved_eol: read.eol,
            bom: read.bom,
            mtime: read.mtime,
            missing: read.missing,
            conflict: false,
            readonly: read.readonly,
            draft: None,
            absolute,
        };
        match self.read_draft(&buffer.absolute) {
            Some(draft) if draft.text != buffer.text => buffer.draft = Some(draft),
            Some(_) => self.remove_draft(&buffer.absolute),
            None => {}
        }
        let opened = buffer.opened();
        self.list.push(buffer);
        Opening::Opened(opened)
    }

    /// The piece of a buffer's text that begins at the code point `from` and takes at most `limit`
    /// bytes (one character at least, so that reading goes on), and where the next piece begins.
    pub fn slice(
        &mut self,
        path: &str,
        version: i64,
        from: usize,
        limit: usize,
    ) -> Result<Slice, Error> {
        let content = &self.at(path, version)?.text;
        let Some(start) = byte_of(content, from) else {
            return Ok(Slice {
                text: String::new(),
                end: from,
            });
        };
        let (mut used, mut count) = (0, 0);
        for ch in content.get(start..).unwrap_or_default().chars() {
            if used + ch.len_utf8() > limit && count > 0 {
                break;
            }
            used += ch.len_utf8();
            count += 1;
        }
        Ok(Slice {
            text: content
                .get(start..start + used)
                .unwrap_or_default()
                .to_owned(),
            end: from + count,
        })
    }

    /// What the marks of the gutter compare with git's: the version and the text of an open buffer
    /// whose file is on disk, none for one that is not or whose text is longer than `limit` bytes.
    pub fn for_diff(&mut self, path: &str, limit: usize) -> Result<Option<(u64, String)>, Error> {
        let buffer = self.get(path)?;
        if buffer.missing || buffer.text.len() > limit {
            return Ok(None);
        }
        Ok(Some((buffer.version, buffer.text.clone())))
    }

    /// The current text of an open buffer.
    pub fn text(&mut self, path: &str) -> Result<Opened, Error> {
        Ok(self.get(path)?.opened())
    }

    /// Applies the page's edits, made on version `base`, in order: each is `(from, to, insert)` in
    /// code points of the text the one before left.
    pub fn edit(
        &mut self,
        path: &str,
        base: i64,
        edits: &[(i64, i64, String)],
    ) -> Result<Info, Error> {
        let buffer = self.at(path, base)?;
        let outside = || Error::OutOfSync(format!("An edit of {path} reaches outside it"));
        let mut content = buffer.text.clone();
        for (from, to, insert) in edits {
            let (Ok(from), Ok(to)) = (usize::try_from(*from), usize::try_from(*to)) else {
                return Err(outside());
            };
            if from > to {
                return Err(outside());
            }
            let first = byte_of(&content, from).ok_or_else(outside)?;
            let after = content
                .get(first..)
                .and_then(|rest| byte_of(rest, to - from))
                .ok_or_else(outside)?;
            content.replace_range(first..first + after, insert);
        }
        buffer.text = content;
        buffer.version += 1;
        let (absolute, info) = (buffer.absolute.clone(), buffer.info());
        self.later.after(absolute, DRAFT_DELAY);
        Ok(info)
    }

    /// Writes the buffer to its file. A file that changed on disk since it was read is not
    /// overwritten unless `force`: the answer says so, and the page asks.
    pub fn save(&mut self, path: &str, version: i64, force: bool) -> Result<Saved, Error> {
        let buffer = self.at(path, version)?;
        if !force {
            // A file that was not there when it was opened (`mtime` none) is in conflict if it is
            // now: whoever made it has not been asked, whether or not the program has looked yet.
            let now = tree::modified(&buffer.absolute);
            if now.is_some() && now != buffer.mtime {
                buffer.conflict = true;
                return Ok(Saved {
                    saved: false,
                    info: buffer.info(),
                });
            }
        }
        let mut body = String::with_capacity(buffer.text.len() + 3);
        if buffer.bom {
            body.push(BOM);
        }
        match buffer.eol {
            Eol::Lf => body.push_str(&buffer.text),
            Eol::Crlf => body.push_str(&buffer.text.replace('\n', "\r\n")),
        }
        let fail =
            |why: io::Error| Error::Failed(format!("{path} could not be saved: {}", said(&why)));
        fs::create_dir_all(paths::dirname(&buffer.absolute)).map_err(fail)?;
        fs::write(&buffer.absolute, body).map_err(fail)?;
        buffer.mtime = tree::modified(&buffer.absolute).or_else(|| Some(SystemTime::now()));
        buffer.saved.clone_from(&buffer.text);
        buffer.saved_eol = buffer.eol;
        buffer.missing = false;
        buffer.conflict = false;
        buffer.readonly = false;
        // The draft an earlier run left is gone with the one this run wrote: none is offered now.
        buffer.draft = None;
        let (absolute, info) = (buffer.absolute.clone(), buffer.info());
        self.later.cancel(&absolute);
        self.remove_draft(&absolute);
        self.project.forget();
        Ok(Saved { saved: true, info })
    }

    /// Reads the file again, dropping the buffer's changes and its draft.
    pub fn revert(&mut self, path: &str) -> Result<Opening, Error> {
        let buffer = self.get(path)?;
        let read = match read_file_text(&buffer.absolute, &buffer.path) {
            Ok(read) => read,
            Err(unopenable) => return Ok(Opening::Unopenable(unopenable)),
        };
        buffer.version += 1;
        buffer.readonly = read.readonly;
        buffer.reset_from(read);
        buffer.draft = None;
        let (absolute, opened) = (buffer.absolute.clone(), buffer.opened());
        self.later.cancel(&absolute);
        self.remove_draft(&absolute);
        Ok(Opening::Opened(opened))
    }

    /// Keeps the buffer's changes over the file that changed on disk; the next save writes them.
    pub fn keep(&mut self, path: &str) -> Result<Info, Error> {
        let buffer = self.get(path)?;
        buffer.mtime = tree::modified(&buffer.absolute).or(buffer.mtime);
        buffer.conflict = false;
        Ok(buffer.info())
    }

    /// Takes the draft an earlier textpen left, or drops it.
    pub fn draft(&mut self, path: &str, take: bool) -> Result<Opened, Error> {
        let buffer = self.get(path)?;
        let draft = buffer.draft.take();
        let absolute = match draft {
            Some(draft) if take => {
                buffer.text = draft.text;
                buffer.version += 1;
                None
            }
            _ => Some(buffer.absolute.clone()),
        };
        let opened = buffer.opened();
        if let Some(absolute) = absolute {
            self.remove_draft(&absolute);
        }
        Ok(opened)
    }

    /// Changes the line ends the buffer is written with.
    pub fn set_eol(&mut self, path: &str, eol: Eol) -> Result<Info, Error> {
        let buffer = self.get(path)?;
        buffer.eol = eol;
        Ok(buffer.info())
    }

    /// Closes a buffer; one with changes only when they are to be dropped, with their draft.
    pub fn close(&mut self, path: &str, discard: bool) -> Result<(), Error> {
        let Some((at, buffer)) = self
            .list
            .iter()
            .enumerate()
            .find(|(_, buffer)| buffer.path == path)
        else {
            return Ok(());
        };
        if buffer.is_dirty() && !discard {
            return Err(Error::Failed(format!(
                "{path} has changes that are not saved"
            )));
        }
        let absolute = buffer.absolute.clone();
        self.later.cancel(&absolute);
        if discard {
            self.remove_draft(&absolute);
        }
        self.list.remove(at);
        Ok(())
    }

    /// The name of a buffer with changes not saved that moving `from` to `to` would leave without its
    /// file: it is of a name a moved buffer would have, a file that is not on disk (or the move would
    /// not be allowed), and `moved` drops it. Asked before the move, so that nothing is lost.
    pub fn unsaved_in_the_way(&self, from: &str, to: &str) -> Option<String> {
        let arrivals: Vec<String> = self
            .list
            .iter()
            .filter_map(|buffer| under(&buffer.path, from))
            .map(|rest| {
                self.project
                    .name(&self.project.absolute(&format!("{to}{rest}")))
            })
            .collect();
        self.list
            .iter()
            .find(|buffer| {
                under(&buffer.path, from).is_none()
                    && buffer.is_dirty()
                    && arrivals.contains(&buffer.path)
            })
            .map(|buffer| buffer.path.clone())
    }

    /// After a rename on disk: the buffers of the file, or of the files under the directory, follow.
    pub fn moved(&mut self, from: &str, to: &str) {
        // The names the moved buffers have now, and whether each buffer is one of them.
        let (mut taken, mut moved, mut drafts) = (Vec::new(), Vec::new(), Vec::new());
        for buffer in &mut self.list {
            let rest = under(&buffer.path, from);
            moved.push(rest.is_some());
            if let Some(rest) = rest {
                let was = std::mem::replace(
                    &mut buffer.absolute,
                    self.project.absolute(&format!("{to}{rest}")),
                );
                buffer.path = self.project.name(&buffer.absolute);
                taken.push(buffer.path.clone());
                let changed = buffer.text != buffer.saved;
                drafts.push((was, changed.then(|| buffer.absolute.clone())));
            }
        }
        // A buffer of the name a moved one has now was of a file that is not there any more.
        let mut moved = moved.into_iter();
        self.list
            .retain(|buffer| moved.next().unwrap_or(false) || !taken.contains(&buffer.path));
        // The draft goes with the file, at the time it had.
        for (was, now) in drafts {
            self.later.cancel(&was);
            self.remove_draft(&was);
            if let Some(now) = now {
                self.later.after(now, DRAFT_DELAY);
            }
        }
    }

    /// The buffers whose file changed on disk since it was read. One without changes of its own is
    /// read again (its version goes up when its text does); one with changes is marked as in
    /// conflict, for the page to ask.
    pub fn changed_on_disk(&mut self) -> Vec<Info> {
        let mut changed = Vec::new();
        for buffer in &mut self.list {
            let Some(now) = tree::modified(&buffer.absolute) else {
                if !buffer.missing && buffer.mtime.is_some() {
                    buffer.missing = true;
                    changed.push(buffer.info());
                }
                continue;
            };
            if Some(now) == buffer.mtime && !buffer.missing {
                continue;
            }
            if !buffer.is_dirty() {
                if let Ok(read) = read_file_text(&buffer.absolute, &buffer.path) {
                    if read.text != buffer.text {
                        buffer.version += 1;
                    }
                    buffer.reset_from(read);
                    buffer.missing = false;
                    changed.push(buffer.info());
                }
            } else if !buffer.conflict {
                buffer.conflict = true;
                buffer.missing = false;
                changed.push(buffer.info());
            }
        }
        changed
    }

    /// Writes a draft of every buffer with changes, now: the program is about to end.
    pub fn flush_drafts(&self) {
        for buffer in &self.list {
            self.later.cancel(&buffer.absolute);
            if buffer.text != buffer.saved {
                self.write_draft(buffer);
            }
        }
    }

    /// Drops every change that is not saved, and its draft: the person quit without saving.
    pub fn discard_all(&mut self) {
        for buffer in &mut self.list {
            buffer.saved.clone_from(&buffer.text);
            buffer.saved_eol = buffer.eol;
        }
        for buffer in &self.list {
            self.later.cancel(&buffer.absolute);
            self.remove_draft(&buffer.absolute);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_place_is_a_code_point_and_the_end_is_the_length() {
        let text = "aş😀z";
        assert_eq!(byte_of(text, 0), Some(0));
        assert_eq!(byte_of(text, 2), Some(3));
        assert_eq!(byte_of(text, 3), Some(7));
        assert_eq!(byte_of(text, 4), Some(8));
        assert_eq!(byte_of(text, 5), None);
    }

    fn project() -> (tempfile::TempDir, Arc<Mutex<Buffers>>) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap().to_owned();
        let buffers = shared(Arc::new(Tree::new(&root)), &format!("{root}/state"));
        (dir, buffers)
    }

    #[test]
    fn edits_count_code_points_and_apply_in_order() {
        let (dir, buffers) = project();
        fs::write(dir.path().join("u.txt"), "\u{feff}aş😀z\r\nlast\r\n").unwrap();
        let mut buffers = buffers.lock().unwrap();
        let Opening::Opened(opened) = buffers.open("u.txt") else {
            panic!("not opened");
        };
        assert_eq!(opened.text, "aş😀z\nlast\n");
        assert_eq!((opened.info.eol, opened.info.bom), (Eol::Crlf, true));
        let edits = [(3, 4, "Z".to_owned()), (2, 3, "<>".to_owned())];
        let info = buffers.edit("u.txt", 1, &edits).unwrap();
        assert_eq!((info.version, info.dirty), (2, true));
        assert_eq!(
            buffers.edit("u.txt", 2, &[(0, 99, "x".to_owned())]),
            Err(Error::OutOfSync(
                "An edit of u.txt reaches outside it".to_owned()
            ))
        );
        assert_eq!(
            buffers.edit("u.txt", 2, &[(-1, 0, "x".to_owned())]),
            Err(Error::OutOfSync(
                "An edit of u.txt reaches outside it".to_owned()
            ))
        );
        assert!(buffers.save("u.txt", 2, false).unwrap().saved);
        assert_eq!(
            fs::read_to_string(dir.path().join("u.txt")).unwrap(),
            "\u{feff}aş<>Z\r\nlast\r\n"
        );
    }

    #[test]
    fn a_long_text_is_read_in_pieces_that_end_between_characters() {
        let (dir, buffers) = project();
        fs::write(dir.path().join("e.txt"), "😀".repeat(10)).unwrap();
        let mut buffers = buffers.lock().unwrap();
        buffers.open("e.txt");
        let first = buffers.slice("e.txt", 1, 0, 10).unwrap();
        assert_eq!((first.text.as_str(), first.end), ("😀😀", 2));
        let next = buffers.slice("e.txt", 1, 2, 3).unwrap();
        assert_eq!((next.text.as_str(), next.end), ("😀", 3));
        let past = buffers.slice("e.txt", 1, 12, 10).unwrap();
        assert_eq!((past.text.as_str(), past.end), ("", 12));
        assert!(matches!(
            buffers.slice("e.txt", 2, 0, 10),
            Err(Error::OutOfSync(_))
        ));
    }

    #[test]
    fn a_draft_is_kept_and_offered_with_its_length_in_code_points() {
        let (dir, shared_buffers) = project();
        fs::write(dir.path().join("a.txt"), "world").unwrap();
        {
            let mut buffers = shared_buffers.lock().unwrap();
            buffers.open("a.txt");
            buffers
                .edit("a.txt", 1, &[(5, 5, " 😀!".to_owned())])
                .unwrap();
            buffers.flush_drafts();
        }
        let root = dir.path().to_str().unwrap();
        let again = shared(Arc::new(Tree::new(root)), &format!("{root}/state"));
        let mut again = again.lock().unwrap();
        let Opening::Opened(opened) = again.open("a.txt") else {
            panic!("not opened");
        };
        assert_eq!(opened.info.draft.map(|draft| draft.length), Some(8));
        let restored = again.draft("a.txt", true).unwrap();
        assert_eq!(
            (restored.text.as_str(), restored.info.version),
            ("world 😀!", 2)
        );
    }

    /// A rename moves the draft that is due and the one on disk to the new name; a close that drops
    /// the changes right after drops them under it, and no draft is written later under either.
    #[test]
    fn a_renamed_buffer_closed_with_its_changes_dropped_leaves_no_draft_under_either_name() {
        let (dir, shared_buffers) = project();
        fs::write(dir.path().join("a.txt"), "world").unwrap();
        {
            let mut buffers = shared_buffers.lock().unwrap();
            buffers.open("a.txt");
            buffers.edit("a.txt", 1, &[(5, 5, "!".to_owned())]).unwrap();
            // A draft of the old name on disk, and another one due.
            buffers.flush_drafts();
            buffers.edit("a.txt", 2, &[(6, 6, "?".to_owned())]).unwrap();
            fs::rename(dir.path().join("a.txt"), dir.path().join("b.txt")).unwrap();
            buffers.moved("a.txt", "b.txt");
            buffers.close("b.txt", true).unwrap();
        }
        std::thread::sleep(DRAFT_DELAY + Duration::from_millis(500));
        let left: Vec<_> = fs::read_dir(dir.path().join("state/drafts"))
            .map(|drafts| drafts.flatten().map(|draft| draft.file_name()).collect())
            .unwrap_or_default();
        assert!(left.is_empty(), "drafts left: {left:?}");
    }

    #[test]
    fn a_buffer_follows_its_file_and_the_files_under_a_directory() {
        let (dir, buffers) = project();
        fs::create_dir(dir.path().join("d")).unwrap();
        fs::write(dir.path().join("d/x.txt"), "x").unwrap();
        fs::write(dir.path().join("y.txt"), "y").unwrap();
        let mut buffers = buffers.lock().unwrap();
        buffers.open("d/x.txt");
        buffers.open("y.txt");
        buffers.moved("d", "e/f");
        buffers.moved("y.txt", "z.txt");
        let paths: Vec<String> = buffers.all().into_iter().map(|info| info.path).collect();
        assert_eq!(paths, ["e/f/x.txt", "z.txt"]);
    }

    #[test]
    fn a_file_that_appeared_since_a_missing_one_was_opened_is_not_overwritten() {
        let (dir, buffers) = project();
        let mut buffers = buffers.lock().unwrap();
        assert!(buffers.open("new.txt").is_opened_missing());
        buffers
            .edit("new.txt", 1, &[(0, 0, "mine".to_owned())])
            .unwrap();
        // Another program makes the file before the program has looked at the disk again.
        fs::write(dir.path().join("new.txt"), "theirs").unwrap();
        let saved = buffers.save("new.txt", 2, false).unwrap();
        assert!(!saved.saved && saved.info.conflict);
        assert_eq!(
            fs::read_to_string(dir.path().join("new.txt")).unwrap(),
            "theirs"
        );
        // Told to, it is overwritten; and a file that is still not there is made as before.
        assert!(buffers.save("new.txt", 2, true).unwrap().saved);
        assert!(buffers.open("other.txt").is_opened_missing());
        assert!(buffers.save("other.txt", 1, false).unwrap().saved);
    }

    #[test]
    fn a_save_leaves_no_draft_of_an_earlier_run_to_be_offered() {
        let (dir, shared_buffers) = project();
        fs::write(dir.path().join("a.txt"), "world").unwrap();
        {
            let mut buffers = shared_buffers.lock().unwrap();
            buffers.open("a.txt");
            buffers.edit("a.txt", 1, &[(5, 5, "!".to_owned())]).unwrap();
            buffers.flush_drafts();
        }
        let root = dir.path().to_str().unwrap();
        let again = shared(Arc::new(Tree::new(root)), &format!("{root}/state"));
        let mut again = again.lock().unwrap();
        let Opening::Opened(opened) = again.open("a.txt") else {
            panic!("not opened");
        };
        assert!(opened.info.draft.is_some());
        let edited = again
            .edit("a.txt", 1, &[(0, 0, "new ".to_owned())])
            .unwrap();
        let saved = again
            .save("a.txt", edited.version.try_into().unwrap(), false)
            .unwrap();
        assert!(
            saved.saved && saved.info.draft.is_none(),
            "{:?}",
            saved.info.draft
        );
        // Taking a draft that is gone does not put its text over what was saved.
        assert_eq!(again.draft("a.txt", true).unwrap().text, "new world");
    }

    #[test]
    fn a_pipe_is_not_opened_as_a_file_to_be_read_to_its_end() {
        let (dir, buffers) = project();
        let pipe = dir.path().join("pipe");
        let c_path = std::ffi::CString::new(pipe.to_str().unwrap()).unwrap();
        // SAFETY: a C string of ours, which outlives the call.
        assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);
        // Whoever writes to it, as a pipe in a project may have someone to.
        let writer = {
            let pipe = pipe.clone();
            std::thread::spawn(move || fs::write(pipe, "text"))
        };
        let opening = buffers.lock().unwrap().open("pipe");
        let Opening::Unopenable(unopenable) = opening else {
            panic!("a pipe was opened");
        };
        assert_eq!(unopenable.reason, Reason::Unreadable);
        // Let the writer go.
        let _ = fs::read(&pipe);
        writer.join().unwrap().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_draft_is_for_its_owner_alone_and_is_written_whole() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, shared_buffers) = project();
        fs::write(dir.path().join("secret.txt"), "key").unwrap();
        let mut buffers = shared_buffers.lock().unwrap();
        buffers.open("secret.txt");
        buffers
            .edit("secret.txt", 1, &[(3, 3, "!".to_owned())])
            .unwrap();
        buffers.flush_drafts();
        let drafts = dir.path().join("state/drafts");
        let mode =
            |path: &std::path::Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&drafts), 0o700);
        let files: Vec<_> = fs::read_dir(&drafts).unwrap().flatten().collect();
        assert_eq!(
            files.len(),
            1,
            "no half-written file is left beside the draft"
        );
        assert_eq!(mode(&files[0].path()), 0o600);
        assert!(buffers.read_draft(&buffers.list[0].absolute).is_some());
    }

    #[test]
    fn a_move_onto_a_buffer_with_changes_not_saved_is_seen_before_it_is_made() {
        let (dir, buffers) = project();
        fs::write(dir.path().join("a.txt"), "a").unwrap();
        fs::create_dir(dir.path().join("d")).unwrap();
        fs::write(dir.path().join("d/x.txt"), "x").unwrap();
        let mut buffers = buffers.lock().unwrap();
        buffers.open("a.txt");
        buffers.open("d/x.txt");
        // A file typed in a buffer and not saved yet is not on disk, so a move may be made to it.
        buffers.open("b.txt");
        assert_eq!(
            buffers.unsaved_in_the_way("a.txt", "b.txt"),
            None,
            "no changes yet"
        );
        buffers
            .edit("b.txt", 1, &[(0, 0, "mine".to_owned())])
            .unwrap();
        assert_eq!(
            buffers.unsaved_in_the_way("a.txt", "b.txt"),
            Some("b.txt".to_owned())
        );
        assert_eq!(buffers.unsaved_in_the_way("d", "e"), None);
        buffers.open("e/x.txt");
        buffers
            .edit("e/x.txt", 1, &[(0, 0, "mine".to_owned())])
            .unwrap();
        assert_eq!(
            buffers.unsaved_in_the_way("d", "e"),
            Some("e/x.txt".to_owned())
        );
        // A buffer that moves with the others is not in their way.
        assert_eq!(buffers.unsaved_in_the_way("e", "d"), None);
    }
}
