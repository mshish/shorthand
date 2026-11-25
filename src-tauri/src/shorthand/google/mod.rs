//! Fork-only "Notes" target: Google Docs. Sign in to Google from the app,
//! pick or create the document meetings go into, and have each meeting fill
//! a new tab in it live. See
//! docs/superpowers/specs/2026-08-25-shorthand-umbrella-design.md, Phase 2.
//!
//! The app owns sign-in and the credentials file; shorthand-core does the
//! writing. `runner.rs` starts core's `capture --sink google` when a meeting
//! begins and core follows the app's follow-stream like any other follower.
//!
//! The OAuth client, first match wins:
//! 1. One the user brought under Settings, Notes, Advanced: the client ID in
//!    settings and its secret in the OS keyring.
//! 2. `SHORTHAND_GOOGLE_CLIENT_ID` / `SHORTHAND_GOOGLE_CLIENT_SECRET` at run
//!    time, for development.
//! 3. The same variables at compile time: the built-in client (a Google
//!    Cloud "Desktop app" client with the `drive.file` scope).
//!
//! A build with none of them still compiles; connecting then says sign-in
//! isn't set up. A connection keeps the client it was made with (core
//! refreshes with the pair in the credentials file), so a change here
//! applies from the next connect.

pub mod container_doc;
pub mod credentials;
pub mod document_info;
pub mod oauth;
pub mod runner;
#[cfg(test)]
mod test_http;

use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_opener::OpenerExt;

use crate::shorthand::credentials::{CredentialSlot, CredentialStatus, CredentialStore};

/// Must match shorthand-core's `GOOGLE_DOCS_SCOPE`; both repos' scope
/// guards keep it from widening.
pub const GOOGLE_DOCS_SCOPE: &str = "https://www.googleapis.com/auth/drive.file";

/// The loopback port Google redirects to after consent. The same one
/// shorthand-config uses, so an existing OAuth client's redirect works.
const REDIRECT_PORT: u16 = 8721;

/// Emitted whenever anything `get_google_docs_status` reports may have
/// changed, so the settings page can refresh.
pub const STATUS_EVENT: &str = "google-docs-status-changed";

/// Where finished meeting notes go. Obsidian is the default and behaves as
/// before: an external follower (the Obsidian plugin) fills the note.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default, Type)]
#[serde(rename_all = "snake_case")]
pub enum NotesTarget {
    #[default]
    Obsidian,
    GoogleDocs,
}

struct OAuthClient {
    id: String,
    secret: String,
}

fn custom_oauth_client(app: &AppHandle) -> Option<OAuthClient> {
    let id = crate::settings::get_settings(app)
        .google_oauth_client_id
        .filter(|id| !id.trim().is_empty())?;
    let secret = credential_store(app)?
        .get(&CredentialSlot::GoogleOAuthClient)
        .ok()
        .flatten()?;
    Some(OAuthClient { id, secret })
}

fn credential_store(app: &AppHandle) -> Option<Arc<CredentialStore>> {
    app.try_state::<Arc<CredentialStore>>()
        .map(|state| state.inner().clone())
}

fn oauth_client(app: &AppHandle) -> Option<OAuthClient> {
    custom_oauth_client(app).or_else(built_in_oauth_client)
}

fn built_in_oauth_client() -> Option<OAuthClient> {
    let pick = |name: &str, compiled: Option<&'static str>| {
        std::env::var(name)
            .ok()
            .or_else(|| compiled.map(String::from))
            .filter(|v| !v.trim().is_empty())
    };
    Some(OAuthClient {
        id: pick(
            "SHORTHAND_GOOGLE_CLIENT_ID",
            option_env!("SHORTHAND_GOOGLE_CLIENT_ID"),
        )?,
        secret: pick(
            "SHORTHAND_GOOGLE_CLIENT_SECRET",
            option_env!("SHORTHAND_GOOGLE_CLIENT_SECRET"),
        )?,
    })
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq, Type)]
pub struct GoogleDocsStatus {
    /// This build has an OAuth client to sign in with.
    pub sign_in_available: bool,
    pub connected: bool,
    /// The document meetings go into, when connected.
    pub document_url: Option<String>,
    /// Its name in Google Drive, when it could be read at sign-in.
    pub document_name: Option<String>,
    /// The notes engine (shorthand-core) was found next to the app.
    pub engine_found: bool,
    pub last_run: Option<runner::LastRun>,
    /// Advanced: the client ID of the user's own OAuth client, if set.
    pub custom_client_id: Option<String>,
    pub custom_client_secret: CredentialStatus,
    /// Sign-in uses the user's own client, not the built-in one.
    pub using_custom_client: bool,
}

pub fn document_url(document_id: &str) -> String {
    format!("https://docs.google.com/document/d/{document_id}/edit")
}

fn document_info_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    crate::portable::app_data_dir(app)
        .ok()
        .map(|dir| document_info::info_path(&dir))
}

