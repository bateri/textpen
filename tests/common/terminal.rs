//! A terminal that hosts nothing, for the program to be started in: a pty with a window of a size, which
//! answers what a terminal answers (the colours, the keyboard, the device attributes, and the Kitty graphics
//! protocol where it has it), types what a person types and reads what the program draws. What is read is
//! the text of the screen with what moves the cursor and sets the colours taken out of it.

use std::ffi::CString;
use std::os::fd::RawFd;
use std::path::Path;
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

/// The program, running on a pty.
pub struct Terminal {
    master: RawFd,
    child: libc::pid_t,
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
        Self {
            master,
            child,
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

    /// Reads what the program has written, and answers the questions it asked. A pty holds a few
    /// kilobytes, so a program that writes a picture waits on this reader for every few: it is read as
    /// long as it writes, and left when it has been quiet a few milliseconds.
    fn pump(&mut self) {
        let mut buffer = [0_u8; 65536];
        let mut quiet = 0;
        while quiet < 3 {
            // SAFETY: a read into a buffer of this size, from the master.
            let n = unsafe { libc::read(self.master, buffer.as_mut_ptr().cast(), buffer.len()) };
            if n > 0 {
                self.raw.extend_from_slice(&buffer[..n as usize]);
                quiet = 0;
            } else {
                quiet += 1;
                std::thread::sleep(Duration::from_millis(1));
            }
        }
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

    /// Waits for `what` to be on the screen (in what was written since `from`, a length of `raw`).
    pub fn until_since(&mut self, from: usize, what: &str, limit: Duration) -> bool {
        self.until_with(from, limit, |text| text.contains(what))
    }

    /// Waits for what was written since `from` (a length of `raw`), as text, to be what `wanted` says.
    pub fn until_with(
        &mut self,
        from: usize,
        limit: Duration,
        wanted: impl Fn(&str) -> bool,
    ) -> bool {
        let end = Instant::now() + limit;
        loop {
            self.pump();
            if wanted(&plain(&self.raw[from.min(self.raw.len())..])) {
                return true;
            }
            if self.ended_now() || Instant::now() >= end {
                return false;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    /// Reads what the program writes for `wait`, and does nothing else with it.
    pub fn settle(&mut self, wait: Duration) {
        let end = Instant::now() + wait;
        loop {
            self.pump();
            if Instant::now() >= end {
                return;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    /// Waits for `what` to be written.
    pub fn until(&mut self, what: &str, limit: Duration) -> bool {
        self.until_since(0, what, limit)
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
            std::thread::sleep(Duration::from_millis(25));
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
        // SAFETY: the master this opened, closed once.
        unsafe {
            libc::close(self.master);
        }
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// How many times `needle` is in `haystack`.
pub fn count(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .filter(|window| *window == needle)
        .count()
}

/// A picture the program sent the terminal (a Kitty graphics command that carries pixels): the keys of
/// its command, and the pixels it says it has.
#[derive(Debug)]
pub struct Sent {
    pub keys: String,
    pub width: u32,
    pub height: u32,
}

/// The pictures in what the program wrote: each command that starts one (a chunked picture is one command
/// and the chunks that follow it, which carry no keys of their own but `m`). The query the program asks
/// first (`a=q`, a pixel), and a command that moves or deletes a picture, are not pictures.
pub fn sent(raw: &[u8]) -> Vec<Sent> {
    let mut found = Vec::new();
    let mut at = 0;
    while at + 3 <= raw.len() {
        if &raw[at..at + 3] != b"\x1b_G" {
            at += 1;
            continue;
        }
        let from = at + 3;
        let mut end = from;
        while end < raw.len() && !(raw[end] == 0x1b && raw.get(end + 1) == Some(&b'\\')) {
            end += 1;
        }
        let command = String::from_utf8_lossy(&raw[from..end]).into_owned();
        let keys = command.split(';').next().unwrap_or("").to_owned();
        let value = |name: &str| {
            keys.split(',')
                .find_map(|pair| pair.strip_prefix(name)?.strip_prefix('='))
                .and_then(|number| number.parse::<u32>().ok())
        };
        let query = keys.split(',').any(|pair| pair == "a=q");
        if let (Some(width), Some(height), false) = (value("s"), value("v"), query) {
            found.push(Sent {
                keys: keys.clone(),
                width,
                height,
            });
        }
        at = end + 2;
    }
    found
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

/// The pixels the program sent, picture by picture and edit by edit: each a row after row of RGBA (`f=32`;
/// a terminal that says it takes no packed pixels is sent them as they are), whole or in the chunks a large
/// one comes in.
pub fn pixels(raw: &[u8]) -> Vec<Vec<u8>> {
    let mut found = Vec::new();
    let mut open: Option<Vec<u8>> = None;
    let mut at = 0;
    while at + 3 <= raw.len() {
        if &raw[at..at + 3] != b"\x1b_G" {
            at += 1;
            continue;
        }
        let from = at + 3;
        let mut end = from;
        while end < raw.len() && !(raw[end] == 0x1b && raw.get(end + 1) == Some(&b'\\')) {
            end += 1;
        }
        let command = &raw[from..end];
        at = end + 2;
        let (keys, data) = match command.iter().position(|byte| *byte == b';') {
            Some(split) => (&command[..split], &command[split + 1..]),
            None => (command, &command[..0]),
        };
        let keys = String::from_utf8_lossy(keys);
        let has = |name: &str| keys.split(',').any(|pair| pair == name);
        let starts = keys.split(',').any(|pair| pair.starts_with("a="));
        let more = has("m=1");
        if starts {
            open = None;
            if has("f=32") && !keys.split(',').any(|pair| pair.starts_with("o=")) {
                if more {
                    open = Some(data.to_vec());
                } else {
                    found.push(unbase64(data));
                }
            }
        } else if let Some(mut so_far) = open.take() {
            so_far.extend_from_slice(data);
            if more {
                open = Some(so_far);
            } else {
                found.push(unbase64(&so_far));
            }
        }
    }
    found
}

/// How many of the pixels the program sent (`pixels`) `is` says yes to, given as the three of colour.
pub fn pixels_that(raw: &[u8], is: impl Fn(u8, u8, u8) -> bool) -> usize {
    pixels(raw)
        .iter()
        .flat_map(|picture| picture.as_chunks::<4>().0.iter())
        .filter(|pixel| is(pixel[0], pixel[1], pixel[2]))
        .count()
}

/// `text` with its runs of white space made one space, as much of it as a failure's message can say.
pub fn squeezed(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    let line = words.join(" ");
    line.chars().take(700).collect()
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
