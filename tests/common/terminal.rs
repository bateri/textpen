//! A terminal that hosts nothing, for the program to be started in: a pty with a window of a size, which
//! answers what a terminal answers (the colours, the keyboard, the device attributes, and the Kitty graphics
//! protocol where it has it), types what a person types and reads what the program draws. What is read is
//! the text of the screen with what moves the cursor and sets the colours taken out of it.

use std::ffi::CString;
use std::os::fd::RawFd;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// What a terminal that has the Kitty keyboard protocol and a dark ground answers to the questions the
/// program asks when it starts: the text colour and ground, the sixteen of the palette, the keyboard, and,
/// where it draws pictures, the picture it was asked to take; the device attributes last.
fn answer(graphics: bool, then: &[u8]) -> Vec<u8> {
    let mut out = String::from("\x1b]10;rgb:cccc/cccc/cccc\x1b\\\x1b]11;rgb:1e1e/1e1e/2e2e\x1b\\");
    for index in 0..16 {
        out.push_str(&format!("\x1b]4;{index};rgb:8080/8080/8080\x1b\\"));
    }
    out.push_str("\x1b[?0u");
    if graphics {
        out.push_str("\x1b_Gi=31;OK\x1b\\");
    }
    let mut out = out.into_bytes();
    out.extend_from_slice(then);
    out.extend_from_slice(b"\x1b[?62;4c");
    out
}

/// What a terminal that draws pictures in pixels answers in the round of questions after the first, each
/// asked once as it is seen in what the program wrote: how many pixels a cell is (10 by 20) and a window
/// (a cell each way times the size of the pty), that it has the pointer in pixels, and that it edits a part
/// of a picture and copies a part of it onto another. Not that it takes packed pixels: they come as they are.
fn pixel_answers(raw: &[u8], size: (u16, u16), answered: &mut [usize; 5]) -> Vec<u8> {
    let (columns, rows) = (u32::from(size.0), u32::from(size.1));
    let questions: [(&[u8], String); 5] = [
        (b"\x1b[16t", "\x1b[6;20;10t".to_owned()),
        (
            b"\x1b[14t",
            format!("\x1b[4;{};{}t", rows * 20, columns * 10),
        ),
        (b"\x1b[?1016$p", "\x1b[?1016;2$y".to_owned()),
        (b"\x1b_Ga=f,i=33,", "\x1b_Gi=33;OK\x1b\\".to_owned()),
        (b"\x1b_Ga=c,i=34,", "\x1b_Gi=34;OK\x1b\\".to_owned()),
    ];
    let mut out = Vec::new();
    for (at, (question, answer)) in questions.iter().enumerate() {
        let asked = count(raw, question);
        for _ in answered[at]..asked {
            out.extend_from_slice(answer.as_bytes());
        }
        answered[at] = asked;
    }
    out
}

/// What the thread that reads the master has read, and how it stands.
#[derive(Default)]
struct Taken {
    /// What it has read that `pump` has not taken yet.
    bytes: Mutex<Vec<u8>>,
    /// That the master has said there is no more to read: the program's side of the pty is closed.
    closed: AtomicBool,
    /// That the terminal is let go of, and the thread ends.
    stop: AtomicBool,
}

