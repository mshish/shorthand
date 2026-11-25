//! Which note taker fills the Google Doc: the same five choices the Obsidian
//! plugin offers, passed to shorthand-core's `capture` as `--backend` and
//! that backend's own flags. Core does the work; this only decides the
//! flags, and refuses before a meeting starts when a choice is missing
//! something it can't run without.
//!
//! The API key choice never hands core a key. Core asks this app to make the
//! request, and the app attaches the key from the keyring slot named by the
//! provider and the endpoint's origin, the same slot the Obsidian plugin
//! saves to, so a key added in either place works in both.

use serde::{Deserialize, Serialize};
use specta::Type;
use url::Url;

use crate::shorthand::credentials::{canonical_origin, CredentialSlot, LlmProvider};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default, Type)]
#[serde(rename_all = "snake_case")]
pub enum NotesAgentBackend {
    #[default]
    Claude,
    Codex,
    Cursor,
    Acp,
    Llm,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default, Type)]
#[serde(rename_all = "kebab-case")]
pub enum NotesLlmProvider {
    #[default]
    Openai,
    Anthropic,
    Ollama,
    OpenaiCompatible,
}

impl From<NotesLlmProvider> for LlmProvider {
    fn from(provider: NotesLlmProvider) -> Self {
        match provider {
            NotesLlmProvider::Openai => LlmProvider::Openai,
            NotesLlmProvider::Anthropic => LlmProvider::Anthropic,
            NotesLlmProvider::Ollama => LlmProvider::Ollama,
            NotesLlmProvider::OpenaiCompatible => LlmProvider::OpenaiCompatible,
        }
    }
}

/// Each backend keeps its own model, as in the plugin, so switching from
/// Codex to Cursor doesn't carry a Codex model name across. Blank means the
/// tool's own default, except for the API key, which has no default.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default, Type)]
#[serde(default)]
pub struct NotesAgent {
    pub backend: NotesAgentBackend,
    pub codex_model: String,
    pub cursor_model: String,
    /// The program for "Another app": any ACP agent over standard I/O.
    pub acp_program: String,
    /// Split on whitespace by core, as the plugin's arguments field is.
    pub acp_args: String,
    pub acp_model: String,
    pub llm_provider: NotesLlmProvider,
    pub llm_model: String,
    /// Blank uses the provider's own endpoint. Required for
    /// `openai-compatible`, which has none.
    pub llm_base_url: String,
}

/// Core's defaults (`llm-client.ts`). The slot origin has to match the one
/// core computes, or every request is refused as an origin mismatch.
fn default_endpoint(provider: NotesLlmProvider) -> Option<&'static str> {
    match provider {
        NotesLlmProvider::Openai => Some("https://api.openai.com"),
        NotesLlmProvider::Anthropic => Some("https://api.anthropic.com"),
        NotesLlmProvider::Ollama => Some("http://127.0.0.1:11434"),
        NotesLlmProvider::OpenaiCompatible => None,
    }
}

fn trimmed(value: &str) -> Option<&str> {
    Some(value.trim()).filter(|v| !v.is_empty())
}

impl NotesAgent {
    /// The keyring slot the API key lives in, or why there isn't one yet.
    pub fn llm_slot(&self) -> Result<CredentialSlot, String> {
        let endpoint = match trimmed(&self.llm_base_url) {
            Some(url) => url,
            None => default_endpoint(self.llm_provider)
                .ok_or("An OpenAI-compatible provider needs its endpoint URL.")?,
        };
        let url = Url::parse(endpoint).map_err(|_| format!("\"{endpoint}\" isn't a URL."))?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(format!("\"{endpoint}\" needs to start with http or https."));
        }
        let origin =
            canonical_origin(&url.origin().ascii_serialization()).map_err(|e| e.to_string())?;
        Ok(CredentialSlot::NotesLlm {
            provider: self.llm_provider.into(),
            origin,
        })
    }

    /// The flags `capture` gets after its own. An error is a sentence for
    /// the user, shown on the meeting that couldn't start.
    pub fn capture_args(&self) -> Result<Vec<String>, String> {
        let mut args = vec!["--backend".to_string()];
        let mut flag = |name: &str, value: Option<&str>| {
            if let Some(value) = value {
                args.push(format!("--{name}={value}"));
            }
        };
        match self.backend {
            NotesAgentBackend::Claude => Ok(vec!["--backend".into(), "claude".into()]),
            NotesAgentBackend::Codex => {
                flag("codex-model", trimmed(&self.codex_model));
                Ok(prepend(args, "codex"))
            }
            NotesAgentBackend::Cursor => {
                flag("cursor-model", trimmed(&self.cursor_model));
                Ok(prepend(args, "cursor"))
            }
            NotesAgentBackend::Acp => {
                let program = trimmed(&self.acp_program)
                    .ok_or("Choose the program for your other app in Settings, Notes.")?;
                flag("acp-exe", Some(program));
                flag("acp-args", trimmed(&self.acp_args));
                flag("acp-model", trimmed(&self.acp_model));
                Ok(prepend(args, "acp"))
            }
            NotesAgentBackend::Llm => {
                let model = trimmed(&self.llm_model)
                    .ok_or("Choose a model for your API key in Settings, Notes.")?;
                self.llm_slot()?;
                let provider = LlmProvider::from(self.llm_provider).as_str();
                flag("llm-provider", Some(provider));
                flag("llm-model", Some(model));
                flag("llm-base-url", trimmed(&self.llm_base_url));
                Ok(prepend(args, "llm"))
            }
        }
    }
}

