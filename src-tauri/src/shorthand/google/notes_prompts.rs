//! Fork: the user's own note-taking prompts for the Google Docs note taker,
//! kept like the AI cleanup prompts. The built-in Default is the notes
//! engine's own: it is never stored, and choosing it means passing no
//! prompt at all, so the engine's default stays current with each release.
//! Meetings and Assisted notes each choose their own prompt.

use std::process::Stdio;

use tauri::{AppHandle, Emitter};

use crate::settings::{self, AppSettings, LLMPrompt};
use crate::shorthand::mode::Mode;

/// The prompt `mode` uses, when it is one of the user's own with any text.
/// None means the engine's Default.
pub fn selected_prompt(settings: &AppSettings, mode: Mode) -> Option<&LLMPrompt> {
    let id = match mode {
        Mode::AssistedNotes => settings.assisted_notes_notes_prompt_id.as_deref(),
        Mode::Meeting | Mode::Dictation => settings.meeting_notes_prompt_id.as_deref(),
    }?;
    settings
        .notes_prompts
        .iter()
        .find(|p| p.id == id && !p.prompt.trim().is_empty())
}

fn announce(app: &AppHandle) {
    let _ = app.emit(
        "settings-changed",
        serde_json::json!({ "setting": "notes_prompts" }),
    );
}

#[tauri::command]
#[specta::specta]
pub fn add_notes_prompt(app: AppHandle, name: String, prompt: String) -> Result<LLMPrompt, String> {
    let mut settings = settings::get_settings(&app);
    let new_prompt = LLMPrompt {
        id: format!("notes_prompt_{}", chrono::Utc::now().timestamp_millis()),
        name,
        prompt,
    };
    settings.notes_prompts.push(new_prompt.clone());
    settings::write_settings(&app, settings);
    announce(&app);
    Ok(new_prompt)
}

#[tauri::command]
#[specta::specta]
pub fn update_notes_prompt(
    app: AppHandle,
    id: String,
    name: String,
    prompt: String,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    let existing = settings
        .notes_prompts
        .iter_mut()
        .find(|p| p.id == id)
        .ok_or_else(|| format!("Prompt with id '{id}' not found"))?;
    existing.name = name;
    existing.prompt = prompt;
    settings::write_settings(&app, settings);
    announce(&app);
    Ok(())
}

/// Deletes one of the user's prompts. Whatever used it goes back to Default.
#[tauri::command]
#[specta::specta]
pub fn delete_notes_prompt(app: AppHandle, id: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    remove_prompt(&mut settings, &id)?;
    settings::write_settings(&app, settings);
    announce(&app);
    Ok(())
}

fn remove_prompt(settings: &mut AppSettings, id: &str) -> Result<(), String> {
    let before = settings.notes_prompts.len();
    settings.notes_prompts.retain(|p| p.id != id);
    if settings.notes_prompts.len() == before {
        return Err(format!("Prompt with id '{id}' not found"));
    }
    for selection in [
        &mut settings.meeting_notes_prompt_id,
        &mut settings.assisted_notes_notes_prompt_id,
    ] {
        if selection.as_deref() == Some(id) {
            *selection = None;
        }
    }
    Ok(())
}

/// Chooses the prompt meetings (or, with `assisted`, Assisted notes) use.
/// None goes back to Default.
#[tauri::command]
#[specta::specta]
pub fn change_notes_prompt_selection(
    app: AppHandle,
    assisted: bool,
    id: Option<String>,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    if let Some(id) = &id {
        if !settings.notes_prompts.iter().any(|p| &p.id == id) {
            return Err(format!("Prompt with id '{id}' not found"));
        }
    }
    if assisted {
        settings.assisted_notes_notes_prompt_id = id;
    } else {
        settings.meeting_notes_prompt_id = id;
    }
    settings::write_settings(&app, settings);
    announce(&app);
    Ok(())
}

/// The notes engine's built-in note-taking prompt for meetings (or, with
/// `assisted`, for Assisted notes), as it writes for Google Docs. Shown read
/// only, and as the starting text of a new prompt.
#[tauri::command]
#[specta::specta]
pub async fn default_notes_prompt(assisted: bool) -> Result<String, String> {
    let engine = super::runner::engine_command()
        .ok_or("The notes engine isn't installed with this build of Shorthand.")?;
    tauri::async_runtime::spawn_blocking(move || {
        let output = engine
            .command()
            .args(default_guidance_args(assisted))
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("Couldn't start the notes engine: {e}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let reason = stderr.lines().rev().find(|l| !l.trim().is_empty());
            return Err(reason
                .map(str::to_string)
                .unwrap_or_else(|| "The notes engine couldn't show its default prompt.".into()));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

fn default_guidance_args(assisted: bool) -> Vec<&'static str> {
    let mut args = vec!["default-guidance", "--sink", "google"];
    if assisted {
        args.push("--assisted-notes");
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prompt(id: &str, text: &str) -> LLMPrompt {
        LLMPrompt {
            id: id.into(),
            name: id.into(),
            prompt: text.into(),
        }
    }

    #[test]
    fn each_mode_uses_its_own_prompt_and_none_means_default() {
        let mut settings = settings::get_default_settings();
        settings.notes_prompts = vec![prompt("a", "Short notes."), prompt("b", "Long notes.")];
        assert!(selected_prompt(&settings, Mode::Meeting).is_none());
        settings.meeting_notes_prompt_id = Some("a".into());
        settings.assisted_notes_notes_prompt_id = Some("b".into());
        assert_eq!(selected_prompt(&settings, Mode::Meeting).unwrap().id, "a");
        assert_eq!(
            selected_prompt(&settings, Mode::AssistedNotes).unwrap().id,
            "b"
        );
    }

    #[test]
    fn a_blank_or_missing_prompt_falls_back_to_default() {
        let mut settings = settings::get_default_settings();
        settings.notes_prompts = vec![prompt("blank", "   ")];
        settings.meeting_notes_prompt_id = Some("blank".into());
        settings.assisted_notes_notes_prompt_id = Some("gone".into());
        assert!(selected_prompt(&settings, Mode::Meeting).is_none());
        assert!(selected_prompt(&settings, Mode::AssistedNotes).is_none());
    }

    #[test]
    fn deleting_a_chosen_prompt_puts_that_mode_back_on_default() {
        let mut settings = settings::get_default_settings();
        settings.notes_prompts = vec![prompt("a", "x"), prompt("b", "y")];
        settings.meeting_notes_prompt_id = Some("a".into());
        settings.assisted_notes_notes_prompt_id = Some("b".into());
        remove_prompt(&mut settings, "a").unwrap();
        assert_eq!(settings.meeting_notes_prompt_id, None);
        assert_eq!(
            settings.assisted_notes_notes_prompt_id.as_deref(),
            Some("b")
        );
        // The last prompt can go too: Default is always there.
        remove_prompt(&mut settings, "b").unwrap();
        assert!(settings.notes_prompts.is_empty());
        assert!(remove_prompt(&mut settings, "b").is_err());
    }

    #[test]
    fn the_default_is_asked_for_in_the_google_docs_format() {
        assert_eq!(
            default_guidance_args(false),
            ["default-guidance", "--sink", "google"]
        );
        assert_eq!(
            default_guidance_args(true),
            ["default-guidance", "--sink", "google", "--assisted-notes"]
        );
    }
}
