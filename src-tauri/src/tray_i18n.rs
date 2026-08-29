//! Backend internationalization for tray menus and labelled transcripts.
//!
//! Everything is auto-generated at compile time by build.rs from the
//! frontend locale files (src/i18n/locales/*/translation.json).
//!
//! The English translation.json is the single source of truth:
//! - TrayStrings and TranscriptStrings are derived from their English sections
//! - All languages are auto-discovered from the locales directory
//!
//! To add a new backend-consumed string:
//! 1. Add the key to en/translation.json under "tray" or "transcript"
//! 2. Add translations to other locale files
//! 3. Use the matching generated translation accessor below.

use once_cell::sync::Lazy;
use std::collections::HashMap;
use tauri::AppHandle;

// Include the auto-generated TrayStrings struct and TRANSLATIONS static
include!(concat!(env!("OUT_DIR"), "/tray_translations.rs"));

/// Get localized tray menu strings based on the system locale.
///
/// Lookup order: exact locale → Chinese script/region fallback → language code → English.
pub fn get_tray_translations(locale: Option<String>) -> TrayStrings {
    resolve_translations(&TRANSLATIONS, locale)
}

pub fn get_transcript_translations(locale: Option<String>) -> TranscriptStrings {
    resolve_translations(&TRANSCRIPT_TRANSLATIONS, locale)
}

pub fn get_app_transcript_translations(app: &AppHandle) -> TranscriptStrings {
    let configured = crate::settings::get_settings(app).app_language;
    let locale =
        if configured.eq_ignore_ascii_case("auto") || configured.eq_ignore_ascii_case("system") {
            tauri_plugin_os::locale()
        } else {
            Some(configured)
        };
    get_transcript_translations(locale)
}

pub fn merged_transcript_retry_error_for_app(app: &AppHandle, text: &str) -> Option<String> {
    let labels = get_app_transcript_translations(app);
    is_merged_transcript(text).then_some(labels.retry_unavailable)
}

fn is_merged_transcript(text: &str) -> bool {
    TRANSCRIPT_TRANSLATIONS.values().any(|strings| {
        let mic = format!("{}: ", strings.speaker_mic);
        let system = format!("{}: ", strings.speaker_system);
        text.lines().any(|line| line.starts_with(&mic))
            && text.lines().any(|line| line.starts_with(&system))
    })
}

fn resolve_translations<T: Clone>(
    translations: &HashMap<&'static str, T>,
    locale: Option<String>,
) -> T {
    let normalized = locale
        .as_deref()
        .unwrap_or("en")
        .to_lowercase()
        .replace('_', "-");
    let subtags: Vec<_> = normalized.split('-').collect();
    let language = subtags.first().copied().unwrap_or("en");
    let is_hant = subtags.contains(&"hant");
    let is_hans = subtags.contains(&"hans");
    let is_traditional_region = ["tw", "hk", "mo"]
        .iter()
        .any(|region| subtags.contains(region));

    let exact_match = translations
        .iter()
        .find_map(|(code, strings)| code.eq_ignore_ascii_case(&normalized).then_some(strings));
    let fallback = match language {
        "zh" if is_hant || (!is_hans && is_traditional_region) => "zh-TW",
        // Cantonese uses Traditional Chinese unless explicitly tagged as Hans.
        "yue" if is_hans => "zh",
        "yue" => "zh-TW",
        _ => language,
    };

    exact_match
        .or_else(|| translations.get(fallback))
        .or_else(|| translations.get("en"))
        .cloned()
        .expect("English translations must exist")
}

#[cfg(test)]
mod tests {
    use super::{get_tray_translations, is_merged_transcript, TRANSLATIONS};

    #[test]
    fn resolves_locale_fallbacks() {
        for (locale, expected) in [
            ("zh-Hant-TW", "zh-TW"),
            ("zh-Hant-HK", "zh-TW"),
            ("zh-HK", "zh-TW"),
            ("zh-MO", "zh-TW"),
            ("ZH-TW", "zh-TW"),
            ("zh_Hant_TW", "zh-TW"),
            ("zh-Hans-CN", "zh"),
            ("yue-Hant-HK", "zh-TW"),
            ("yue-Hans-CN", "zh"),
            ("fr-FR", "fr"),
            ("xx-YY", "en"),
        ] {
            assert_eq!(
                format!("{:?}", get_tray_translations(Some(locale.into()))),
                format!("{:?}", TRANSLATIONS[expected]),
                "{locale} should resolve to {expected}"
            );
        }
    }

    #[test]
    fn merged_transcripts_are_detected_but_mic_only_text_is_retryable() {
        assert!(is_merged_transcript("You: hello\nSystem: hi"));
        assert!(!is_merged_transcript("hello"));
        assert!(!is_merged_transcript("You: hello"));
    }
}