/// `args` is `["--backend", ...flags]`; the backend name goes between.
fn prepend(mut args: Vec<String>, backend: &str) -> Vec<String> {
    args.insert(1, backend.to_string());
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(backend: NotesAgentBackend) -> NotesAgent {
        NotesAgent {
            backend,
            ..Default::default()
        }
    }

    #[test]
    fn claude_is_the_default_and_takes_no_other_flags() {
        assert_eq!(NotesAgent::default().backend, NotesAgentBackend::Claude);
        assert_eq!(
            NotesAgent::default().capture_args().unwrap(),
            vec!["--backend", "claude"]
        );
    }

    #[test]
    fn codex_and_cursor_pass_a_model_only_when_one_is_set() {
        assert_eq!(
            agent(NotesAgentBackend::Codex).capture_args().unwrap(),
            vec!["--backend", "codex"]
        );
        let cursor = NotesAgent {
            cursor_model: " gpt-5 ".into(),
            ..agent(NotesAgentBackend::Cursor)
        };
        assert_eq!(
            cursor.capture_args().unwrap(),
            vec!["--backend", "cursor", "--cursor-model=gpt-5"]
        );
    }

    #[test]
    fn another_app_needs_a_program_and_keeps_its_arguments_in_one_value() {
        assert!(agent(NotesAgentBackend::Acp)
            .capture_args()
            .unwrap_err()
            .contains("Choose the program"));
        let acp = NotesAgent {
            acp_program: "/usr/local/bin/gemini".into(),
            acp_args: "--experimental-acp --quiet".into(),
            ..agent(NotesAgentBackend::Acp)
        };
        assert_eq!(
            acp.capture_args().unwrap(),
            vec![
                "--backend",
                "acp",
                "--acp-exe=/usr/local/bin/gemini",
                "--acp-args=--experimental-acp --quiet",
            ]
        );
    }

    #[test]
    fn api_key_needs_a_model_and_an_endpoint_it_can_name() {
        assert!(agent(NotesAgentBackend::Llm)
            .capture_args()
            .unwrap_err()
            .contains("Choose a model"));
        let compatible = NotesAgent {
            llm_provider: NotesLlmProvider::OpenaiCompatible,
            llm_model: "local".into(),
            ..agent(NotesAgentBackend::Llm)
        };
        assert!(compatible
            .capture_args()
            .unwrap_err()
            .contains("needs its endpoint URL"));
        let anthropic = NotesAgent {
            llm_provider: NotesLlmProvider::Anthropic,
            llm_model: "claude-sonnet-4-5".into(),
            ..agent(NotesAgentBackend::Llm)
        };
        assert_eq!(
            anthropic.capture_args().unwrap(),
            vec![
                "--backend",
                "llm",
                "--llm-provider=anthropic",
                "--llm-model=claude-sonnet-4-5",
            ]
        );
    }

    #[test]
    fn the_key_slot_matches_the_origin_core_computes() {
        let slot = |provider, base_url: &str| {
            NotesAgent {
                llm_provider: provider,
                llm_base_url: base_url.into(),
                ..Default::default()
            }
            .llm_slot()
        };
        assert_eq!(
            slot(NotesLlmProvider::Openai, "").unwrap().service(),
            "notes-llm/openai/https://api.openai.com"
        );
        assert_eq!(
            slot(NotesLlmProvider::Ollama, "").unwrap().service(),
            "notes-llm/ollama/http://127.0.0.1:11434"
        );
        assert_eq!(
            slot(
                NotesLlmProvider::OpenaiCompatible,
                " HTTP://LocalHost:1234/v1 "
            )
            .unwrap()
            .service(),
            "notes-llm/openai-compatible/http://localhost:1234"
        );
        assert!(slot(NotesLlmProvider::Openai, "file:///models").is_err());
        assert!(slot(NotesLlmProvider::Openai, "not a url").is_err());
    }

    #[test]
    fn a_partly_written_setting_fills_in_the_rest() {
        let parsed: NotesAgent = serde_json::from_str(r#"{"backend":"cursor"}"#).unwrap();
        assert_eq!(parsed, agent(NotesAgentBackend::Cursor));
        let provider: NotesAgent =
            serde_json::from_str(r#"{"llm_provider":"openai-compatible"}"#).unwrap();
        assert_eq!(provider.llm_provider, NotesLlmProvider::OpenaiCompatible);
    }
}