fn current_status(app: &AppHandle) -> GoogleDocsStatus {
    let document_id = credentials::credentials_path()
        .ok()
        .and_then(|path| credentials::read(&path))
        .and_then(|c| c.document_id);
    let document_name = document_id.as_deref().and_then(|id| {
        document_info_path(app).and_then(|path| document_info::read_name(&path, id))
    });
    let custom_client_id = crate::settings::get_settings(app)
        .google_oauth_client_id
        .filter(|id| !id.trim().is_empty());
    let custom_client_secret = credential_store(app)
        .map(|store| store.status(&CredentialSlot::GoogleOAuthClient))
        .unwrap_or(CredentialStatus::Unavailable);
    let using_custom_client =
        custom_client_id.is_some() && custom_client_secret == CredentialStatus::Configured;
    GoogleDocsStatus {
        sign_in_available: using_custom_client || built_in_oauth_client().is_some(),
        connected: document_id.is_some(),
        document_url: document_id.as_deref().map(document_url),
        document_name,
        engine_found: runner::engine_command().is_some(),
        last_run: runner::last_run(),
        custom_client_id,
        custom_client_secret,
        using_custom_client,
    }
}

pub(crate) fn notify_status_changed(app: &AppHandle) {
    let _ = app.emit(STATUS_EVENT, ());
}

static SIGN_IN_RUNNING: AtomicBool = AtomicBool::new(false);
/// Aborts the wait for Google's redirect, for the Cancel button.
static SIGN_IN_ABORT: Mutex<Option<tokio::task::AbortHandle>> = Mutex::new(None);

const SIGN_IN_CANCELLED: &str = "Google sign-in was cancelled.";

fn set_sign_in_abort(handle: Option<tokio::task::AbortHandle>) {
    if let Ok(mut slot) = SIGN_IN_ABORT.lock() {
        *slot = handle;
    }
}

struct SignInGuard;
impl Drop for SignInGuard {
    fn drop(&mut self) {
        set_sign_in_abort(None);
        SIGN_IN_RUNNING.store(false, Ordering::Release);
    }
}

