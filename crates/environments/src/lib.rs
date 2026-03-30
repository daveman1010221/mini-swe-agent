//! `environments` — embedded nushell engine and file/search operations.
//!
//! Public surface:
//!   - `ShellWorker`      — async handle to the stateful nu session thread
//!   - `NushellSession`   — the session itself (sync, for direct use in tests)
//!   - `file_ops`         — read, write, edit, search functions

pub mod file_ops;
pub mod session;
pub mod shell_worker;
pub mod value_to_observation;

pub use file_ops::{edit_file, read_file, search, write_file};
pub use session::NushellSession;
pub use shell_worker::ShellWorker;
