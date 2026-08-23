import { useSettings } from "@/hooks/useSettings";

/**
 * Whether advanced settings are revealed, and how to toggle that.
 *
 * Backed by the existing `show_all_settings` field, so this needs no Rust
 * change — but its *meaning* has changed. It used to swap the fork's simplified
 * sidebar for upstream's full one: two section trees that shared no vocabulary,
 * so turning the hatch on felt like being moved to a different application.
 *
 * It now reveals additional rows and groups in place. Same sections, same
 * order, more of the page you were already looking at. Nothing moves.
 */
export function useAdvanced(): {
  advanced: boolean;
  setAdvanced: (next: boolean) => void;
  isUpdating: boolean;
} {
  const { getSetting, updateSetting, isUpdating } = useSettings();

  return {
    advanced: getSetting("show_all_settings") ?? false,
    setAdvanced: (next) => {
      void updateSetting("show_all_settings", next);
    },
    isUpdating: isUpdating("show_all_settings"),
  };
}
