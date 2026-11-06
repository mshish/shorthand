use crate::managers::{
    history::{HistoryEntry, HistoryManager, PaginatedHistory},
    transcription_jobs::TranscriptionJobManager,
};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, State};

/// Rejects a retry attempt for a history entry that has no saved recording
/// (an empty `file_name`, e.g. because the save-recordings toggle was off
/// when it was created) — there is no audio file to re-transcribe.
pub(crate) fn require_recording(file_name: &str) -> Result<(), String> {
    if file_name.is_empty() {
        Err("This entry has no saved recording to re-transcribe".to_string())
    } else {
        Ok(())
    }
}

/// Why an entry can't be transcribed again, if it can't: it has no recording,
/// or its text merges two speakers (so a single pass would drop one).
/// `retry_unavailable` is the user-facing line for the merged case.
pub(crate) fn retry_blocker(
    file_name: &str,
    text: &str,
    retry_unavailable: impl FnOnce() -> String,
) -> Option<String> {
    if let Err(message) = require_recording(file_name) {
        return Some(message);
    }
    crate::tray_i18n::is_merged_transcript(text).then(retry_unavailable)
}

/// [`retry_blocker`] with the app's language for the merged-transcript line.
pub(crate) fn check_retryable(app: &AppHandle, entry: &HistoryEntry) -> Result<(), String> {
    match retry_blocker(&entry.file_name, &entry.transcription_text, || {
        crate::tray_i18n::merged_transcript_retry_error_for_app(app, &entry.transcription_text)
            .unwrap_or_default()
    }) {
        Some(message) => Err(message),
        None => Ok(()),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn get_history_entries(
    _app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    cursor: Option<i64>,
    limit: Option<usize>,
) -> Result<PaginatedHistory, String> {
    history_manager
        .get_history_entries(cursor, limit)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn toggle_history_entry_saved(
    _app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    id: i64,
) -> Result<(), String> {
    history_manager
        .toggle_saved_status(id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn get_audio_file_path(
    _app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    file_name: String,
) -> Result<String, String> {
    let path = history_manager.get_audio_file_path(&file_name);
    path.to_str()
        .ok_or_else(|| "Invalid file path".to_string())
        .map(|s| s.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn delete_history_entry(
    _app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    jobs: State<'_, Arc<TranscriptionJobManager>>,
    id: i64,
) -> Result<(), String> {
    jobs.cancel(id);
    history_manager
        .delete_entry(id)
        .await
        .map_err(|e| e.to_string())
}

/// Queues the entry to be transcribed again with the current model. Its
/// progress and outcome arrive as `TranscriptionJob` events. Refused up
/// front, with the reason, when the entry can't be transcribed again; the job
/// checks again when it runs.
#[tauri::command]
#[specta::specta]
pub async fn retry_history_entry_transcription(
    app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    jobs: State<'_, Arc<TranscriptionJobManager>>,
    id: i64,
) -> Result<(), String> {
    let entry = history_manager
        .get_entry_by_id(id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("History entry {id} not found"))?;
    check_retryable(&app, &entry)?;
    jobs.retry(id);
    Ok(())
}

/// Stops the entry's background transcription: a queued one is dropped, a
/// running one stops after the chunk it is on. The entry keeps the text it
/// had before.
#[tauri::command]
#[specta::specta]
pub fn cancel_history_entry_transcription(jobs: State<'_, Arc<TranscriptionJobManager>>, id: i64) {
    jobs.cancel(id);
}

/// Adds an audio or video file to History and queues its transcription.
/// Fails without adding anything if the file has no audio Handy can read.
#[tauri::command]
#[specta::specta]
pub async fn transcribe_file(
    jobs: State<'_, Arc<TranscriptionJobManager>>,
    path: String,
) -> Result<HistoryEntry, String> {
    let jobs = Arc::clone(&jobs);
    tauri::async_runtime::spawn_blocking(move || jobs.transcribe_file(PathBuf::from(path)))
        .await
        .map_err(|e| format!("Failed to read the file: {}", e))?
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn update_history_limit(
    app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    limit: usize,
) -> Result<(), String> {
    let mut settings = crate::settings::get_settings(&app);
    settings.history_limit = limit;
    crate::settings::write_settings(&app, settings);

    history_manager
        .cleanup_old_entries()
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn update_recording_retention_period(
    app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    period: String,
) -> Result<(), String> {
    use crate::settings::RecordingRetentionPeriod;

    let retention_period = match period.as_str() {
        "never" => RecordingRetentionPeriod::Never,
        "preserve_limit" => RecordingRetentionPeriod::PreserveLimit,
        "days3" => RecordingRetentionPeriod::Days3,
        "weeks2" => RecordingRetentionPeriod::Weeks2,
        "months3" => RecordingRetentionPeriod::Months3,
        _ => return Err(format!("Invalid retention period: {}", period)),
    };

    let mut settings = crate::settings::get_settings(&app);
    settings.recording_retention_period = retention_period;
    crate::settings::write_settings(&app, settings);

    history_manager
        .cleanup_old_entries()
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn require_recording_rejects_empty_file_name() {
        let err = require_recording("").expect_err("empty file_name must be rejected");
        assert_eq!(err, "This entry has no saved recording to re-transcribe");
    }

    #[test]
    fn require_recording_accepts_non_empty_file_name() {
        assert!(require_recording("handy-123.wav").is_ok());
    }

    #[test]
    fn retry_blocker_refuses_an_entry_without_a_recording() {
        let message = retry_blocker("", "hello", || "merged".to_string());
        assert_eq!(
            message.as_deref(),
            Some("This entry has no saved recording to re-transcribe")
        );
    }

    #[test]
    fn retry_blocker_refuses_a_merged_two_speaker_transcript() {
        let message = retry_blocker("a.wav", "You: hi\nSystem: hello", || "merged".to_string());
        assert_eq!(message.as_deref(), Some("merged"));
    }

    #[test]
    fn retry_blocker_allows_a_plain_entry_with_a_recording() {
        assert_eq!(
            retry_blocker("a.wav", "hello", || unreachable!("not merged")),
            None
        );
    }
}
