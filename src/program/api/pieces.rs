//! What one message to the page may carry. The terminal passes a page's messages unread and drops
//! one over 1 MiB, in either direction, so nothing the program answers is allowed to grow with the
//! project: a long text, a directory of thousands of entries, the files of a big repository and a
//! long search all come in pieces that fit.
//!
//! pencere counts a message as the UTF-8 of its JSON before the wire's escapes: a text takes as
//! many bytes as its UTF-8 has.
//!
//! This is api/'s: the files of api/ know the page, and project/, state/ and terminal/ do not.

/// The room a payload has in a message of 1 MiB: what is around it (the envelope, the other members
/// of the result) takes a few hundred bytes, and this leaves them 124 KiB.
pub const WIRE_BUDGET: usize = 900 * 1024;

/// How many of the first `items` go in a message together, each taking `size(item)` bytes. At
/// least one, when there is one.
pub fn fitting<T>(items: &[T], size: impl Fn(&T) -> usize) -> usize {
    let mut used = 0;
    for (index, item) in items.iter().enumerate() {
        used += size(item);
        if used > WIRE_BUDGET {
            return index.max(1);
        }
    }
    items.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_many_fit_as_the_budget_has_room_for_and_one_at_least() {
        assert_eq!(fitting(&[1, 2, 3], |_| WIRE_BUDGET / 2), 2);
        assert_eq!(fitting(&[1], |_| WIRE_BUDGET * 2), 1);
        assert_eq!(fitting::<u8>(&[], |_| 1), 0);
        assert_eq!(fitting(&[1, 2], |_| 1), 2);
    }
}
