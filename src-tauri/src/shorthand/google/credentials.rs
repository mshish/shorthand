//! `google-credentials.json`: the file shorthand-core reads to write into a
//! Google Doc. Ported from shorthand-config's `credentials.rs`.
//!
//! Core only reads this file; exactly one app may write it. With this module
//! the Shorthand app becomes that writer, so shorthand-config must not be
//! used alongside it (umbrella design, "one writer per credentials file").
//!
//! The field order and formatting are a byte-exact contract with core's
//! conformance fixture; do not reorder.

use std::io::Write as _;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credentials {
    pub client_id: String,
    pub client_secret: String,
    pub refresh_token: String,
    pub document_id: Option<String>,
    pub folder_id: Option<String>,
}

const FILE_NAME: &str = "google-credentials.json";

/// `<core's config directory>/google-credentials.json`, the path core's
/// `credentialsPath()` computes.
pub fn credentials_path() -> std::io::Result<PathBuf> {
    Ok(crate::shorthand::request_socket::config_directory()?.join(FILE_NAME))
}

/// Reads the file back to show its state. One writer per file governs
/// writes; reading it to display "connected" breaks nothing.
pub fn read(path: &Path) -> Option<Credentials> {
    let raw = std::fs::read_to_string(path).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&raw).ok()?;
    if parsed.get("type")?.as_str()? != "authorized_user" {
        return None;
    }
    let text = |key: &str| parsed.get(key).and_then(|v| v.as_str()).map(String::from);
    Some(Credentials {
        client_id: text("client_id")?,
        client_secret: text("client_secret")?,
        refresh_token: text("refresh_token")?,
        document_id: text("document_id"),
        folder_id: text("folder_id"),
    })
}

/// Field order follows the declaration below. A `serde_json::Map` would sort
/// the keys alphabetically (the crate is built without `preserve_order`),
/// which breaks the byte-exact contract with core.
#[derive(serde::Serialize)]
struct OnDisk<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    client_id: &'a str,
    client_secret: &'a str,
    refresh_token: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    document_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    folder_id: Option<&'a str>,
}

fn render(credentials: &Credentials) -> String {
    let on_disk = OnDisk {
        kind: "authorized_user",
        client_id: &credentials.client_id,
        client_secret: &credentials.client_secret,
        refresh_token: &credentials.refresh_token,
        document_id: credentials.document_id.as_deref(),
        folder_id: credentials.folder_id.as_deref(),
    };
    let mut body = serde_json::to_string_pretty(&on_disk).expect("credentials always serialize");
    body.push('\n');
    body
}

/// Writes the whole file atomically: a temp file in the same directory,
/// owner-only on POSIX, renamed over the old one. Never merges with what was
/// there.
pub fn write(path: &Path, credentials: &Credentials) -> std::io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| std::io::Error::other("credentials path has no parent directory"))?;
    std::fs::create_dir_all(dir)?;
    let temp = dir.join(format!(".{FILE_NAME}.{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        file.write_all(render(credentials).as_bytes())?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

/// Removes the file. The refresh token stays valid at Google until the user
/// removes the app's access in their Google account.
pub fn remove(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn minimal() -> Credentials {
        Credentials {
            client_id: "1234567890-conformance.apps.googleusercontent.com".into(),
            client_secret: "conformance-client-secret".into(),
            refresh_token: "conformance-refresh-token".into(),
            document_id: Some("conformance-document-id".into()),
            folder_id: None,
        }
    }

    #[test]
    fn matches_cores_golden_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        write(&path, &minimal()).unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "{\n  \"type\": \"authorized_user\",\n  \"client_id\": \"1234567890-conformance.apps.googleusercontent.com\",\n  \"client_secret\": \"conformance-client-secret\",\n  \"refresh_token\": \"conformance-refresh-token\",\n  \"document_id\": \"conformance-document-id\"\n}\n"
        );
    }

    #[test]
    fn matches_cores_golden_bytes_with_a_folder() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        let mut creds = minimal();
        creds.folder_id = Some("conformance-folder-id".into());
        write(&path, &creds).unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "{\n  \"type\": \"authorized_user\",\n  \"client_id\": \"1234567890-conformance.apps.googleusercontent.com\",\n  \"client_secret\": \"conformance-client-secret\",\n  \"refresh_token\": \"conformance-refresh-token\",\n  \"document_id\": \"conformance-document-id\",\n  \"folder_id\": \"conformance-folder-id\"\n}\n"
        );
    }

    #[test]
    fn a_second_write_replaces_the_first_and_leaves_no_temp_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        let mut with_folder = minimal();
        with_folder.folder_id = Some("f".into());
        write(&path, &with_folder).unwrap();
        write(&path, &minimal()).unwrap();
        assert!(!fs::read_to_string(&path).unwrap().contains("folder_id"));
        let names: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from(FILE_NAME)]);
    }

    #[test]
    fn round_trips_through_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        let mut creds = minimal();
        creds.folder_id = Some("f".into());
        write(&path, &creds).unwrap();
        assert_eq!(read(&path), Some(creds));
    }

    #[test]
    fn read_rejects_a_file_that_is_not_an_authorized_user_credential() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        fs::write(&path, r#"{"type":"service_account","client_id":"x"}"#).unwrap();
        assert_eq!(read(&path), None);
        assert_eq!(read(&dir.path().join("missing.json")), None);
    }

    #[test]
    fn remove_is_quiet_when_nothing_is_there() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        remove(&path).unwrap();
        write(&path, &minimal()).unwrap();
        remove(&path).unwrap();
        assert!(!path.exists());
    }

    #[test]
    #[cfg(unix)]
    fn file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        write(&path, &minimal()).unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