/// Reads the master from the moment the program starts, for as long as it writes, and keeps what it reads
/// for `pump`. A pty holds a few kilobytes, so a program that writes a picture waits for its reader every
/// few. A reader that came round only when a test looked, and left after a few quiet milliseconds, held the
/// program to under a megabyte a second (a picture of the window took seconds to be written, and the pointer
/// and the keys, which it reads between two pictures, waited as long); one that is always reading holds it
/// to nothing.
fn drain(master: RawFd, taken: &Taken) {
    let mut buffer = vec![0_u8; 65536];
    while !taken.stop.load(Ordering::Acquire) {
        let mut waiting = libc::pollfd {
            fd: master,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: a wait on the one descriptor this was given, which stays open until this has ended.
        unsafe { libc::poll(&raw mut waiting, 1, 50) };
        while !taken.stop.load(Ordering::Acquire) {
            // SAFETY: a read into a buffer of this size, from the master.
            let n = unsafe { libc::read(master, buffer.as_mut_ptr().cast(), buffer.len()) };
            if n > 0 {
                taken
                    .bytes
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .extend_from_slice(&buffer[..n as usize]);
                continue;
            }
            let more_later = n < 0
                && matches!(
                    std::io::Error::last_os_error().raw_os_error(),
                    Some(libc::EAGAIN | libc::EINTR)
                );
            if !more_later {
                // The end of the file, or the error a pty gives once the program's side of it is closed.
                taken.closed.store(true, Ordering::Release);
                return;
            }
            break;
        }
    }
}

/// The program, running on a pty.
pub struct Terminal {
    master: RawFd,
    child: libc::pid_t,
    taken: Arc<Taken>,
    reader: Option<JoinHandle<()>>,
    graphics: bool,
    size: (u16, u16),
    /// How many questions of the pixels the terminal has answered (`pixel_answers`), and how many rounds of
    /// questions it has closed (the device attributes, which end each).
    pixels_answered: [usize; 5],
    fenced: usize,
    /// How much of `raw` has been looked through for the end of a round (a long run is not read again).
    looked: usize,
    /// How many ends of a round are in what was looked through.
    fences_seen: usize,
    /// Everything the program wrote, as it wrote it.
    pub raw: Vec<u8>,
    status: Option<i32>,
}

/// What the program is run with.
pub struct Start<'a> {
    pub program: &'a str,
    pub args: &'a [&'a str],
    pub dir: &'a Path,
    pub env: &'a [(&'a str, &'a str)],
    pub env_remove: &'a [&'a str],
    pub size: (u16, u16),
    /// Whether the terminal draws pictures (answers the Kitty graphics query).
    pub graphics: bool,
}

/// How long what waits for the program leaves between one look at what it has written and the next.
const PAUSE: Duration = Duration::from_millis(5);

