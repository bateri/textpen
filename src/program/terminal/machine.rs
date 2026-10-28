//! Who runs textpen, and where: the user's name and home, the machine's name, and whether it is
//! reached over ssh.
//!
//! Plain values in and out: this file knows nothing of the page.

fn variable(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

/// The home directory, "" where it is not known.
pub fn home() -> String {
    variable("HOME").unwrap_or_default()
}

/// Whether the program runs in a session that ssh started.
pub fn remote() -> bool {
    variable("SSH_CONNECTION").is_some() || variable("SSH_TTY").is_some()
}

/// The machine's name, as it calls itself ("" where it cannot be found).
pub fn hostname() -> String {
    let mut name = [0u8; 256];
    // SAFETY: fills a buffer of ours, with room for the end of the text.
    let got = unsafe { libc::gethostname(name.as_mut_ptr().cast(), name.len() - 1) };
    if got != 0 {
        return String::new();
    }
    let end = name
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(name.len());
    String::from_utf8_lossy(name.get(..end).unwrap_or_default()).into_owned()
}

/// The name of the person who runs the program, from the system's records of users ("" where the
/// user has no entry).
pub fn user() -> String {
    // The record is filled into a buffer of ours, which has to be big enough for it: a user with
    // many names in the record, or a long home, outgrows a small one (ERANGE).
    let mut size = 1024usize;
    loop {
        let mut buffer = vec![0u8; size];
        // SAFETY: the record is filled into buffers of ours, which outlive its use below; the name
        // it points to is in `buffer`.
        let (status, name) = unsafe {
            let mut record: libc::passwd = std::mem::zeroed();
            let mut found: *mut libc::passwd = std::ptr::null_mut();
            let status = libc::getpwuid_r(
                libc::geteuid(),
                &raw mut record,
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                &raw mut found,
            );
            let name = if status == 0 && !found.is_null() && !record.pw_name.is_null() {
                Some(
                    std::ffi::CStr::from_ptr(record.pw_name)
                        .to_string_lossy()
                        .into_owned(),
                )
            } else {
                None
            };
            (status, name)
        };
        if status == libc::ERANGE && size < 1 << 20 {
            size *= 4;
            continue;
        }
        return name.unwrap_or_default();
    }
}
