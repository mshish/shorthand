use std::sync::Arc;

use serde::Serialize;

use crate::managers::transcription::StreamSource;

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

impl FollowEvent {
    pub fn to_line(&self) -> Arc<str> {
        match serde_json::to_string(self) {
            Ok(mut json) => {
                json.push('\n');
                Arc::from(json)
            }
            Err(error) => {
                log::error!("Failed to serialize follow-stream event: {error}");
                Arc::from(
                    "{\"t\":\"error\",\"code\":\"serialization_failed\",\"message\":\"serialization failed\"}\n",
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_event_variant_has_the_exact_wire_format() {
        let cases = [
            (
                FollowEvent::Hello {
                    protocol: FOLLOW_PROTOCOL_VERSION,
                    version: "0.9.5".to_string(),
                },
                "{\"t\":\"hello\",\"protocol\":1,\"version\":\"0.9.5\"}\n",
            ),
            (
                FollowEvent::Begin {
                    session: 1,
                    streaming: true,
                },
                "{\"t\":\"begin\",\"session\":1,\"streaming\":true}\n",
            ),
            (
                FollowEvent::Partial {
                    session: 2,
                    speaker: Speaker::Me,
                    committed: "hello ".to_string(),
                    tentative: "wor".to_string(),
                },
                "{\"t\":\"partial\",\"session\":2,\"speaker\":\"me\",\"committed\":\"hello \",\"tentative\":\"wor\"}\n",
            ),
            (
                FollowEvent::Final {
                    session: 3,
                    speaker: Some(Speaker::Them),
                    text: "Done.".to_string(),
                },
                "{\"t\":\"final\",\"session\":3,\"speaker\":\"them\",\"text\":\"Done.\"}\n",
            ),
            (
                FollowEvent::Final {
                    session: 4,
                    speaker: None,
                    text: "Me: Hi\nThem: Hello".to_string(),
                },
                "{\"t\":\"final\",\"session\":4,\"text\":\"Me: Hi\\nThem: Hello\"}\n",
            ),
            (
                FollowEvent::NoSpeech { session: 5 },
                "{\"t\":\"no_speech\",\"session\":5}\n",
            ),
            (
                FollowEvent::Cancel { session: 6 },
                "{\"t\":\"cancel\",\"session\":6}\n",
            ),
            (
                FollowEvent::Error {
                    session: Some(7),
                    code: None,
                    message: "transcription failed".to_string(),
                },
                "{\"t\":\"error\",\"session\":7,\"message\":\"transcription failed\"}\n",
            ),
            (
                FollowEvent::Error {
                    session: None,
                    code: Some(ERR_FOLLOWER_LIMIT),
                    message: "too many followers".to_string(),
                },
                "{\"t\":\"error\",\"code\":\"follower_limit\",\"message\":\"too many followers\"}\n",
            ),
            (
                FollowEvent::Error {
                    session: None,
                    code: Some(ERR_SERIALIZATION_FAILED),
                    message: "serialization failed".to_string(),
                },
                "{\"t\":\"error\",\"code\":\"serialization_failed\",\"message\":\"serialization failed\"}\n",
            ),
        ];

        for (event, expected) in cases {
            assert_eq!(&*event.to_line(), expected);
        }
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