impl Terminal {
    /// Starts the program with the pty as its terminal and the controlling one of a session of its own.
    pub fn start(start: &Start) -> Self {
        let program = CString::new(start.program).expect("a program name");
        let mut args = vec![program.clone()];
        for arg in start.args {
            args.push(CString::new(*arg).expect("an argument"));
        }
        let mut env: Vec<(String, String)> = std::env::vars()
            .filter(|(name, _)| !start.env_remove.contains(&name.as_str()))
            .collect();
        for (name, value) in start.env {
            env.retain(|(have, _)| have != name);
            env.push(((*name).to_owned(), (*value).to_owned()));
        }
        let pairs: Vec<CString> = env
            .iter()
            .map(|(name, value)| CString::new(format!("{name}={value}")).expect("a variable"))
            .collect();
        let dir = CString::new(start.dir.to_string_lossy().as_bytes()).expect("a directory");
        let mut argv: Vec<*const libc::c_char> = args.iter().map(|arg| arg.as_ptr()).collect();
        argv.push(std::ptr::null());
        let mut envp: Vec<*const libc::c_char> = pairs.iter().map(|pair| pair.as_ptr()).collect();
        envp.push(std::ptr::null());
        let mut window = libc::winsize {
            ws_row: start.size.1,
            ws_col: start.size.0,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let mut master: libc::c_int = -1;
        // SAFETY: everything the child needs was made before the fork; it only changes directory and
        // replaces itself with the program, or ends.
        let child = unsafe {
            let child = libc::forkpty(
                &raw mut master,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &raw mut window,
            );
            if child == 0 {
                if libc::chdir(dir.as_ptr()) == 0 {
                    libc::execve(program.as_ptr(), argv.as_ptr(), envp.as_ptr());
                }
                libc::_exit(127);
            }
            child
        };
        assert!(child > 0, "a pty and a child");
        // SAFETY: the master's flags, set on the descriptor just made.
        unsafe {
            let flags = libc::fcntl(master, libc::F_GETFL);
            libc::fcntl(master, libc::F_SETFL, flags | libc::O_NONBLOCK);
        }
        let taken = Arc::new(Taken::default());
        let reader = {
            let taken = Arc::clone(&taken);
            std::thread::spawn(move || drain(master, &taken))
        };
        Self {
            master,
            child,
            taken,
            reader: Some(reader),
            graphics: start.graphics,
            size: start.size,
            pixels_answered: [0; 5],
            fenced: 0,
            looked: 0,
            fences_seen: 0,
            raw: Vec::new(),
            status: None,
        }
    }

    /// Takes what the reader of the master has read since the last time, and answers the questions the
    /// program asked in it.
    fn pump(&mut self) {
        self.raw.append(
            &mut self
                .taken
                .bytes
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        );
        // Each round of questions ends with the one for the device attributes, which a terminal answers last:
        // what was asked in the round is answered before it (the program asks again in each round it starts,
        // and what a terminal that draws pictures has besides in the later ones).
        let from = self.looked.saturating_sub(2);
        self.looked = self.raw.len();
        let fences = self.fences_seen + count(&self.raw[from..], b"\x1b[c");
        self.fences_seen = fences;
        while self.fenced < fences {
            self.fenced += 1;
            let more = if self.graphics {
                pixel_answers(&self.raw, self.size, &mut self.pixels_answered)
            } else {
                Vec::new()
            };
            self.type_bytes(&answer(self.graphics, &more));
        }
    }

    /// Types what a person types.
    pub fn type_bytes(&mut self, bytes: &[u8]) {
        // SAFETY: a write of these bytes to the master.
        let written = unsafe { libc::write(self.master, bytes.as_ptr().cast(), bytes.len()) };
        assert!(written >= 0, "the keys were typed");
    }

    /// What the screen says, as text: the escape sequences taken out.
    pub fn text(&self) -> String {
        plain(&self.raw)
    }

    /// Reads what the program writes for `wait`, and does nothing else with it.
    pub fn settle(&mut self, wait: Duration) {
        let end = Instant::now() + wait;
        loop {
            self.pump();
            if Instant::now() >= end {
                return;
            }
            std::thread::sleep(PAUSE);
        }
    }

    /// Whether the program has ended (and how, once).
    fn ended_now(&mut self) -> bool {
        if self.status.is_some() {
            return true;
        }
        let mut status: libc::c_int = 0;
        // SAFETY: a wait for the child this made, without blocking.
        let done = unsafe { libc::waitpid(self.child, &raw mut status, libc::WNOHANG) };
        if done == self.child {
            self.status = Some(status);
            return true;
        }
        false
    }

    /// Waits for the program to end: its exit code, or the signal that ended it as a negative number.
    pub fn ended(&mut self, limit: Duration) -> Option<i32> {
        let end = Instant::now() + limit;
        while !self.ended_now() {
            self.pump();
            if Instant::now() >= end {
                return None;
            }
            std::thread::sleep(PAUSE);
        }
        // What the program wrote last is read by the reader a moment after it ends: its end of the file
        // comes after it. (A process the program started that holds the pty keeps it from coming.)
        let end = Instant::now() + Duration::from_secs(2);
        while !self.taken.closed.load(Ordering::Acquire) && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(1));
        }
        self.pump();
        let status = self.status?;
        Some(if libc::WIFEXITED(status) {
            libc::WEXITSTATUS(status)
        } else {
            -libc::WTERMSIG(status)
        })
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        // The one this started, and only that one.
        if !self.ended_now() {
            // SAFETY: the signal goes to the child this made, which is waited for.
            unsafe {
                libc::kill(self.child, libc::SIGKILL);
                let mut status: libc::c_int = 0;
                libc::waitpid(self.child, &raw mut status, 0);
            }
        }
        // The reader ends before the master is closed: a descriptor closed under it is one a test that
        // starts at the same time may be given, and the reader would read that one.
        self.taken.stop.store(true, Ordering::Release);
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        // SAFETY: the master this opened, closed once.
        unsafe {
            libc::close(self.master);
        }
    }
}

