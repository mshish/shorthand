//! Decision logic for the explicit `--start-assisted-notes` /
//! `--stop-assisted-notes` CLI commands.
//!
//! These replace `--toggle-assisted-notes` for callers (the Obsidian plugin,
//! chiefly) that need a command safe to retry. A toggle flips state on every
//! delivery, so a retry sent because the first attempt's confirmation was
//! lost — the exact bug this module exists to close, see FOLLOW_STREAM.md's
//! "Level-triggered attachment" section — can stop the very capture it meant
//! to confirm. `decide` below is the one place that turns "start" or "stop"
//! plus the current capture state into an action, so that rule lives in one
//! pure, unit-tested function instead of being re-derived at each call site.
//!
//! `decide` alone does not make a command idempotent — that also requires
//! deciding and acting atomically against state that cannot go stale between
//! the two. `TranscriptionCoordinator` is what supplies that: it classifies
//! its own authoritative `Stage` into the `CaptureState` below and calls
//! `decide` from inside its single serialized command loop, so a second
//! explicit command can never race the first (see
//! `transcription_coordinator::decide_explicit_capture`). This module used to
//! be paired with a helper that read capture state from outside that thread
//! instead; that snapshot could go stale between being read and being acted
//! on, which is exactly the double-start/stop race this module exists to
//! prevent — so that helper was removed rather than kept as a second, unsound
//! path to the same decision.

use super::mode::Mode;

/// What is presently capturing. `Recording` and `Processing` are kept as
/// distinct variants rather than folded into one `Capturing(Mode)` — see
/// `decide`'s `Start` arm for why an explicit start observed during
/// `Processing` must always be refused, even for the mode already draining,
/// rather than treated the same as an active `Recording` of that mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureState {
    Idle,
    Recording(Mode),
    Processing(Mode),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplicitOp {
    Start,
    Stop,
}

/// Why an explicit command was declined. Its own type rather than
/// `follow_stream::RefusalReason` re-used directly: this module's decision
/// must stay free of the wire's serde/kebab-case concerns, the same reason
/// `shorthand::mode::Mode` keeps its own `From` mapping into
/// `follow_stream::FollowMode` instead of being serialized itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefusalReason {
    /// A different mode's capture is already running.
    Busy,
    /// The requested mode is switched off in Settings.
    ModeDisabled,
}

