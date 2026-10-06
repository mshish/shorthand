// Copy keys for the overlay's error state. Kinds are fixed codes from
// `shorthand::overlay_error` in the backend.
const KEYS: Record<string, string[]> = {
  mic_permission_windows: [
    "overlay.error.micPermission.message",
    "overlay.error.micPermission.windows",
  ],
  mic_permission_macos: [
    "overlay.error.micPermission.message",
    "overlay.error.micPermission.macos",
  ],
  mic_permission: [
    "overlay.error.micPermission.message",
    "overlay.error.micPermission.other",
  ],
  mic_missing: ["overlay.error.micMissing"],
  mic_failed: [
    "overlay.error.micFailed.message",
    "overlay.error.micFailed.action",
  ],
  not_ready: [
    "overlay.error.notReady.message",
    "overlay.error.notReady.action",
  ],
  busy: ["overlay.error.busy"],
  transcribe_failed: ["overlay.error.transcribeFailed"],
};

/** Every kind the backend can send. */
export const OVERLAY_ERROR_KINDS = Object.keys(KEYS);

export function overlayErrorKeys(kind: string, saved: boolean): string[] {
  const keys = KEYS[kind] ?? ["overlay.error.generic"];
  return saved ? [...keys, "overlay.error.savedInHistory"] : keys;
}