/// Where `needle` first is in `haystack`. What a stream is looked through for starts with a byte that is
/// rare in it (an escape), and the C library finds the first of those fast, which a loop over the bytes is
/// not in the build the tests run in.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    let mut from = 0;
    while from + needle.len() <= haystack.len() {
        let rest = &haystack[from..];
        // SAFETY: a search for one byte in the bytes that are left.
        let at = unsafe {
            libc::memchr(
                rest.as_ptr().cast(),
                libc::c_int::from(needle[0]),
                rest.len(),
            )
        };
        if at.is_null() {
            return None;
        }
        let at = from + (at as usize - rest.as_ptr() as usize);
        if haystack[at..].starts_with(needle) {
            return Some(at);
        }
        from = at + 1;
    }
    None
}

/// How many times `needle` is in `haystack`.
pub fn count(haystack: &[u8], needle: &[u8]) -> usize {
    let (mut found, mut from) = (0, 0);
    while let Some(at) = find(&haystack[from..], needle) {
        found += 1;
        from += at + 1;
    }
    found
}

/// Where the Kitty graphics commands (`ESC _ G` to `ESC \`) are in what the program wrote, found as it
/// grows: each is given once, when it is whole. The tests ask again each time the program has written more,
/// and it writes a picture of the window each time something on it changes: to look through everything it
/// has written for each ask is to take longer after every picture, and in a build for tests longer than the
/// program takes to draw the next.
#[derive(Default)]
struct Commands {
    at: usize,
}

impl Commands {
    /// Calls `each` with where each command that `raw` has whole, and has not given before, begins, and
    /// the bytes of it between the marks. `raw` is the same stream as at the last read, grown.
    fn read(&mut self, raw: &[u8], mut each: impl FnMut(usize, &[u8])) {
        assert!(raw.len() >= self.at, "what is read is the stream, grown");
        while let Some(found) = find(&raw[self.at..], b"\x1b_G") {
            let begins = self.at + found;
            let from = begins + 3;
            let Some(length) = find(&raw[from..], b"\x1b\\") else {
                // Not whole yet: read again from its start.
                self.at = begins;
                return;
            };
            each(begins, &raw[from..from + length]);
            self.at = from + length + 2;
        }
        // The start of a mark may be at the end.
        self.at = self.at.max(raw.len().saturating_sub(2));
    }
}

/// A picture the program sent the terminal (a Kitty graphics command that carries pixels): the keys of
/// its command, and the pixels it says it has.
#[derive(Debug)]
pub struct Sent {
    pub keys: String,
    pub width: u32,
    pub height: u32,
}

/// The picture a command is, if it starts one (a chunked picture is one command and the chunks that follow
/// it, which carry no keys of their own but `m`). The query the program asks first (`a=q`, a pixel), and a
/// command that moves or deletes a picture, are not pictures.
fn sent_in(command: &[u8]) -> Option<Sent> {
    let keys = command.split(|byte| *byte == b';').next().unwrap_or(&[]);
    let keys = String::from_utf8_lossy(keys).into_owned();
    let value = |name: &str| {
        keys.split(',')
            .find_map(|pair| pair.strip_prefix(name)?.strip_prefix('='))
            .and_then(|number| number.parse::<u32>().ok())
    };
    let query = keys.split(',').any(|pair| pair == "a=q");
    let (Some(width), Some(height), false) = (value("s"), value("v"), query) else {
        return None;
    };
    Some(Sent {
        keys,
        width,
        height,
    })
}

/// The pictures in what the program wrote (`sent_in`), in order.
pub fn sent(raw: &[u8]) -> Vec<Sent> {
    let mut found = Vec::new();
    Commands::default().read(raw, |_, command| found.extend(sent_in(command)));
    found
}

/// A question about the pictures the program sent, for a wait: whether one `is` what it asks. It is asked
/// again with the same stream each time that has grown, and looks only at the pictures that came since.
pub fn sends(is: impl Fn(&Sent) -> bool) -> impl FnMut(&[u8]) -> bool {
    let mut commands = Commands::default();
    let mut found = false;
    move |raw| {
        if !found {
            commands.read(raw, |_, command| {
                found = found || sent_in(command).is_some_and(|picture| is(&picture));
            });
        }
        found
    }
}

