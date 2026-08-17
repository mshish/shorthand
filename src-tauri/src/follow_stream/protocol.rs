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

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum FollowEvent {
    Hello {
        protocol: u32,
        version: String,
    },
    Begin {
        session: u64,
        streaming: bool,
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
                },
                None,
                "{\"t\":\"hello\",\"protocol\":1,\"version\":\"0.9.5\",\"emitted_at\":\"2026-08-15T14:03:21.412-07:00\"}\n",
            ),
            (
                FollowEvent::Begin {
                    session: 1,
                    streaming: true,
                },
                Some(0),
                "{\"t\":\"begin\",\"session\":1,\"streaming\":true,\"emitted_at\":\"2026-08-15T14:03:21.412-07:00\",\"session_elapsed_ms\":0}\n",
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
