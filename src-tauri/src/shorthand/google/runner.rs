//! Starts shorthand-core's `capture --sink google` when a meeting begins and
//! Google Docs is the notes target. Core attaches to this app's follow-stream
//! (`shorthand --follow-stream json`), fills a new tab in the connected doc
//! as the meeting goes, and exits after that one recording
//! (`--exit-after-session`, core 0.24.5 or later).
//!
//! Core needs a markdown note to anchor a capture, so each meeting gets one
//! in `<app data>/google-docs-notes/`, named for the meeting's start time.
//! The doc tab takes the same name. That folder also holds core's transcript
//! sidecars.
//!
//! With "a doc per meeting" (`GoogleDocsLayout::DocPerMeeting`) the app
//! first creates that meeting's own doc in the meeting notes folder, named
//! the same way, and hands core its id (`--google-document`, in the core
//! release after 0.24.4), so core writes into the doc's one tab instead of adding a tab.
//!
//! When Meeting mode's "Save transcripts" setting is on, core also gets
//! `--google-transcript` and keeps the transcript in a "Transcript" tab: a
//! child of the meeting's tab, or a top-level tab in a meeting's own doc.
//!
//! The engine is the `shorthand-notes` binary bundled next to the app (see
//! `scripts/build-notes-engine.sh`), or whatever `SHORTHAND_NOTES_BIN` names
//! (a `.mjs`/`.js` file runs under `node`), for development.

use serde::Serialize;
use specta::Type;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};

use super::{GoogleDocsLayout, NotesTarget};
use crate::shorthand::mode::Mode;

const ENGINE_NAME: &str = "shorthand-notes";
const NOTES_FOLDER: &str = "google-docs-notes";

#[derive(Serialize, Debug, Clone, PartialEq, Eq, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LastRun {
    Running { title: String },
    Finished { title: String },
    Failed { title: String, message: String },
}

static LAST_RUN: Mutex<Option<LastRun>> = Mutex::new(None);

pub fn last_run() -> Option<LastRun> {
    LAST_RUN.lock().ok().and_then(|run| run.clone())
}

fn set_last_run(app: &AppHandle, run: LastRun) {
    if let Ok(mut slot) = LAST_RUN.lock() {
        *slot = Some(run);
    }
    super::notify_status_changed(app);
}

/// Tells the overlay that the doc a meeting's notes go to changed. The
/// payload is that doc, or `null` once the meeting's notes are done.
pub const DESTINATION_EVENT: &str = "notes-destination-changed";

/// What the overlay shows of the doc a running meeting writes to. The link
/// itself stays here: `open_notes_destination` opens it, so the overlay never
/// hands the app a URL to open.
#[derive(Serialize, Debug, Clone, PartialEq, Eq, Type)]
pub struct NotesDestination {
    /// The doc's name, when it is known.
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Destination {
    name: Option<String>,
    url: String,
    /// Tells apart two meetings that write to the same doc.
    run: u64,
}

static DESTINATION: Mutex<Option<Destination>> = Mutex::new(None);
static NEXT_RUN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub fn notes_destination() -> Option<NotesDestination> {
    DESTINATION
        .lock()
        .ok()
        .and_then(|slot| slot.clone())
        .map(|d| NotesDestination { name: d.name })
}

pub fn notes_destination_url() -> Option<String> {
    DESTINATION
        .lock()
        .ok()
        .and_then(|slot| slot.as_ref().map(|d| d.url.clone()))
}

/// Shows the meeting's doc in the overlay for as long as this lives. Dropping
/// it, on any way out of `run_meeting`, takes the doc away again, unless a
/// later meeting has replaced it meanwhile.
struct ShownDestination<'a> {
    app: &'a AppHandle,
    run: u64,
}

impl<'a> ShownDestination<'a> {
    fn show(app: &'a AppHandle, name: Option<String>, document_id: &str) -> Self {
        let run = NEXT_RUN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if let Ok(mut slot) = DESTINATION.lock() {
            *slot = Some(Destination {
                name: name.clone(),
                url: super::document_url(document_id),
                run,
            });
        }
        let _ = app.emit(DESTINATION_EVENT, Some(NotesDestination { name }));
        Self { app, run }
    }
}

