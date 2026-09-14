//! The discovery file: `<config dir>/request-socket.json`. Follow-stream
//! needs nothing like this because only this app's own CLI ever attaches to
//! it, at a name it can derive itself. The request socket's clients are
//! separate processes (core, the Obsidian plugin) with no way to derive
//! that name, so each run of the app writes down where it is listening.

use std::io::Write as _;
use std::{fs, io, path::PathBuf};

use serde::Serialize;

use super::protocol::REQUEST_PROTOCOL_VERSION;

const FILE_NAME: &str = "request-socket.json";

/// Mirrors core's `shorthandConfigDirectory()` exactly (see
/// REQUEST_SOCKET.md at the repo root): the two must agree on this path
/// without either reading the other's source.
#[cfg(target_os = "windows")]
pub fn config_directory() -> io::Result<PathBuf> {
    let appdata = std::env::var_os("APPDATA")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "%APPDATA% is not set"))?;
    Ok(PathBuf::from(appdata).join("Shorthand"))
}

#[cfg(target_os = "macos")]
pub fn config_directory() -> io::Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "$HOME is not set"))?;
    Ok(PathBuf::from(home)
        .join("Library")
        .join("Application Support")
        .join("Shorthand"))
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn config_directory() -> io::Result<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(xdg).join("shorthand"));
    }
    let home = std::env::var_os("HOME")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "$HOME is not set"))?;
    Ok(PathBuf::from(home).join(".config").join("shorthand"))
}

#[derive(Serialize)]
struct Discovery<'a> {
    protocol: u32,
    path: &'a str,
}

/// Writes the discovery file atomically (temp file + rename) so a client
/// polling for it never observes a half-written document, mode 0600 on
/// Unix so only this user's own processes can read the path out of it.
///
/// The temp file is per-process (`{FILE_NAME}.<pid>.tmp`) and opened with
/// `create_new`, rather than a fixed name opened with plain `write`: two
/// instances racing to (re)write the discovery file on the same fixed temp
/// name could otherwise have one process's rename pick up bytes the other
/// one wrote. `create_new` also means the 0600 mode applies at the moment
/// the file is created, not as a second step after a plain-permissions
/// window during which another local user could have opened it.
pub fn write_discovery(path_for_clients: &str) -> io::Result<()> {
    let dir = config_directory()?;
    fs::create_dir_all(&dir)?;

    let contents = serde_json::to_string(&Discovery {
        protocol: REQUEST_PROTOCOL_VERSION,
        path: path_for_clients,
    })
    .expect("discovery document always serializes");

    let temp_path = dir.join(format!("{FILE_NAME}.{}.tmp", std::process::id()));
    {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp_path)?;
        file.write_all(contents.as_bytes())?;
    }
    fs::rename(&temp_path, dir.join(FILE_NAME))
}

/// Best-effort: called on clean shutdown so a stale file does not point a
/// future client at a socket nobody is listening on. Nothing reads this
/// file once the app is gone, so a failure here is not worth surfacing —
/// the next successful `write_discovery` overwrites it anyway.
pub fn remove_discovery() {
    if let Ok(dir) = config_directory() {
        let _ = fs::remove_file(dir.join(FILE_NAME));
    }
}