/// What `text` says, written in base 64 (the standard alphabet; what is not of it is skipped).
fn unbase64(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let (mut bits, mut have) = (0_u32, 0_u32);
    for &c in text {
        let value = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => continue,
        };
        bits = bits << 6 | u32::from(value);
        have += 6;
        if have >= 8 {
            have -= 8;
            out.push(u8::try_from(bits >> have).unwrap_or(0));
            bits &= (1 << have) - 1;
        }
    }
    out
}

/// A picture the program sent, or an edit of one: the row of the window its top is at (a picture is put
/// where the cursor was moved to just before it, an edit says where it goes), how many pixels across it
/// is, and its pixels, row after row of RGBA (`f=32`; a terminal that says it takes no packed pixels is
/// sent them as they are).
pub struct Piece {
    pub top: u32,
    pub width: u32,
    pub pixels: Vec<u8>,
}

/// The row of the window, in pixels, the cursor was moved to by what ends `before`: `CSI H` or
/// `CSI row;1H`, in cells of 20 pixels.
fn cursor_row(before: &[u8]) -> u32 {
    let Some(start) = before.iter().rposition(|byte| *byte == 0x1b) else {
        return 0;
    };
    let csi = &before[start..];
    let Some(inner) = csi
        .strip_prefix(b"\x1b[")
        .and_then(|rest| rest.strip_suffix(b"H"))
    else {
        return 0;
    };
    let row = String::from_utf8_lossy(inner)
        .split(';')
        .next()
        .and_then(|row| row.parse::<u32>().ok())
        .unwrap_or(1);
    row.saturating_sub(1) * 20
}

/// The pictures and edits the program sent, read as it writes them: whole, or in the chunks a large one
/// comes in. The pieces are the ones a command makes: the chunks that follow it are its.
#[derive(Default)]
pub struct Pieces {
    commands: Commands,
    /// A picture whose chunks are still coming: the row it is at, how many pixels across, and the base 64 so
    /// far.
    open: Option<(u32, u32, Vec<u8>)>,
}

impl Pieces {
    /// Calls `each` with the pieces that `raw` has whole, and has not given before, in the order they were
    /// sent. `raw` is the same stream as at the last read, grown.
    pub fn read(&mut self, raw: &[u8], mut each: impl FnMut(Piece)) {
        let open = &mut self.open;
        self.commands.read(raw, |begins, command| {
            let (keys, data) = match command.iter().position(|byte| *byte == b';') {
                Some(split) => (&command[..split], &command[split + 1..]),
                None => (command, &command[..0]),
            };
            let keys = String::from_utf8_lossy(keys);
            let has = |name: &str| keys.split(',').any(|pair| pair == name);
            let value = |name: &str| {
                keys.split(',')
                    .find_map(|pair| pair.strip_prefix(name)?.strip_prefix('='))
                    .and_then(|number| number.parse::<u32>().ok())
            };
            let starts = keys.split(',').any(|pair| pair.starts_with("a="));
            let more = has("m=1");
            if starts {
                *open = None;
                if has("f=32") && !keys.split(',').any(|pair| pair.starts_with("o=")) {
                    let top = if has("a=f") {
                        value("y").unwrap_or(0)
                    } else {
                        cursor_row(&raw[..begins])
                    };
                    let width = value("s").unwrap_or(1).max(1);
                    if more {
                        *open = Some((top, width, data.to_vec()));
                    } else {
                        each(Piece {
                            top,
                            width,
                            pixels: unbase64(data),
                        });
                    }
                }
            } else if let Some((top, width, mut so_far)) = open.take() {
                so_far.extend_from_slice(data);
                if more {
                    *open = Some((top, width, so_far));
                } else {
                    each(Piece {
                        top,
                        width,
                        pixels: unbase64(&so_far),
                    });
                }
            }
        });
    }
}

/// The pictures and edits the program sent, in the order it sent them, whole or in the chunks a large one
/// comes in.
pub fn pieces(raw: &[u8]) -> Vec<Piece> {
    let mut found = Vec::new();
    Pieces::default().read(raw, |piece| found.push(piece));
    found
}