impl Drop for ShownDestination<'_> {
    fn drop(&mut self) {
        let cleared = DESTINATION.lock().is_ok_and(|mut slot| {
            let ours = slot.as_ref().is_some_and(|d| d.run == self.run);
            if ours {
                *slot = None;
            }
            ours
        });
        if cleared {
            let _ = self.app.emit(DESTINATION_EVENT, None::<NotesDestination>);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineCommand {
    pub program: PathBuf,
    pub leading_args: Vec<String>,
}

impl EngineCommand {
    fn command(&self) -> Command {
        let mut command = Command::new(&self.program);
        command.args(&self.leading_args);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        command
    }
}

fn resolve_engine(
    override_path: Option<String>,
    app_exe: Option<PathBuf>,
) -> Option<EngineCommand> {
    if let Some(path) = override_path.filter(|p| !p.trim().is_empty()) {
        let path = PathBuf::from(path);
        let is_script = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("mjs") || e.eq_ignore_ascii_case("js"));
        return Some(if is_script {
            EngineCommand {
                program: PathBuf::from("node"),
                leading_args: vec![path.to_string_lossy().into_owned()],
            }
        } else {
            EngineCommand {
                program: path,
                leading_args: Vec::new(),
            }
        });
    }
    // Tauri bundles an `externalBin` next to the app's own executable, with
    // the target-triple suffix it was built under removed.
    let bundled = app_exe?
        .parent()?
        .join(format!("{ENGINE_NAME}{}", std::env::consts::EXE_SUFFIX));
    bundled.is_file().then_some(EngineCommand {
        program: bundled,
        leading_args: Vec::new(),
    })
}

pub fn engine_command() -> Option<EngineCommand> {
    resolve_engine(
        std::env::var("SHORTHAND_NOTES_BIN").ok(),
        std::env::current_exe().ok(),
    )
}

/// "Meeting 2026-11-25 0930", with " 2", " 3"… when that note already
/// exists. No characters that a file name or a doc tab would trip on.
fn unique_title(folder: &Path, started: chrono::DateTime<chrono::Local>) -> String {
    let base = format!("Meeting {}", started.format("%Y-%m-%d %H%M"));
    (1..)
        .map(|n| {
            if n == 1 {
                base.clone()
            } else {
                format!("{base} {n}")
            }
        })
        .find(|title| !folder.join(format!("{title}.md")).exists())
        .expect("an unused title always exists")
}

fn capture_args(
    folder: &Path,
    note: &str,
    app_exe: &Path,
    agent_args: &[String],
    own_document: Option<&str>,
    save_transcript: bool,
) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "capture".into(),
        "--vault".into(),
        folder.to_string_lossy().into_owned(),
        "--note".into(),
        note.into(),
        "--enhance".into(),
        "--sink".into(),
        "google".into(),
    ];
    if let Some(document) = own_document {
        args.push(format!("--google-document={document}"));
    }
    if save_transcript {
        args.push("--google-transcript".into());
    }
    args.push("--shorthand".to_string());
    args.push(app_exe.to_string_lossy().into_owned());
    args.extend(agent_args.iter().cloned());
    args.push("--exit-after-session".into());
    args
}

/// The last thing the engine said on stderr is the most useful reason; the
/// exit code is the fallback.
fn failure_message(stderr_tail: &[String], code: Option<i32>) -> String {
    stderr_tail
        .iter()
        .rev()
        .find(|line| !line.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| match code {
            Some(code) => format!("The notes engine stopped with exit code {code}."),
            None => "The notes engine was stopped.".to_string(),
        })
}

/// Called right after a capture starts. `published` is whether that capture
/// reaches follow-stream followers; without it there's nothing for core to
/// follow.
pub fn on_capture_begin(app: &AppHandle, published: bool) {
    if !published || crate::shorthand::mode::active(app) != Mode::Meeting {
        return;
    }
    if crate::settings::get_settings(app).notes_target != NotesTarget::GoogleDocs {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || run_meeting(&app));
}

