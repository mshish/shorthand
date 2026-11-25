//! The connected doc's name, so the settings page can say where meetings go
//! ("Meetings go into Team standups") instead of only offering a link.
//!
//! Kept in the app's own data folder, not in `google-credentials.json`,
//! whose bytes are a contract with core. The name is only shown while its
//! `document_id` matches the one in the credentials, so a doc changed by
//! another writer never shows a stale name.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const FILE_NAME: &str = "google-docs-document.json";
const GOOGLE_API_BASE: &str = "https://www.googleapis.com";

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct DocumentInfo {
    pub document_id: String,
    pub name: String,
}

pub fn info_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join(FILE_NAME)
}

/// The stored name, only if it belongs to `document_id`.
pub fn read_name(path: &Path, document_id: &str) -> Option<String> {
    let info: DocumentInfo = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    (info.document_id == document_id).then_some(info.name)
}

pub fn write(path: &Path, info: &DocumentInfo) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(info).map_err(std::io::Error::other)?;
    std::fs::write(path, bytes)
}

pub fn remove(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

#[derive(Deserialize)]
struct NameResponse {
    name: Option<String>,
}

/// Best effort: the name is a label, so any failure is just "no name".
pub async fn fetch_name(
    http: &reqwest::Client,
    access_token: &str,
    document_id: &str,
) -> Option<String> {
    fetch_name_at(http, GOOGLE_API_BASE, access_token, document_id).await
}

async fn fetch_name_at(
    http: &reqwest::Client,
    drive_base: &str,
    access_token: &str,
    document_id: &str,
) -> Option<String> {
    let response = http
        .get(format!(
            "{drive_base}/drive/v3/files/{document_id}?fields=name"
        ))
        .bearer_auth(access_token)
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let body: NameResponse = response.json().await.ok()?;
    body.name.filter(|n| !n.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::super::test_http::{client, serve, Route};
    use super::*;

    #[test]
    fn the_name_shows_only_for_its_own_document() {
        let dir = tempfile::tempdir().unwrap();
        let path = info_path(dir.path());
        assert_eq!(read_name(&path, "d1"), None);
        write(
            &path,
            &DocumentInfo {
                document_id: "d1".into(),
                name: "Team standups".into(),
            },
        )
        .unwrap();
        assert_eq!(read_name(&path, "d1").as_deref(), Some("Team standups"));
        assert_eq!(read_name(&path, "d2"), None);
        remove(&path).unwrap();
        remove(&path).unwrap();
        assert_eq!(read_name(&path, "d1"), None);
    }

    #[tokio::test]
    async fn fetches_the_name_from_drive() {
        let (drive, seen) = serve(vec![Route::json(
            "GET /drive/v3/files/d1",
            200,
            r#"{"name":"Team standups"}"#,
        )])
        .await;
        assert_eq!(
            fetch_name_at(&client(), &drive, "token", "d1")
                .await
                .as_deref(),
            Some("Team standups")
        );
        assert!(seen.await.unwrap()[0].target.contains("fields=name"));
    }

    #[tokio::test]
    async fn a_failed_lookup_is_just_no_name() {
        let (drive, _) = serve(vec![Route::json("GET /drive/v3/files/d1", 404, "{}")]).await;
        assert_eq!(fetch_name_at(&client(), &drive, "token", "d1").await, None);
    }
}