/// PKCE, consent in the browser, the loopback redirect, token exchange, then
/// pick or create the document, then write the credentials file.
async fn sign_in(app: &AppHandle, create_new: bool) -> Result<(), String> {
    let client = oauth_client(app).ok_or(
        "Google sign-in isn't set up in this build of Shorthand. It needs a Google OAuth client ID.",
    )?;
    if SIGN_IN_RUNNING.swap(true, Ordering::AcqRel) {
        return Err("Google sign-in is already open in your browser.".into());
    }
    let _guard = SignInGuard;

    let redirect_uri = format!("http://127.0.0.1:{REDIRECT_PORT}/callback");
    let challenge = oauth::generate_pkce_challenge();
    let authorization_url = oauth::build_authorization_url(&oauth::AuthorizationUrlOptions {
        client_id: &client.id,
        redirect_uri: &redirect_uri,
        code_challenge: &challenge.code_challenge,
        scope: GOOGLE_DOCS_SCOPE,
        use_picker: !create_new,
    });
    let listener = oauth::bind_loopback(REDIRECT_PORT).await?;
    app.opener()
        .open_url(&authorization_url, None::<&str>)
        .map_err(|e| format!("Couldn't open your browser for Google sign-in: {e}"))?;
    let waiting = tokio::spawn(oauth::listen_for_redirect(
        listener,
        oauth::REDIRECT_LISTEN_TIMEOUT,
    ));
    set_sign_in_abort(Some(waiting.abort_handle()));
    let redirect = match waiting.await {
        Ok(result) => result?,
        Err(e) if e.is_cancelled() => return Err(SIGN_IN_CANCELLED.into()),
        Err(e) => return Err(format!("Google sign-in stopped unexpectedly: {e}")),
    };
    set_sign_in_abort(None);

    let http = reqwest::Client::new();
    let pair = oauth::ClientCredentials {
        client_id: &client.id,
        client_secret: &client.secret,
    };
    let refresh_token = oauth::exchange_code(
        &http,
        &pair,
        &redirect.code,
        &challenge.code_verifier,
        &redirect_uri,
    )
    .await?;

    let path = credentials::credentials_path()
        .map_err(|e| format!("Couldn't find Shorthand's config folder: {e}"))?;
    let access_token = oauth::refresh_access_token(&http, &pair, &refresh_token).await?;
    let (document_id, folder_id) = if create_new {
        let existing = credentials::read(&path)
            .map(|c| container_doc::ExistingTarget {
                folder_id: c.folder_id,
                document_id: c.document_id,
            })
            .unwrap_or_default();
        let doc = container_doc::ensure_container_doc(&http, &access_token, existing).await?;
        (doc.document_id, Some(doc.folder_id))
    } else {
        let picked = redirect
            .picked_file_ids
            .into_iter()
            .next()
            .ok_or("No document was picked. Connect again and choose a Google Doc.")?;
        (picked, None)
    };

    let name = document_info::fetch_name(&http, &access_token, &document_id).await;

    credentials::write(
        &path,
        &credentials::Credentials {
            client_id: client.id,
            client_secret: client.secret,
            refresh_token,
            document_id: Some(document_id.clone()),
            folder_id,
        },
    )
    .map_err(|e| format!("Couldn't save the Google connection: {e}"))?;

    // The name is only a label; a doc that connects without one still works.
    if let Some(info_path) = document_info_path(app) {
        let _ = match name {
            Some(name) => document_info::write(
                &info_path,
                &document_info::DocumentInfo { document_id, name },
            ),
            None => document_info::remove(&info_path),
        };
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn get_google_docs_status(app: AppHandle) -> GoogleDocsStatus {
    current_status(&app)
}

/// `create_new` false shows Google's picker to choose an existing doc; true
/// creates a "Shorthand Meeting Notes" doc in a folder of the same name.
#[tauri::command]
#[specta::specta]
pub async fn connect_google_docs(
    app: AppHandle,
    create_new: bool,
) -> Result<GoogleDocsStatus, String> {
    let result = sign_in(&app, create_new).await;
    notify_status_changed(&app);
    result.map(|()| current_status(&app))
}

/// Stops waiting for Google's redirect, for when the browser tab was closed
/// or the wrong account was picked. The pending connect returns
/// "Google sign-in was cancelled."
#[tauri::command]
#[specta::specta]
pub fn cancel_google_sign_in() {
    if let Some(handle) = SIGN_IN_ABORT.lock().ok().and_then(|mut slot| slot.take()) {
        handle.abort();
    }
}

/// Forgets the connection by removing the credentials file. Google keeps
/// the grant until it's removed from the Google account's third-party access
/// page; the document itself is untouched.
#[tauri::command]
#[specta::specta]
pub fn disconnect_google_docs(app: AppHandle) -> Result<GoogleDocsStatus, String> {
    let path = credentials::credentials_path().map_err(|e| e.to_string())?;
    credentials::remove(&path)
        .map_err(|e| format!("Couldn't remove the Google connection: {e}"))?;
    if let Some(info_path) = document_info_path(&app) {
        let _ = document_info::remove(&info_path);
    }
    notify_status_changed(&app);
    Ok(current_status(&app))
}

#[tauri::command]
#[specta::specta]
pub fn open_google_doc(app: AppHandle) -> Result<(), String> {
    let url = current_status(&app)
        .document_url
        .ok_or("Google Docs isn't connected.")?;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn change_notes_target_setting(app: AppHandle, target: NotesTarget) -> Result<(), String> {
    let mut settings = crate::settings::get_settings(&app);
    settings.notes_target = target;
    crate::settings::write_settings(&app, settings);
    let _ = app.emit(
        "settings-changed",
        serde_json::json!({ "setting": "notes_target", "value": target }),
    );
    Ok(())
}

/// Advanced: the client ID of the user's own OAuth client. Blank clears it,
/// which goes back to the built-in client.
#[tauri::command]
#[specta::specta]
pub fn change_google_oauth_client_id_setting(
    app: AppHandle,
    client_id: String,
) -> Result<GoogleDocsStatus, String> {
    let client_id = Some(client_id.trim().to_string()).filter(|id| !id.is_empty());
    let mut settings = crate::settings::get_settings(&app);
    settings.google_oauth_client_id = client_id.clone();
    crate::settings::write_settings(&app, settings);
    let _ = app.emit(
        "settings-changed",
        serde_json::json!({ "setting": "google_oauth_client_id", "value": client_id }),
    );
    notify_status_changed(&app);
    Ok(current_status(&app))
}

/// Advanced: the secret of the user's own OAuth client, kept in the OS
/// keyring like every other secret the app holds.
#[tauri::command]
#[specta::specta]
pub fn set_google_oauth_client_secret(
    app: AppHandle,
    secret: String,
) -> Result<GoogleDocsStatus, String> {
    let store = credential_store(&app).ok_or("Secure storage isn't ready yet.")?;
    store
        .set(&CredentialSlot::GoogleOAuthClient, &secret)
        .map_err(|e| e.to_string())?;
    notify_status_changed(&app);
    Ok(current_status(&app))
}

#[tauri::command]
#[specta::specta]
pub fn clear_google_oauth_client_secret(app: AppHandle) -> Result<GoogleDocsStatus, String> {
    let store = credential_store(&app).ok_or("Secure storage isn't ready yet.")?;
    store
        .clear(&CredentialSlot::GoogleOAuthClient)
        .map_err(|e| e.to_string())?;
    notify_status_changed(&app);
    Ok(current_status(&app))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_target_defaults_to_obsidian_and_serializes_snake_case() {
        assert_eq!(NotesTarget::default(), NotesTarget::Obsidian);
        assert_eq!(
            serde_json::to_string(&NotesTarget::GoogleDocs).unwrap(),
            "\"google_docs\""
        );
        assert_eq!(
            serde_json::from_str::<NotesTarget>("\"obsidian\"").unwrap(),
            NotesTarget::Obsidian
        );
    }

    #[test]
    fn document_url_opens_the_doc_for_editing() {
        assert_eq!(
            document_url("abc"),
            "https://docs.google.com/document/d/abc/edit"
        );
    }

    #[test]
    fn scope_is_drive_file_only() {
        assert_eq!(
            GOOGLE_DOCS_SCOPE,
            "https://www.googleapis.com/auth/drive.file"
        );
    }
}
