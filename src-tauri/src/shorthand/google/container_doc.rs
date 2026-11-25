//! The "create a new doc" flow: a Drive folder named "Shorthand Meeting
//! Notes" holding one Google Doc of the same name, which core then fills with
//! a tab per meeting. Ported from shorthand-config's `container_doc.rs`.
//!
//! The same folder serves the "a doc per meeting" setting: each meeting gets
//! a new doc created straight into it, and the shared doc isn't needed.
//!
//! Every call is made with the `drive.file` scope, so the app only ever sees
//! the folder and document it created (or the one the user picked).

const DEFAULT_NAME: &str = "Shorthand Meeting Notes";
const GOOGLE_API_BASE: &str = "https://www.googleapis.com";
const GOOGLE_DOCS_API_BASE: &str = "https://docs.googleapis.com";

#[derive(Debug, Default, Clone)]
pub struct ExistingTarget {
    pub folder_id: Option<String>,
    pub document_id: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ContainerDoc {
    pub folder_id: String,
    pub document_id: String,
    pub folder_created: bool,
    pub document_created: bool,
}

#[derive(serde::Deserialize)]
struct IdResponse {
    id: Option<String>,
}

#[derive(serde::Deserialize)]
struct DocsCreateResponse {
    #[serde(rename = "documentId")]
    document_id: Option<String>,
}

#[derive(serde::Deserialize)]
struct ParentsResponse {
    parents: Option<Vec<String>>,
}

/// Checks the status before the body is trusted: every field above is an
/// `Option`, so Google's `{"error": …}` body would otherwise parse as "no id".
async fn ensure_success(
    response: reqwest::Response,
    context: &str,
) -> Result<reqwest::Response, String> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    Err(format!("{context} failed with status {status}: {body}"))
}

pub async fn ensure_container_doc(
    http: &reqwest::Client,
    access_token: &str,
    existing: ExistingTarget,
) -> Result<ContainerDoc, String> {
    ensure_container_doc_at(
        http,
        GOOGLE_API_BASE,
        GOOGLE_DOCS_API_BASE,
        access_token,
        existing,
    )
    .await
}

async fn ensure_container_doc_at(
    http: &reqwest::Client,
    drive_base: &str,
    docs_base: &str,
    access_token: &str,
    existing: ExistingTarget,
) -> Result<ContainerDoc, String> {
    if let (Some(folder_id), Some(document_id)) = (&existing.folder_id, &existing.document_id) {
        return Ok(ContainerDoc {
            folder_id: folder_id.clone(),
            document_id: document_id.clone(),
            folder_created: false,
            document_created: false,
        });
    }

    let (folder_id, folder_created) =
        ensure_folder_at(http, drive_base, access_token, existing.folder_id.clone()).await?;

    match ensure_document_and_reparent(http, drive_base, docs_base, access_token, &existing, &folder_id)
        .await
    {
        Ok((document_id, document_created)) => Ok(ContainerDoc {
            folder_id,
            document_id,
            folder_created,
            document_created,
        }),
        Err(message) if folder_created => Err(format!(
            "{message} (a \"{DEFAULT_NAME}\" folder was already created in Google Drive, id {folder_id}; \
             reuse or delete it before trying again)"
        )),
        Err(message) => Err(message),
    }
}

/// The "Shorthand Meeting Notes" folder: `existing` when there is one,
/// otherwise a new folder in the top of the user's Drive. The bool is
/// whether it was created now.
pub async fn ensure_folder(
    http: &reqwest::Client,
    access_token: &str,
    existing: Option<String>,
) -> Result<(String, bool), String> {
    ensure_folder_at(http, GOOGLE_API_BASE, access_token, existing).await
}