impl From<RefusalReason> for crate::follow_stream::RefusalReason {
    fn from(reason: RefusalReason) -> Self {
        match reason {
            RefusalReason::Busy => Self::Busy,
            RefusalReason::ModeDisabled => Self::ModeDisabled,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Start or stop the capture directly. Only returned when the observed
    /// state is exactly the edge the op asks for (idle -> start a capture,
    /// or this mode capturing -> stop it), so forwarding can never flip the
    /// wrong direction.
    Forward,
    /// The requested state already holds. Reporting success here rather than
    /// forwarding is what makes the command retry-safe: forwarding would
    /// flip a capture the retry only meant to confirm, which is the
    /// reproduced bug this command replaces `--toggle-assisted-notes` for.
    NoOp,
    Refuse(RefusalReason),
}

/// Decides what an explicit `op` for `mode` should do, given whether `mode`
/// is enabled in Settings and what is presently capturing. Pure and
/// exhaustively unit-tested below; see the module doc for why this must be
/// the only place that makes this decision.
pub fn decide(op: ExplicitOp, mode: Mode, mode_enabled: bool, capture: CaptureState) -> Decision {
    match op {
        ExplicitOp::Start => {
            if !mode_enabled {
                return Decision::Refuse(RefusalReason::ModeDisabled);
            }
            match capture {
                CaptureState::Idle => Decision::Forward,
                CaptureState::Recording(active) if active == mode => Decision::NoOp,
                CaptureState::Recording(_) => Decision::Refuse(RefusalReason::Busy),
                // The microphone for whatever was recording is already
                // closed here (see `capture_state_for_decision`'s doc
                // comment), including when `mode` is the one still
                // draining — so treating this the same as `Recording(mode)`
                // and returning `NoOp` would report success while starting
                // nothing: the pipeline then finishes on its own, `Stage`
                // resets to `Idle`, and no `begin`, `refused`, or any other
                // observable record is ever produced for this command.
                // Refusing as `Busy` instead makes the decline observable
                // immediately; the caller can retry once the pipeline
                // drains.
                CaptureState::Processing(_) => Decision::Refuse(RefusalReason::Busy),
            }
        }
        // Unlike Start, Stop is not gated on `mode_enabled`: if `mode` is
        // somehow capturing (e.g. it was disabled mid-capture), a stop must
        // still be able to end it rather than being refused for a setting
        // that no longer describes what is running.
        ExplicitOp::Stop => match capture {
            CaptureState::Recording(active) if active == mode => Decision::Forward,
            _ => Decision::NoOp,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TARGET: Mode = Mode::AssistedNotes;
    const OTHER_A: Mode = Mode::Meeting;
    const OTHER_B: Mode = Mode::Dictation;

    /// Every `(op, capture, mode_enabled)` combination the command can
    /// observe. `Recording`/`Processing` of `TARGET` and of each `OTHER_*`
    /// are all kept distinct because `decide` must tell them apart —
    /// `Processing(TARGET)` in particular must not collapse into
    /// `Recording(TARGET)`'s `NoOp` (see `decide`'s `Start` arm).
    fn every_capture_state() -> [CaptureState; 7] {
        [
            CaptureState::Idle,
            CaptureState::Recording(TARGET),
            CaptureState::Recording(OTHER_A),
            CaptureState::Recording(OTHER_B),
            CaptureState::Processing(TARGET),
            CaptureState::Processing(OTHER_A),
            CaptureState::Processing(OTHER_B),
        ]
    }

    #[test]
    fn start_while_idle_and_enabled_forwards() {
        assert_eq!(
            decide(ExplicitOp::Start, TARGET, true, CaptureState::Idle),
            Decision::Forward
        );
    }

    #[test]
    fn start_while_this_mode_is_already_recording_is_a_noop() {
        // The idempotency guarantee: a retried start must never toggle off
        // the capture it just confirmed.
        assert_eq!(
            decide(
                ExplicitOp::Start,
                TARGET,
                true,
                CaptureState::Recording(TARGET)
            ),
            Decision::NoOp
        );
    }

    #[test]
    fn start_while_a_different_mode_is_recording_is_refused_as_busy() {
        for other in [OTHER_A, OTHER_B] {
            assert_eq!(
                decide(
                    ExplicitOp::Start,
                    TARGET,
                    true,
                    CaptureState::Recording(other)
                ),
                Decision::Refuse(RefusalReason::Busy)
            );
        }
    }

    /// A start for the mode that is itself still draining must be refused,
    /// not treated like `Recording(TARGET)`'s `NoOp` — the mic is already
    /// closed during `Processing`, so a `NoOp` here would report success
    /// while starting nothing.
    #[test]
    fn start_while_this_mode_is_processing_is_refused_as_busy() {
        assert_eq!(
            decide(
                ExplicitOp::Start,
                TARGET,
                true,
                CaptureState::Processing(TARGET)
            ),
            Decision::Refuse(RefusalReason::Busy)
        );
    }

    #[test]
    fn start_while_a_different_mode_is_processing_is_refused_as_busy() {
        for other in [OTHER_A, OTHER_B] {
            assert_eq!(
                decide(
                    ExplicitOp::Start,
                    TARGET,
                    true,
                    CaptureState::Processing(other)
                ),
                Decision::Refuse(RefusalReason::Busy)
            );
        }
    }

    #[test]
    fn start_while_the_mode_is_disabled_is_refused_regardless_of_capture_state() {
        for capture in every_capture_state() {
            assert_eq!(
                decide(ExplicitOp::Start, TARGET, false, capture),
                Decision::Refuse(RefusalReason::ModeDisabled),
                "capture={capture:?}"
            );
        }
    }

    #[test]
    fn stop_while_this_mode_is_recording_forwards() {
        assert_eq!(
            decide(
                ExplicitOp::Stop,
                TARGET,
                true,
                CaptureState::Recording(TARGET)
            ),
            Decision::Forward
        );
        // A disabled mode can still be the one actually running (e.g. the
        // setting changed mid-capture); stop must still be able to end it.
        assert_eq!(
            decide(
                ExplicitOp::Stop,
                TARGET,
                false,
                CaptureState::Recording(TARGET)
            ),
            Decision::Forward
        );
    }

    #[test]
    fn stop_while_idle_is_a_noop() {
        for mode_enabled in [true, false] {
            assert_eq!(
                decide(ExplicitOp::Stop, TARGET, mode_enabled, CaptureState::Idle),
                Decision::NoOp
            );
        }
    }

    #[test]
    fn stop_while_a_different_mode_is_recording_is_a_noop() {
        // Stop never interrupts a capture it was not asked to stop, but this
        // is success (nothing for this mode to stop), not a refusal.
        for other in [OTHER_A, OTHER_B] {
            for mode_enabled in [true, false] {
                assert_eq!(
                    decide(
                        ExplicitOp::Stop,
                        TARGET,
                        mode_enabled,
                        CaptureState::Recording(other)
                    ),
                    Decision::NoOp,
                    "other={other:?} mode_enabled={mode_enabled}"
                );
            }
        }
    }

    /// A stop for the mode that is itself draining is a no-op, not a
    /// refusal: the capture already stopped (that is why it is
    /// `Processing`), so there is nothing left for this stop to do.
    #[test]
    fn stop_while_this_mode_is_processing_is_a_noop() {
        for mode_enabled in [true, false] {
            assert_eq!(
                decide(
                    ExplicitOp::Stop,
                    TARGET,
                    mode_enabled,
                    CaptureState::Processing(TARGET)
                ),
                Decision::NoOp,
                "mode_enabled={mode_enabled}"
            );
        }
    }

    #[test]
    fn stop_while_a_different_mode_is_processing_is_a_noop() {
        for other in [OTHER_A, OTHER_B] {
            for mode_enabled in [true, false] {
                assert_eq!(
                    decide(
                        ExplicitOp::Stop,
                        TARGET,
                        mode_enabled,
                        CaptureState::Processing(other)
                    ),
                    Decision::NoOp,
                    "other={other:?} mode_enabled={mode_enabled}"
                );
            }
        }
    }

    /// Exhaustive sweep: every op x every observable capture state x
    /// enabled/disabled, cross-checked against the targeted cases above so
    /// no combination is silently unmatched by `decide`'s own arms.
    #[test]
    fn every_combination_of_op_capture_and_enabled_is_decided() {
        for op in [ExplicitOp::Start, ExplicitOp::Stop] {
            for capture in every_capture_state() {
                for mode_enabled in [true, false] {
                    let decision = decide(op, TARGET, mode_enabled, capture);
                    let expected = match (op, capture, mode_enabled) {
                        (ExplicitOp::Start, _, false) => {
                            Decision::Refuse(RefusalReason::ModeDisabled)
                        }
                        (ExplicitOp::Start, CaptureState::Idle, true) => Decision::Forward,
                        (ExplicitOp::Start, CaptureState::Recording(m), true) if m == TARGET => {
                            Decision::NoOp
                        }
                        (ExplicitOp::Start, CaptureState::Recording(_), true) => {
                            Decision::Refuse(RefusalReason::Busy)
                        }
                        // Processing always refuses on Start, same mode or
                        // not -- see the targeted Processing tests above and
                        // `decide`'s own comment on this arm.
                        (ExplicitOp::Start, CaptureState::Processing(_), true) => {
                            Decision::Refuse(RefusalReason::Busy)
                        }
                        (ExplicitOp::Stop, CaptureState::Recording(m), _) if m == TARGET => {
                            Decision::Forward
                        }
                        (ExplicitOp::Stop, _, _) => Decision::NoOp,
                    };
                    assert_eq!(
                        decision, expected,
                        "op={op:?} capture={capture:?} mode_enabled={mode_enabled}"
                    );
                }
            }
        }
    }
}
