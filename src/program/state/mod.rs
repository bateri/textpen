//! What the program holds and remembers: the open files and their drafts, the settings, the page's
//! state in each project. It knows nothing of the page: plain values in and out.

pub mod buffers;
pub mod defaults;
pub mod later;
pub mod session;
pub mod store;
