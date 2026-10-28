//! The terminal textpen runs in: its command line, who runs it and where, and the pane lent to a shell
//! or to the person's own editor. It knows nothing of the page: plain values in and out.

pub mod command_line;
pub mod handoff;
pub mod machine;
