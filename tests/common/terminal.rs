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
fn answer(graphics: bool) -> Vec<u8> {
    let mut out = String::from("\x1b]10;rgb:cccc/cccc/cccc\x1b\\\x1b]11;rgb:1e1e/1e1e/2e2e\x1b\\");
    for index in 0..16 {
        out.push_str(&format!("\x1b]4;{index};rgb:8080/8080/8080\x1b\\"));
    }
    out.push_str("\x1b[?0u");
    if graphics {
        out.push_str("\x1b_Gi=31;OK\x1b\\");
    }
    out.push_str("\x1b[?62;4c");
    out.into_bytes()
}

/// The program, running on a pty.
pub struct Terminal {
    master: RawFd,
    child: libc::pid_t,
    graphics: bool,
    asked: bool,
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
            asked: false,
            raw: Vec::new(),
            status: None,
        }
    }

    /// Reads what the program has written, and answers the questions it asked.
    fn pump(&mut self) {
        let mut buffer = [0_u8; 65536];
        loop {
            // SAFETY: a read into a buffer of this size, from the master.
            let n = unsafe { libc::read(self.master, buffer.as_mut_ptr().cast(), buffer.len()) };
            if n <= 0 {
                break;
            }
            self.raw.extend_from_slice(&buffer[..n as usize]);
        }
        if !self.asked && contains(&self.raw, b"\x1b[c") {
            self.asked = true;
            self.type_bytes(&answer(self.graphics));
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