fn run_meeting(app: &AppHandle) {
    let started = chrono::Local::now();
    let fail = |title: String, message: String| {
        log::warn!("Google Docs notes for {title}: {message}");
        set_last_run(app, LastRun::Failed { title, message });
    };
    let fallback_title = format!("Meeting {}", started.format("%Y-%m-%d %H%M"));

    let layout = crate::settings::get_settings(app).google_docs_layout;
    let saved = super::credentials::credentials_path()
        .ok()
        .and_then(|path| super::credentials::read(&path));
    let connected = saved.as_ref().is_some_and(|c| match layout {
        GoogleDocsLayout::TabPerMeeting => c.document_id.is_some(),
        GoogleDocsLayout::DocPerMeeting => c.folder_id.is_some(),
    });
    let Some(saved) = saved.filter(|_| connected) else {
        return fail(
            fallback_title,
            match layout {
                GoogleDocsLayout::TabPerMeeting => {
                    "Google Docs isn't connected. Connect it in Settings, Notes."
                }
                GoogleDocsLayout::DocPerMeeting => {
                    "The meeting notes folder isn't set up. Set it up in Settings, Notes."
                }
            }
            .into(),
        );
    };
    let agent_args = match crate::settings::get_settings(app)
        .google_notes_agent
        .capture_args()
    {
        Ok(args) => args,
        Err(message) => return fail(fallback_title, message),
    };
    let Some(engine) = engine_command() else {
        return fail(
            fallback_title,
            "The notes engine isn't installed with this build of Shorthand.".into(),
        );
    };
    let app_exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(e) => {
            return fail(
                fallback_title,
                format!("Couldn't find Shorthand itself: {e}"),
            )
        }
    };
    let folder = match crate::portable::app_data_dir(app) {
        Ok(dir) => dir.join(NOTES_FOLDER),
        Err(e) => {
            return fail(
                fallback_title,
                format!("Couldn't find the app data folder: {e}"),
            )
        }
    };
    if let Err(e) = std::fs::create_dir_all(&folder) {
        return fail(
            fallback_title,
            format!("Couldn't create {}: {e}", folder.display()),
        );
    }
    let title = unique_title(&folder, started);
    let note = format!("{title}.md");
    set_last_run(
        app,
        LastRun::Running {
            title: title.clone(),
        },
    );

    let own_document = match (layout, saved.folder_id.as_deref()) {
        (GoogleDocsLayout::DocPerMeeting, Some(folder_id)) => {
            match tauri::async_runtime::block_on(create_meeting_doc(app, &saved, folder_id, &title))
            {
                Ok(id) => Some(id),
                Err(message) => return fail(title, message),
            }
        }
        _ => None,
    };
    // A meeting's own doc is named for the meeting; the connected doc keeps
    // the name it had in Drive at sign-in, and the meeting goes in a new tab.
    let _shown = match (&own_document, saved.document_id.as_deref()) {
        (Some(id), _) => Some(ShownDestination::show(app, Some(title.clone()), id)),
        (None, Some(id)) => Some(ShownDestination::show(
            app,
            super::connected_document_name(app, id),
            id,
        )),
        (None, None) => None,
    };

    let init = engine
        .command()
        .args(["init-note", "--vault"])
        .arg(&folder)
        .args(["--note", note.as_str(), "--title", title.as_str()])
        .stdin(Stdio::null())
        .output();
    match init {
        Ok(output) if output.status.success() => {}
        Ok(output) => {
            let stderr: Vec<String> = String::from_utf8_lossy(&output.stderr)
                .lines()
                .map(String::from)
                .collect();
            return fail(title, failure_message(&stderr, output.status.code()));
        }
        Err(e) => return fail(title, format!("Couldn't start the notes engine: {e}")),
    }

    let child = engine
        .command()
        .args(capture_args(
            &folder,
            &note,
            &app_exe,
            &agent_args,
            own_document.as_deref(),
            // Meeting mode's "Save transcripts" setting: the transcript also
            // goes in the doc, in a "Transcript" tab.
            crate::settings::get_settings(app).save_transcripts,
        ))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(e) => return fail(title, format!("Couldn't start the notes engine: {e}")),
    };
    let mut tail: Vec<String> = Vec::new();
    if let Some(stderr) = child.stderr.take() {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            log::info!("[shorthand-notes] {line}");
            tail.push(line);
            if tail.len() > 8 {
                tail.remove(0);
            }
        }
    }
    match child.wait() {
        Ok(status) if status.success() => set_last_run(app, LastRun::Finished { title }),
        Ok(status) => fail(title, failure_message(&tail, status.code())),
        Err(e) => fail(title, format!("Lost track of the notes engine: {e}")),
    }
}