/// Whether the window has the page in it, and not only the line that stands in for the page until its
/// module has handed out the first ("Starting the page…"). A busy machine takes seconds over that and
/// writes nothing meanwhile, and what is typed or pressed on the line reaches no page. The line is a few
/// words in the middle of the window, on the ground (the colour of the first picture's corner); every page
/// draws above or below them.
///
/// It is asked again with the same stream each time that has grown, and reads only the pieces that came since.
pub fn shows_page() -> impl FnMut(&[u8]) -> bool {
    /// What the window the pieces are of is known by: its ground, and the row in the middle of it.
    #[derive(Clone, Copy)]
    enum Window {
        /// The first picture of the window has not come.
        Not,
        /// It came with no pixel in it to take the ground from.
        Bare,
        Is([u8; 4], u32),
    }
    /// Whether the piece has a pixel that is drawn, is not the ground, and is not at the line.
    fn draws_off_the_line(piece: &Piece, ground: [u8; 4], middle: u32) -> bool {
        piece
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .any(|(n, pixel)| {
                let y = piece.top + u32::try_from(n).unwrap_or(0) / piece.width;
                y.abs_diff(middle) > 3 * 20 && pixel[3] != 0 && *pixel != ground
            })
    }
    let mut pieces = Pieces::default();
    let mut window = Window::Not;
    // The pieces that came before the window did: they are of the page too, once its middle is known.
    let mut before: Vec<Piece> = Vec::new();
    let mut shows = false;
    move |raw| {
        if shows {
            return true;
        }
        pieces.read(raw, |piece| {
            if shows {
                return;
            }
            if let Window::Not = window
                && piece.top == 0
                && piece.width >= 200
            {
                window = match piece.pixels.as_chunks::<4>().0.first() {
                    Some(&ground) => {
                        let middle =
                            u32::try_from(piece.pixels.len() / 4).unwrap_or(0) / piece.width / 2;
                        Window::Is(ground, middle)
                    }
                    None => Window::Bare,
                };
                if let Window::Is(ground, middle) = window {
                    shows = before
                        .drain(..)
                        .any(|earlier| draws_off_the_line(&earlier, ground, middle));
                }
            }
            match window {
                Window::Not => before.push(piece),
                Window::Bare => {}
                Window::Is(ground, middle) => {
                    shows = shows || draws_off_the_line(&piece, ground, middle);
                }
            }
        });
        shows
    }
}

/// A question about the pixels the program sent, for a wait: how many of them (picture by picture and edit
/// by edit, `Pieces`) `is` says yes to, given as the three of colour. It is asked again with the same stream
/// each time that has grown, and counts only the pieces that came since.
pub fn pixels_counted(is: impl Fn(u8, u8, u8) -> bool) -> impl FnMut(&[u8]) -> usize {
    let mut pieces = Pieces::default();
    let mut counted = 0;
    move |raw| {
        pieces.read(raw, |piece| {
            counted += piece
                .pixels
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|pixel| is(pixel[0], pixel[1], pixel[2]))
                .count();
        });
        counted
    }
}

/// `raw` as the text a screen would hold, the escape sequences (CSI, OSC and the pictures' APC) taken out.
pub fn plain(raw: &[u8]) -> String {
    let mut out = Vec::with_capacity(raw.len());
    let mut at = 0;
    while at < raw.len() {
        if raw[at] != 0x1b {
            out.push(raw[at]);
            at += 1;
            continue;
        }
        match raw.get(at + 1) {
            Some(b'[') => {
                at += 2;
                while at < raw.len() && !(0x40..=0x7e).contains(&raw[at]) {
                    at += 1;
                }
                at += 1;
            }
            Some(b']' | b'_' | b'P') => {
                at += 2;
                while at < raw.len() {
                    if raw[at] == 0x07 {
                        at += 1;
                        break;
                    }
                    if raw[at] == 0x1b && raw.get(at + 1) == Some(&b'\\') {
                        at += 2;
                        break;
                    }
                    at += 1;
                }
            }
            _ => at += 2,
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}
