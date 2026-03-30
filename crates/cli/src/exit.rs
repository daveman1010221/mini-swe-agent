//! Process exit codes.
//!
//! These mirror common conventions so shell scripts can branch on them.

use mswea_core::error::ExitStatus;

pub fn exit_code(status: &ExitStatus) -> i32 {
    match status {
        ExitStatus::Submitted        => 0,
        ExitStatus::UserInterruption => 130, // matches SIGINT convention
        ExitStatus::LimitsExceeded   => 1,
        ExitStatus::FormatError      => 2,
        ExitStatus::ModelError       => 3,
        ExitStatus::EnvironmentError => 4,
        ExitStatus::Uncaught         => 5,
    }
}
