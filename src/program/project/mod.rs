//! The project on disk: the tree of files, git, the search through them. It knows nothing of the
//! page: plain values in and out, and failures as the words a person reads.

pub mod diff;
pub mod git;
pub mod paths;
pub mod search;
pub mod tree;
pub mod watch;

use std::io;
use std::time::{SystemTime, UNIX_EPOCH};

/// What the system said went wrong, as a person reads it: without the number of the error.
pub fn said(why: &io::Error) -> String {
    let words = why.to_string();
    match words.find(" (os error ") {
        Some(at) => words.get(..at).unwrap_or(&words).to_owned(),
        None => words,
    }
}

/// A time as milliseconds since 1970, as a page counts time.
pub fn ms_of(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH).map_or(0, |since| {
        u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
    })
}

/// Now, as milliseconds since 1970.
pub fn now_ms() -> u64 {
    ms_of(SystemTime::now())
}
