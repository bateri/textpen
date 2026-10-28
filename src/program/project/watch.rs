//! Which of the directories that were listed have changed since. A directory is noted with its
//! modification time when it is listed; asking tells those whose time is another now (or that went),
//! and tells of one that went only once.
//!
//! Plain values in and out: this file knows nothing of the page.

use std::collections::BTreeMap;
use std::time::SystemTime;

/// The directories listed, with their modification time then.
#[derive(Debug, Default)]
pub struct Listed {
    times: BTreeMap<String, SystemTime>,
}

impl Listed {
    /// A directory was listed, and was last changed at `modified`.
    pub fn note(&mut self, dir: String, modified: Option<SystemTime>) {
        if let Some(modified) = modified {
            self.times.insert(dir, modified);
        }
    }

    /// The listed directories whose entries changed since they were listed (or that went), in
    /// order. `modified_of(dir)` is the directory's modification time now, none when it is gone.
    /// One that changed is told again until it is listed again.
    pub fn changed(&mut self, modified_of: impl Fn(&str) -> Option<SystemTime>) -> Vec<String> {
        let mut changed = Vec::new();
        let mut gone = Vec::new();
        for (dir, then) in &self.times {
            match modified_of(dir) {
                None => {
                    gone.push(dir.clone());
                    changed.push(dir.clone());
                }
                Some(now) if now != *then => changed.push(dir.clone()),
                Some(_) => {}
            }
        }
        for dir in gone {
            self.times.remove(&dir);
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn a_directory_that_changed_is_told_until_it_is_listed_again_and_one_that_went_once() {
        let then = SystemTime::UNIX_EPOCH + Duration::from_secs(10);
        let later = then + Duration::from_secs(1);
        let mut listed = Listed::default();
        listed.note("a".to_owned(), Some(then));
        listed.note("b".to_owned(), Some(then));
        listed.note("c".to_owned(), Some(then));
        let now = |dir: &str| match dir {
            "a" => Some(then),
            "b" => Some(later),
            _ => None,
        };
        assert_eq!(listed.changed(now), ["b", "c"]);
        assert_eq!(listed.changed(now), ["b"]);
        listed.note("b".to_owned(), Some(later));
        assert!(listed.changed(now).is_empty());
    }
}
