//! Compile time build information provided by built

use chrono::Local;
use serde::Serialize;

include!(concat!(env!("OUT_DIR"), "/built.rs"));

/// User friendly label for marking a build as debug or not
pub const DEBUG_LABEL: &str = if DEBUG { " (debug)" } else { "" };
/// User friendly label for indicating repo state
pub const DIRTY_LABEL: &str = match GIT_DIRTY {
    Some(true) => " (+unstaged changes)",
    _ => "",
};

pub fn build_info() -> String {
    format!(
        concat!("- {}{}\n", "Built: {}\n", "Commit: {}{}"),
        PKG_VERSION,
        DEBUG_LABEL,
        BUILT_TIME_UTC,
        GIT_COMMIT_HASH.unwrap_or("Unknown"),
        DIRTY_LABEL
    )
}

#[derive(Debug, Clone, Serialize)]
pub struct ServerStatus {
    version: String,
    start_time: String,
    build: String,
}

impl ServerStatus {
    pub fn new() -> Self {
        Self {
            version: PKG_VERSION.into(),
            start_time: Local::now().to_rfc3339(),
            build: GIT_COMMIT_HASH.unwrap_or("Unknown").into(),
        }
    }
}