async fn ensure_folder_at(
    http: &reqwest::Client,
    drive_base: &str,
    access_token: &str,
    existing: Option<String>,
) -> Result<(String, bool), String> {
    if let Some(id) = existing.filter(|id| !id.trim().is_empty()) {
        return Ok((id, false));
    }
    let response = http
        .post(format!("{drive_base}/drive/v3/files"))
        .bearer_auth(access_token)
        .json(&serde_json::json!({
            "name": DEFAULT_NAME,
            "mimeType": "application/vnd.google-apps.folder",
        }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let response = ensure_success(response, "Creating the Drive folder").await?;
    let body: IdResponse = response.json().await.map_err(|e| e.to_string())?;
    Ok((
        body.id.ok_or("Drive created a folder but returned no id")?,
        true,
    ))
}

/// One meeting's own doc, created straight into `folder_id` and named
/// `title`. Drive makes it, not Docs, so it lands in the folder in one call
/// with nothing to move afterwards. A new doc always has one tab, which core
/// writes into.
pub async fn create_meeting_doc(
    http: &reqwest::Client,
    access_token: &str,
    folder_id: &str,
    title: &str,
) -> Result<String, String> {
    create_meeting_doc_at(http, GOOGLE_API_BASE, access_token, folder_id, title).await
}

async fn create_meeting_doc_at(
    http: &reqwest::Client,
    drive_base: &str,
    access_token: &str,
    folder_id: &str,
    title: &str,
) -> Result<String, String> {
    let response = http
        .post(format!("{drive_base}/drive/v3/files?fields=id"))
        .bearer_auth(access_token)
        .json(&serde_json::json!({
            "name": title,
            "mimeType": "application/vnd.google-apps.document",
            "parents": [folder_id],
        }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    // Drive answers 404 for a parent it can't see: the folder was deleted,
    // or emptied from the trash. Name the fix rather than the status code.
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(
            "The meeting notes folder is gone from Google Drive. Disconnect and connect again in Settings, Notes, to make a new one."
                .into(),
        );
    }
    let response = ensure_success(response, "Creating the meeting's Google Doc").await?;
    let body: IdResponse = response.json().await.map_err(|e| e.to_string())?;
    body.id
        .ok_or_else(|| "Drive created the meeting's doc but returned no id".into())
}

/// Moves the document into the folder even when the document already
/// existed: someone who picked a doc earlier and now chooses "create" still
/// wants that doc inside the folder.
async fn ensure_document_and_reparent(
    http: &reqwest::Client,
    drive_base: &str,
    docs_base: &str,
    access_token: &str,
    existing: &ExistingTarget,
    folder_id: &str,
) -> Result<(String, bool), String> {
    let (document_id, document_created) = match &existing.document_id {
        Some(id) => (id.clone(), false),
        None => {
            // Docs is its own host, not a path under the Drive host.
            let response = http
                .post(format!("{docs_base}/v1/documents"))
                .bearer_auth(access_token)
                .json(&serde_json::json!({ "title": DEFAULT_NAME }))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            let response = ensure_success(response, "Creating the Google Doc").await?;
            let body: DocsCreateResponse = response.json().await.map_err(|e| e.to_string())?;
            (
                body.document_id
                    .ok_or("Docs created a document but returned no id")?,
                true,
            )
        }
    };

    // A Drive file has one parent, and moving it needs addParents and
    // removeParents on the same call, so read the current parents first.
    let current = http
        .get(format!(
            "{drive_base}/drive/v3/files/{document_id}?fields=parents"
        ))
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let current = ensure_success(current, "Reading the document's folder").await?;
    let parents: ParentsResponse = current.json().await.map_err(|e| e.to_string())?;
    let remove_parents = parents
        .parents
        .filter(|p| !p.is_empty())
        .map(|p| p.join(","));

    let mut update = url::Url::parse(&format!("{drive_base}/drive/v3/files/{document_id}"))
        .map_err(|e| e.to_string())?;
    {
        let mut pairs = update.query_pairs_mut();
        pairs.append_pair("addParents", folder_id);
        pairs.append_pair("fields", "id");
        if let Some(remove) = &remove_parents {
            pairs.append_pair("removeParents", remove);
        }
    }
    let response = http
        .patch(update)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    ensure_success(response, "Moving the document into the folder").await?;

    Ok((document_id, document_created))
}

#[cfg(test)]
mod tests {
    use super::super::test_http::{client, serve, Route};
    use super::*;

    #[tokio::test]
    async fn both_ids_present_makes_no_calls() {
        let result = ensure_container_doc_at(
            &client(),
            "http://127.0.0.1:9",
            "http://127.0.0.1:9",
            "token",
            ExistingTarget {
                folder_id: Some("f1".into()),
                document_id: Some("d1".into()),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            result,
            ContainerDoc {
                folder_id: "f1".into(),
                document_id: "d1".into(),
                folder_created: false,
                document_created: false,
            }
        );
    }

    #[tokio::test]
    async fn creates_folder_on_drive_and_doc_on_docs_then_moves_the_doc() {
        let (drive, drive_seen) = serve(vec![
            Route::json("POST /drive/v3/files", 200, r#"{"id":"new-folder"}"#),
            Route::json(
                "GET /drive/v3/files/new-doc",
                200,
                r#"{"parents":["root"]}"#,
            ),
            Route::json("PATCH /drive/v3/files/new-doc", 200, r#"{"id":"new-doc"}"#),
        ])
        .await;
        let (docs, docs_seen) = serve(vec![Route::json(
            "POST /v1/documents",
            200,
            r#"{"documentId":"new-doc"}"#,
        )])
        .await;
        let result =
            ensure_container_doc_at(&client(), &drive, &docs, "token", ExistingTarget::default())
                .await
                .unwrap();
        assert_eq!(result.folder_id, "new-folder");
        assert_eq!(result.document_id, "new-doc");
        assert!(result.folder_created && result.document_created);

        let drive_seen = drive_seen.await.unwrap();
        let patch = drive_seen.iter().find(|r| r.method == "PATCH").unwrap();
        assert!(patch.target.contains("addParents=new-folder"));
        assert!(patch.target.contains("removeParents=root"));
        assert!(docs_seen.await.unwrap()[0].body.contains(DEFAULT_NAME));
    }

    #[tokio::test]
    async fn an_existing_doc_is_moved_into_the_new_folder() {
        let (drive, drive_seen) = serve(vec![
            Route::json("POST /drive/v3/files", 200, r#"{"id":"new-folder"}"#),
            Route::json(
                "GET /drive/v3/files/existing-doc",
                200,
                r#"{"parents":["elsewhere"]}"#,
            ),
            Route::json(
                "PATCH /drive/v3/files/existing-doc",
                200,
                r#"{"id":"existing-doc"}"#,
            ),
        ])
        .await;
        let result = ensure_container_doc_at(
            &client(),
            &drive,
            "http://127.0.0.1:9",
            "token",
            ExistingTarget {
                folder_id: None,
                document_id: Some("existing-doc".into()),
            },
        )
        .await
        .unwrap();
        assert!(!result.document_created);
        let patch = drive_seen
            .await
            .unwrap()
            .into_iter()
            .find(|r| r.method == "PATCH")
            .unwrap();
        assert!(patch.target.contains("removeParents=elsewhere"));
    }

    #[tokio::test]
    async fn a_failure_after_the_folder_exists_names_the_folder() {
        let (drive, _) = serve(vec![Route::json(
            "POST /drive/v3/files",
            200,
            r#"{"id":"orphaned-folder-id"}"#,
        )])
        .await;
        let (docs, _) = serve(vec![Route::json("POST /v1/documents", 500, "{}")]).await;
        let err =
            ensure_container_doc_at(&client(), &drive, &docs, "token", ExistingTarget::default())
                .await
                .unwrap_err();
        assert!(err.contains("orphaned-folder-id"), "{err}");
    }

    #[tokio::test]
    async fn an_existing_folder_is_kept_and_a_missing_one_is_made() {
        let kept = ensure_folder_at(&client(), "http://127.0.0.1:9", "token", Some("f1".into()))
            .await
            .unwrap();
        assert_eq!(kept, ("f1".to_string(), false));

        let (drive, seen) = serve(vec![Route::json(
            "POST /drive/v3/files",
            200,
            r#"{"id":"new-folder"}"#,
        )])
        .await;
        let made = ensure_folder_at(&client(), &drive, "token", Some("  ".into()))
            .await
            .unwrap();
        assert_eq!(made, ("new-folder".to_string(), true));
        let body = &seen.await.unwrap()[0].body;
        assert!(
            body.contains("application/vnd.google-apps.folder"),
            "{body}"
        );
        assert!(body.contains(DEFAULT_NAME), "{body}");
    }

    #[tokio::test]
    async fn a_meeting_doc_is_created_inside_the_folder_in_one_call() {
        let (drive, seen) = serve(vec![Route::json(
            "POST /drive/v3/files",
            200,
            r#"{"id":"meeting-doc"}"#,
        )])
        .await;
        let id = create_meeting_doc_at(&client(), &drive, "token", "f1", "Meeting 2026-11-25 0930")
            .await
            .unwrap();
        assert_eq!(id, "meeting-doc");
        let seen = seen.await.unwrap();
        assert_eq!(seen.len(), 1);
        let body: serde_json::Value = serde_json::from_str(&seen[0].body).unwrap();
        assert_eq!(
            body,
            serde_json::json!({
                "name": "Meeting 2026-11-25 0930",
                "mimeType": "application/vnd.google-apps.document",
                "parents": ["f1"],
            })
        );
    }

    #[tokio::test]
    async fn a_missing_folder_says_how_to_fix_it() {
        let (drive, _) = serve(vec![Route::json("POST /drive/v3/files", 404, "{}")]).await;
        let err = create_meeting_doc_at(&client(), &drive, "token", "gone", "Meeting")
            .await
            .unwrap_err();
        assert!(err.contains("folder is gone"), "{err}");
    }

    #[tokio::test]
    async fn a_failure_creating_the_folder_mentions_no_folder() {
        let (drive, _) = serve(vec![Route::json("POST /drive/v3/files", 500, "boom")]).await;
        let err = ensure_container_doc_at(
            &client(),
            &drive,
            "http://127.0.0.1:9",
            "token",
            ExistingTarget::default(),
        )
        .await
        .unwrap_err();
        assert!(err.contains("status 500"));
        assert!(!err.contains("already created"));
    }
}
