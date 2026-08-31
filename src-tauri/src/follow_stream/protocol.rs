use std::sync::Arc;

use chrono::{DateTime, FixedOffset, SecondsFormat};
use serde::Serialize;

use crate::managers::transcription::StreamSource;

/// Additive only. `emitted_at` and `session_elapsed_ms` were introduced without
/// a bump because the documented contract already tells consumers to ignore
/// fields they do not recognize. Reserve a bump for a removal, a rename, or a
/// changed event meaning.
pub const FOLLOW_PROTOCOL_VERSION: u32 = 1;
pub const ERR_DISABLED: &str = "disabled";
pub const ERR_FOLLOWER_LIMIT: &str = "follower_limit";
pub const ERR_SERIALIZATION_FAILED: &str = "serialization_failed";

/// Which capture lane produced an event. Wire values are "me" (microphone) and
/// "them" (Windows system-audio loopback).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Speaker {
    Me,
    Them,
}

impl Speaker {
    pub fn index(self) -> usize {
        match self {
            Self::Me => 0,
            Self::Them => 1,
        }
    }
}

impl From<StreamSource> for Speaker {
    fn from(source: StreamSource) -> Self {
        match source {
            StreamSource::Mic => Self::Me,
            StreamSource::System => Self::Them,
        }
    }
}

/// Why the app declined an explicit `--start-assisted-notes` /
/// `--stop-assisted-notes` command. Carried on [`FollowEvent::Refused`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RefusalReason {
    /// A different mode's capture is already running. An explicit command
    /// never interrupts a capture it did not ask to start.
    Busy,
    /// The requested mode is switched off in Settings.
    ModeDisabled,
}

/// Which capture mode produced a session, as it appears on the wire.
///
/// Deliberately its own type rather than `shorthand::mode::Mode` re-serialized:
/// this is a wire contract a follower gates behaviour on, and it must not
/// change spelling because someone renamed an internal variant. The mapping
/// between the two lives in `shorthand::mode`, so this module stays ignorant of
/// the mode cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FollowMode {
    Meeting,
    AssistedNotes,
    Dictation,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum FollowEvent {
    Hello {
        protocol: u32,
        version: String,
        /// Optional protocol capabilities this binary supports, as kebab-case
        /// names. Control flags appear here as the CLI flag minus its `--`
        /// (e.g. `"toggle-assisted-notes"`); other capabilities name a feature
        /// of the wire format (e.g. `"begin-mode"`, meaning `begin` records
        /// carry a `mode`). It advertises what this binary can do, never
        /// whether a mode is currently enabled — a follower still gets the
        /// app's own settings pane as the single description of behaviour.
        /// This exists so a follower can tell an installed binary that
        /// predates a capability from one that merely has the corresponding
        /// setting turned off, instead of guessing from a version number.
        /// Additive under protocol 1: an older follower ignores a field it
        /// does not recognize.
        capabilities: Vec<&'static str>,
    },
    Begin {
        session: u64,
        streaming: bool,
        /// Additive under protocol 1. An older follower ignores it; a current
        /// one uses it to decide whether a session is any of its business at
        /// all. Advertised by the `begin-mode` capability on `hello`, because
        /// "field absent" and "app predates the field" are the same bytes and a
        /// follower must not guess between them from a version number.
        mode: FollowMode,
    },
    Partial {
        session: u64,
        speaker: Speaker,
        committed: String,
        tentative: String,
    },
    Final {
        session: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        speaker: Option<Speaker>,
        text: String,
    },
    NoSpeech {
        session: u64,
    },
    Cancel {
        session: u64,
    },
    Error {
        #[serde(skip_serializing_if = "Option::is_none")]
        session: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        code: Option<&'static str>,
        message: String,
    },
    /// Emitted right after `hello` when subscribing finds no active session.
    /// Idle was previously only inferable from the absence of a `begin`,
    /// which a follower cannot tell apart from "attached before anything has
    /// started yet". This makes idle a state a follower can read directly
    /// instead of a guess from silence.
    Idle,
    /// A capture request was accepted but never produced a `begin`, because
    /// `begin` now fires only once `try_start_recording` has actually
    /// succeeded (see `TranscribeAction::start`). Session-less: no session
    /// was ever announced for this attempt, so there is nothing for a
    /// terminal event to close, and a follower that got `Response::Accepted`
    /// on its command would otherwise wait forever for a `begin` that is
    /// never coming. Deliberately its own record rather than a session-less
    /// `error`: every other session-less `error` on this wire carries a
    /// `code` (`follower_limit`, `disabled`, `serialization_failed`), so
    /// reusing that shape here would give a follower two things to
    /// distinguish by the *absence* of a field instead of one to match on.
    ///
    /// Reaches a follower only when the failed mode's own
    /// `follow_stream_enabled` is on — the same publication gate `begin`
    /// respects — and carries `mode` for the same reason `begin` does:
    /// without it a follower watching one mode could misattribute a
    /// different mode's failure to itself.
    StartFailed {
        mode: FollowMode,
        message: String,
    },
    /// The app declined an explicit `--start-assisted-notes` /
    /// `--stop-assisted-notes` command. Carries no request id: this protocol
    /// is one-way with no request/response correlation, so a follower can
    /// only relate this to a command it just issued for the same `mode` by
    /// having attached first and read current state before issuing it (see
    /// "Level-triggered attachment" in FOLLOW_STREAM.md).
    Refused {
        mode: FollowMode,
        reason: RefusalReason,
    },
}