/// The meeting's own doc, made in the folder before core starts. Reads the
/// folder's name again on the way, so a folder renamed in Drive shows its
/// new name in settings.
async fn create_meeting_doc(
    app: &AppHandle,
    saved: &super::credentials::Credentials,
    folder_id: &str,
    title: &str,
) -> Result<String, String> {
    let http = reqwest::Client::new();
    let pair = super::oauth::ClientCredentials {
        client_id: &saved.client_id,
        client_secret: &saved.client_secret,
    };
    let access_token =
        super::oauth::refresh_access_token(&http, &pair, &saved.refresh_token).await?;
    let id =
        super::container_doc::create_meeting_doc(&http, &access_token, folder_id, title).await?;
    super::remember_folder_name(app, &http, &access_token, folder_id).await;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn override_runs_scripts_under_node_and_binaries_directly() {
        assert_eq!(
            resolve_engine(Some("/dev/core/dist/shorthand-notes.mjs".into()), None),
            Some(EngineCommand {
                program: PathBuf::from("node"),
                leading_args: vec!["/dev/core/dist/shorthand-notes.mjs".into()],
            })
        );
        assert_eq!(
            resolve_engine(Some("/opt/shorthand-notes".into()), None),
            Some(EngineCommand {
                program: PathBuf::from("/opt/shorthand-notes"),
                leading_args: Vec::new(),
            })
        );
    }

    #[test]
    fn bundled_engine_is_found_next_to_the_app_and_only_if_present() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("shorthand");
        assert_eq!(resolve_engine(None, Some(exe.clone())), None);
        let engine = dir
            .path()
            .join(format!("{ENGINE_NAME}{}", std::env::consts::EXE_SUFFIX));
        std::fs::write(&engine, b"").unwrap();
        assert_eq!(
            resolve_engine(Some("  ".into()), Some(exe)),
            Some(EngineCommand {
                program: engine,
                leading_args: Vec::new(),
            })
        );
    }

    #[test]
    fn titles_name_the_start_time_and_never_reuse_a_note() {
        let dir = tempfile::tempdir().unwrap();
        let started = chrono::Local
            .with_ymd_and_hms(2026, 11, 25, 9, 30, 12)
            .unwrap();
        assert_eq!(unique_title(dir.path(), started), "Meeting 2026-11-25 0930");
        std::fs::write(dir.path().join("Meeting 2026-11-25 0930.md"), "").unwrap();
        assert_eq!(
            unique_title(dir.path(), started),
            "Meeting 2026-11-25 0930 2"
        );
    }

    #[test]
    fn capture_uses_the_google_sink_and_ends_with_the_meeting() {
        let args = capture_args(
            Path::new("/notes"),
            "Meeting.md",
            Path::new("/app/shorthand"),
            &["--backend".into(), "cursor".into()],
            None,
            false,
        );
        let joined = args.join(" ");
        assert!(joined.starts_with("capture --vault /notes --note Meeting.md --enhance"));
        assert!(joined.contains("--sink google --shorthand"));
        assert!(joined.contains("--shorthand /app/shorthand --backend cursor"));
        assert!(joined.ends_with("--exit-after-session"));
        assert!(!joined.contains("--google-document"));
        assert!(!joined.contains("--google-transcript"));
    }

    #[test]
    fn a_doc_per_meeting_hands_core_that_doc() {
        let args = capture_args(
            Path::new("/notes"),
            "Meeting.md",
            Path::new("/app/shorthand"),
            &[],
            Some("meeting-doc"),
            false,
        );
        assert!(args
            .join(" ")
            .contains("--sink google --google-document=meeting-doc --shorthand"));
    }

    #[test]
    fn saving_transcripts_puts_the_transcript_in_the_doc_too() {
        let args = capture_args(
            Path::new("/notes"),
            "Meeting.md",
            Path::new("/app/shorthand"),
            &[],
            Some("meeting-doc"),
            true,
        );
        assert!(args
            .join(" ")
            .contains("--google-document=meeting-doc --google-transcript --shorthand"));
    }

    #[test]
    fn failure_message_prefers_the_engines_last_words() {
        let tail = vec![
            "first".to_string(),
            "Shorthand is not running.".into(),
            " ".into(),
        ];
        assert_eq!(failure_message(&tail, Some(1)), "Shorthand is not running.");
        assert_eq!(
            failure_message(&[], Some(3)),
            "The notes engine stopped with exit code 3."
        );
    }

    #[test]
    fn last_run_serializes_with_a_kind_tag() {
        let run = LastRun::Failed {
            title: "Meeting".into(),
            message: "boom".into(),
        };
        assert_eq!(
            serde_json::to_value(run).unwrap(),
            serde_json::json!({"kind": "failed", "title": "Meeting", "message": "boom"})
        );
    }
}
