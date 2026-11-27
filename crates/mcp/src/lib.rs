//! openDJ's command line and local MCP server, for library work.
//!
//! ```text
//! opendj search 'bpm:120-124 genre:house' --json
//! opendj list add 'Friday' 1234 1240
//! opendj mcp                       an AI assistant starts this and talks over stdio
//! ```
//!
//! Both run headless from the app binary: no window, audio or controllers.
//! They reach the library only, never a deck. The commands and the MCP
//! tools are one thing ([`tools`]): each command is a tool call, and
//! `--json` prints the tool's result as it is.
//!
//! The library is SQLite in WAL mode, so a command can write while openDJ is
//! open. After a write, the command touches a change marker file that the
//! app watches, so it reloads rather than saving over the change.

pub mod cli;
mod plan;
mod prep;
pub mod server;
pub mod tools;

use std::path::{Path, PathBuf};

pub use opendj_library::Library;

/// Where the library and the change marker are, and what the DJ calls
/// their own fields.
#[derive(Clone, Debug)]
pub struct Config {
    /// The library database.
    pub library: PathBuf,
    /// Touched after every write, for a running app to notice.
    pub marker: Option<PathBuf>,
    /// What the DJ calls the grouping field (Settings), for searches.
    pub grouping: String,
}

/// The file a running app watches, in its config folder.
pub fn change_marker(config_dir: &Path) -> PathBuf {
    config_dir.join(".library-changed")
}

/// Whether the app's first argument is one of these commands, so the app
/// runs it instead of opening its window.
pub fn is_command(arg: &str) -> bool {
    cli::COMMANDS.contains(&arg) || matches!(arg, "help" | "--help" | "-h")
}

/// Opens the library for commands. It is never created here: a command
/// run before openDJ has made a library says so instead of leaving an
/// empty one behind.
pub fn open(config: &Config) -> Result<tools::Ctx, String> {
    if !config.library.exists() {
        return Err(format!(
            "No openDJ library at {}. Open openDJ once to make one.",
            config.library.display()
        ));
    }
    let lib =
        Library::open(&config.library).map_err(|e| format!("Couldn't open the library: {e}"))?;
    Ok(tools::Ctx::new(
        lib,
        config.marker.clone(),
        &config.grouping,
    ))
}
