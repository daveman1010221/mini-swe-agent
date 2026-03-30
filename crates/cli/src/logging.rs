//! Initialise `tracing-subscriber` from CLI flags.
//!
//! Verbosity ladder:
//!   (none) → info
//!   -v     → debug
//!   -vv+   → trace
//!
//! `RUST_LOG` still overrides everything when set explicitly.

use tracing_subscriber::{fmt, EnvFilter};

pub fn init(verbose: u8, json: bool) {
    let default_level = match verbose {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(default_level));

    if json {
        fmt().json().with_env_filter(filter).init();
    } else {
        fmt().with_env_filter(filter).init();
    }
}
