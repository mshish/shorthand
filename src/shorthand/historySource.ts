/**
 * The fork-only History badge for an entry's `source`.
 *
 * `source` is what the entry was made by: a capture's mode, or `file` for a
 * dropped audio or video file. A value this version doesn't know (a row from
 * a newer one) reads as Meeting, the column's default.
 */
export function historySourceLabelKey(source: string): string {
  switch (source) {
    case "dictation":
      return "settings.history.source.dictation";
    case "assisted_notes":
      return "settings.history.source.assistedNotes";
    case "file":
      return "settings.history.source.file";
    default:
      return "settings.history.source.meeting";
  }
}
