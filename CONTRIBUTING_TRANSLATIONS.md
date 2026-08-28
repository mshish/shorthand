# Contributing Translations to Handy

Thank you for helping translate Handy! This guide explains how to add or improve translations.

## Quick Start

1. Fork the repository
2. Copy the English translation file to your language folder
3. Translate the values (not the keys!)
4. Submit a pull request

## File Structure

Translation files are located in:

```
src/i18n/locales/
├── en/
│   └── translation.json    # English (source)
├── vi/
│   └── translation.json    # Vietnamese
├── fr/
│   └── translation.json    # French
└── [your-language]/
    └── translation.json    # Your contribution!
```

## Fork-only strings

Shorthand adds screens upstream Handy does not have. Their strings live in
`src/shorthand/locales/<lang>.json`, separately from the catalogues above, so
that upstream's files stay byte-identical and merges never conflict on them.

A complete translation covers both files. See
[`src/shorthand/locales/README.md`](src/shorthand/locales/README.md).

## Adding a New Language

### Step 1: Create the Language Folder

Create a new folder using the [ISO 639-1 language code](https://en.wikipedia.org/wiki/List_of_ISO_639-1_codes):

```bash
mkdir src/i18n/locales/[language-code]
```

Examples:

- `de` for German
- `es` for Spanish
- `ja` for Japanese
- `zh` for Chinese
- `ko` for Korean
- `pt` for Portuguese

### Step 2: Copy the English File

```bash
cp src/i18n/locales/en/translation.json src/i18n/locales/[language-code]/translation.json
```

### Step 3: Translate the Values

Open the file and translate only the **values** (right side), not the keys (left side):

```json
{
  "sidebar": {
    "general": "General",      // ← Translate this value
    "advanced": "Advanced",    // ← Translate this value
    ...
  }
}
```

**Important:**

- Keep all keys exactly the same
- Preserve any `{{variables}}` in the text (e.g., `{{error}}`, `{{model}}`)
- Keep the JSON structure and formatting intact

### Step 4: Register Your Language

Edit `src/i18n/languages.ts` and add your language metadata:

```typescript
export const LANGUAGE_METADATA: Record<
  string,
  { name: string; nativeName: string }
> = {
  en: { name: "English", nativeName: "English" },
  es: { name: "Spanish", nativeName: "Español" },
  fr: { name: "French", nativeName: "Français" },
  vi: { name: "Vietnamese", nativeName: "Tiếng Việt" },
  de: { name: "German", nativeName: "Deutsch" }, // ← Add your language
};
```

### Step 5: Test Your Translation

1. Run the app: `bun run tauri dev`
2. Go to Settings → General → App Language
3. Select your language
4. Verify all text displays correctly

### Step 6: Submit a Pull Request

1. Commit your changes
2. Push to your fork
3. Open a pull request with:
   - Language name in the title (e.g., "Add German translation")
   - Any notes about the translation

## Improving Existing Translations

Found a typo or better translation?

1. Edit the relevant `translation.json` file
2. Submit a PR with a brief description of the change

## Translation Guidelines

### Do:

- Use natural, native-sounding language
- Keep translations concise (UI space is limited)
- Match the tone of the English text (friendly, clear)
- Preserve technical terms when appropriate (e.g., "API", "GPU")

### Don't:

- Translate brand names (Handy, transcribe.cpp, ggml, OpenAI)
- Change or remove `{{variables}}`
- Modify JSON keys
- Add extra spaces or formatting

### Handling Variables

Some strings contain variables like `{{error}}` or `{{model}}`. Keep these exactly as-is:

```json
// English
"downloadModel": "Failed to download model: {{error}}"

// French (correct)
"downloadModel": "Échec du téléchargement du modèle : {{error}}"

// French (incorrect - don't translate the variable!)
"downloadModel": "Échec du téléchargement du modèle : {{erreur}}"
```

### Handling Plurals

Some languages have complex plural rules. For now, use a general form that works for all cases. We may add proper plural support in the future.

## Questions?

- Open an issue on GitHub
- Join the discussion in existing translation PRs

## Currently Supported Languages

24 locales, listed in `src/i18n/languages.ts`. `bun run check:translations`
enforces key parity across all of them for upstream's catalogues, and
`bun run check:fork-translations` does the same for the fork-only catalogues
above — so "supported" means both files, not just this one.

| Language            | Code    | Status                                                                                                                                                                                                                                                          |
| ------------------- | ------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| English             | `en`    | Complete (source)                                                                                                                                                                                                                                               |
| Simplified Chinese  | `zh`    | Complete                                                                                                                                                                                                                                                        |
| Traditional Chinese | `zh-TW` | Complete                                                                                                                                                                                                                                                        |
| Spanish             | `es`    | Complete                                                                                                                                                                                                                                                        |
| French              | `fr`    | Complete                                                                                                                                                                                                                                                        |
| German              | `de`    | Complete                                                                                                                                                                                                                                                        |
| Japanese            | `ja`    | Complete                                                                                                                                                                                                                                                        |
| Korean              | `ko`    | Complete                                                                                                                                                                                                                                                        |
| Vietnamese          | `vi`    | Complete                                                                                                                                                                                                                                                        |
| Polish              | `pl`    | Complete                                                                                                                                                                                                                                                        |
| Italian             | `it`    | Complete                                                                                                                                                                                                                                                        |
| Russian             | `ru`    | Complete                                                                                                                                                                                                                                                        |
| Ukrainian           | `uk`    | Complete                                                                                                                                                                                                                                                        |
| Portuguese          | `pt`    | Complete                                                                                                                                                                                                                                                        |
| Czech               | `cs`    | Complete                                                                                                                                                                                                                                                        |
| Turkish             | `tr`    | Complete (8 keys under `settings.advanced.acceleration.transcribe.*`, `overlay.style.*`, `overlay.position.*` and `about.acknowledgments.ggml.*` predate an upstream rewording and haven't been revisited — a translation-quality follow-up, not a missing key) |
| Arabic              | `ar`    | Complete                                                                                                                                                                                                                                                        |
| Hebrew              | `he`    | Complete                                                                                                                                                                                                                                                        |
| Swedish             | `sv`    | Complete                                                                                                                                                                                                                                                        |
| Bulgarian           | `bg`    | Complete                                                                                                                                                                                                                                                        |
| Dutch               | `nl`    | Complete                                                                                                                                                                                                                                                        |
| Nepali              | `ne`    | Complete                                                                                                                                                                                                                                                        |
| Hindi               | `hi`    | Complete                                                                                                                                                                                                                                                        |
| Danish              | `da`    | Complete                                                                                                                                                                                                                                                        |

This table used to list 7 languages as complete and ask for help with Korean
and Portuguese — both had already shipped by the time anyone re-read it.
Treat `src/i18n/languages.ts` as the source of truth for what's supported and
this table as a rendering of it, not the other way round.

---

Thank you for making Handy accessible to more people around the world!