/// When an event was produced, merged into every emitted line.
///
/// `emitted_at` is civil time for display and log correlation; `session_elapsed_ms`
/// is the monotonic ordering key, because wall clocks move backward across NTP
/// corrections, DST transitions, and suspend/resume. It is absent on events that
/// belong to no session (`hello`, connection-level `error`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stamp {
    pub emitted_at: String,
    pub session_elapsed_ms: Option<u64>,
}

impl Stamp {
    pub fn new(wall: DateTime<FixedOffset>, session_elapsed_ms: Option<u64>) -> Self {
        Self {
            // Millisecond precision with a numeric UTC offset, never `Z`, so the
            // reader always sees the offset the capture machine was running at.
            emitted_at: wall.to_rfc3339_opts(SecondsFormat::Millis, false),
            session_elapsed_ms,
        }
    }
}

/// One wire line: the event's own fields, then the stamp. `flatten` writes the
/// event's entries into this map in declaration order, so `t` stays first.
#[derive(Serialize)]
struct StampedEvent<'a> {
    #[serde(flatten)]
    event: &'a FollowEvent,
    emitted_at: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_elapsed_ms: Option<u64>,
}

impl FollowEvent {
    pub fn to_line(&self, stamp: &Stamp) -> Arc<str> {
        let stamped = StampedEvent {
            event: self,
            emitted_at: &stamp.emitted_at,
            session_elapsed_ms: stamp.session_elapsed_ms,
        };
        match serde_json::to_string(&stamped) {
            Ok(mut json) => {
                json.push('\n');
                Arc::from(json)
            }
            Err(error) => {
                log::error!("Failed to serialize follow-stream event: {error}");
                // Only the event payload can fail here; the stamp is our own
                // RFC3339 text and needs no escaping.
                Arc::from(format!(
                    "{{\"t\":\"error\",\"code\":\"{ERR_SERIALIZATION_FAILED}\",\"message\":\"serialization failed\",\"emitted_at\":\"{}\"}}\n",
                    stamp.emitted_at
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every stamped case below shares this instant so the expected bytes stay
    /// readable; `TS` is what `Stamp::new` renders it to.
    const TS: &str = "2026-08-15T14:03:21.412-07:00";

    fn stamp(session_elapsed_ms: Option<u64>) -> Stamp {
        Stamp {
            emitted_at: TS.to_string(),
            session_elapsed_ms,
        }
    }

    #[test]
    fn every_event_variant_has_the_exact_wire_format() {
        let cases = [
            (
                FollowEvent::Hello {
                    protocol: FOLLOW_PROTOCOL_VERSION,
                    version: "0.9.5".to_string(),
                    capabilities: vec!["toggle-assisted-notes", "begin-mode"],
                },
                None,
                "{\"t\":\"hello\",\"protocol\":1,\"version\":\"0.9.5\",\"capabilities\":[\"toggle-assisted-notes\",\"begin-mode\"],\"emitted_at\":\"2026-08-15T14:03:21.412-07:00\"}\n",
            ),
            (
                FollowEvent::Begin {
                    session: 1,
                    streaming: true,
                    mode: FollowMode::Meeting,
                },
                Some(0),
                "{\"t\":\"begin\",\"session\":1,\"streaming\":true,\"mode\":\"meeting\",\"emitted_at\":\"2026-08-15T14:03:21.412-07:00\",\"session_elapsed_ms\":0}\n",
            ),
            (
                FollowEvent::Partial {
                    session: 2,
                    speaker: Speaker::Me,
                    committed: "hello ".to_string(),
                    tentative: "wor".to_string(),
                },
                Some(1212),
                "{\"t\":\"partial\",\"session\":2,\"speaker\":\"me\",\"committed\":\"hello \",\"tentative\":\"wor\",\"emitted_at\":\"2026-08-15T14:03:21.412-07:00\",\"session_elapsed_ms\":1212}\n",
            ),
            (
                FollowEvent::Final {
                    session: 3,
                    speaker: Some(Speaker::Them),
                    text: "Done.".to_string(),
                },
                Some(1850),
                "{\"t\":\"final\",\"session\":3,\"speaker\":\"them\",\"text\":\"Done.\",\"emitted_at\":\"2026-08-15T14:03:21.412-07:00\",\"session_elapsed_ms\":1850}\n",
            ),
            (
                FollowEvent::Final {
                    session: 4,
                    speaker: None,
                    text: "Me: Hi\nThem: Hello".to_string(),
                },
                Some(1850),
                "{\"t\":\"final\",\"session\":4,\"text\":\"Me: Hi\\nThem: Hello\",\"emitted_at\":\"2026-08-15T14:03:21.412-07:00\",\"session_elapsed_ms\":1850}\n",
            ),
            (
                FollowEvent::NoSpeech { session: 5 },
                Some(700),
                "{\"t\":\"no_speech\",\"session\":5,\"emitted_at\":\"2026-08-15T14:03:21.412-07:00\",\"session_elapsed_ms\":700}\n",
            ),
            (
                FollowEvent::Cancel { session: 6 },
                Some(700),
                "{\"t\":\"cancel\",\"session\":6,\"emitted_at\":\"2026-08-15T14:03:21.412-07:00\",\"session_elapsed_ms\":700}\n",
            ),
            (
                FollowEvent::Error {
                    session: Some(7),
                    code: None,
                    message: "transcription failed".to_string(),
                },
                Some(900),
                "{\"t\":\"error\",\"session\":7,\"message\":\"transcription failed\",\"emitted_at\":\"2026-08-15T14:03:21.412-07:00\",\"session_elapsed_ms\":900}\n",
            ),
            (
                FollowEvent::Error {
                    session: None,
                    code: Some(ERR_FOLLOWER_LIMIT),
                    message: "too many followers".to_string(),
                },
                None,
                "{\"t\":\"error\",\"code\":\"follower_limit\",\"message\":\"too many followers\",\"emitted_at\":\"2026-08-15T14:03:21.412-07:00\"}\n",
            ),
            (
                FollowEvent::Error {
                    session: None,
                    code: Some(ERR_SERIALIZATION_FAILED),
                    message: "serialization failed".to_string(),
                },
                None,
                "{\"t\":\"error\",\"code\":\"serialization_failed\",\"message\":\"serialization failed\",\"emitted_at\":\"2026-08-15T14:03:21.412-07:00\"}\n",
            ),
            (
                FollowEvent::Idle,
                None,
                "{\"t\":\"idle\",\"emitted_at\":\"2026-08-15T14:03:21.412-07:00\"}\n",
            ),
            (
                FollowEvent::StartFailed {
                    mode: FollowMode::AssistedNotes,
                    message: "no input device".to_string(),
                },
                None,
                "{\"t\":\"start_failed\",\"mode\":\"assisted-notes\",\"message\":\"no input device\",\"emitted_at\":\"2026-08-15T14:03:21.412-07:00\"}\n",
            ),
            (
                FollowEvent::Refused {
                    mode: FollowMode::AssistedNotes,
                    reason: RefusalReason::ModeDisabled,
                },
                None,
                "{\"t\":\"refused\",\"mode\":\"assisted-notes\",\"reason\":\"mode-disabled\",\"emitted_at\":\"2026-08-15T14:03:21.412-07:00\"}\n",
            ),
        ];

        for (event, session_elapsed_ms, expected) in cases {
            assert_eq!(&*event.to_line(&stamp(session_elapsed_ms)), expected);
        }
    }

    #[test]
    fn stamp_renders_rfc3339_millis_with_a_numeric_offset() {
        let wall = DateTime::parse_from_rfc3339("2026-08-15T14:03:21.412345-07:00").unwrap();
        let stamp = Stamp::new(wall, Some(1212));

        // Sub-millisecond precision is truncated, and the offset is preserved as
        // written rather than normalized to UTC.
        assert_eq!(stamp.emitted_at, TS);
        assert_eq!(stamp.session_elapsed_ms, Some(1212));
    }

    #[test]
    fn stamp_never_renders_a_zulu_offset() {
        // `Z` would drop the offset the user asked to see, so UTC must still
        // render as `+00:00`.
        let wall = DateTime::parse_from_rfc3339("2026-08-15T21:03:21.412Z").unwrap();

        assert_eq!(
            Stamp::new(wall, None).emitted_at,
            "2026-08-15T21:03:21.412+00:00"
        );
    }

    #[test]
    fn begin_names_the_capture_mode_in_kebab_case() {
        let stamp = Stamp::new(
            DateTime::parse_from_rfc3339("2026-08-15T14:03:21.412-07:00").unwrap(),
            Some(0),
        );
        // The wire spelling is the contract a follower gates on, so it is asserted
        // literally rather than round-tripped through serde.
        for (mode, expected) in [
            (FollowMode::Meeting, "meeting"),
            (FollowMode::AssistedNotes, "assisted-notes"),
            (FollowMode::Dictation, "dictation"),
        ] {
            let line = FollowEvent::Begin {
                session: 1,
                streaming: true,
                mode,
            }
            .to_line(&stamp);
            assert_eq!(
                &*line,
                format!(
                    "{{\"t\":\"begin\",\"session\":1,\"streaming\":true,\"mode\":\"{expected}\",\"emitted_at\":\"2026-08-15T14:03:21.412-07:00\",\"session_elapsed_ms\":0}}\n"
                )
            );
        }
    }

    #[test]
    fn refusal_reason_serializes_to_kebab_case() {
        assert_eq!(
            serde_json::to_string(&RefusalReason::Busy).unwrap(),
            "\"busy\""
        );
        assert_eq!(
            serde_json::to_string(&RefusalReason::ModeDisabled).unwrap(),
            "\"mode-disabled\""
        );
    }

    #[test]
    fn speaker_maps_capture_lanes_and_serializes_to_wire_values() {
        let me = Speaker::from(StreamSource::Mic);
        let them = Speaker::from(StreamSource::System);

        assert_eq!(me, Speaker::Me);
        assert_eq!(them, Speaker::Them);
        assert_eq!(me.index(), 0);
        assert_eq!(them.index(), 1);
        assert_eq!(serde_json::to_string(&me).unwrap(), "\"me\"");
        assert_eq!(serde_json::to_string(&them).unwrap(), "\"them\"");
    }
}
