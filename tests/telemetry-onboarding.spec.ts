import { test, expect, type Page } from "@playwright/test";

const STUB_MODEL_ID = "stub-model";
const STUB_MODEL_NAME = "Stub Model";

/**
 * Fork-only. Stubs `window.__TAURI_INTERNALS__` so the app boots under plain
 * Vite, with a fresh profile (`onboarding_completed: false`) on a platform
 * with no permission step, and records every command the UI invokes.
 *
 * The consent step is now the last onboarding step (after model selection),
 * so the stub also carries one already-downloaded, recommended model so the
 * model step can be driven to completion without a real download.
 */
async function bootFreshProfile(page: Page, telemetryEnabled?: boolean) {
  await page.setViewportSize({ width: 680, height: 570 });
  await page.addInitScript(
    ({ telemetryEnabled, modelId, modelName }) => {
      const calls: Array<{ cmd: string; args: unknown }> = [];
      const settings: Record<string, unknown> = {
        onboarding_completed: false,
        selected_model: "",
        bindings: {},
        post_process_providers: [],
        post_process_prompts: [],
        custom_words: [],
        // Mirrors the real payload: the backend always serialises this key,
        // as `null` until the consent step or the toggle writes a value.
        telemetry_enabled:
          telemetryEnabled === undefined ? null : telemetryEnabled,
      };
      const stubModel = {
        id: modelId,
        name: modelName,
        description: "A stub model for tests.",
        filename: "stub-model.bin",
        source: "Local",
        size_mb: 1,
        is_downloaded: true,
        is_downloading: false,
        partial_size: 0,
        is_directory: false,
        engine_type: "TranscribeCpp",
        accuracy_score: 0.9,
        speed_score: 0.9,
        supports_translation: false,
        is_recommended: true,
        supported_languages: ["en"],
        supports_language_selection: false,
        is_custom: false,
        supports_streaming: false,
        supports_language_detection: false,
      };
      // @tauri-apps/plugin-os's `platform()` reads this global synchronously
      // rather than invoking a command.
      (
        window as unknown as { __TAURI_OS_PLUGIN_INTERNALS__: unknown }
      ).__TAURI_OS_PLUGIN_INTERNALS__ = {
        platform: "linux",
        os_type: "linux",
        family: "unix",
        arch: "x86_64",
        exe_extension: "",
        eol: "\n",
        version: "",
      };
      // `listen()`'s returned unlisten function calls this synchronously
      // before it invokes `plugin:event|unlisten`.
      (
        window as unknown as { __TAURI_EVENT_PLUGIN_INTERNALS__: unknown }
      ).__TAURI_EVENT_PLUGIN_INTERNALS__ = {
        unregisterListener: () => {},
      };
      (
        window as unknown as { __TAURI_INTERNALS__: unknown }
      ).__TAURI_INTERNALS__ = {
        metadata: { currentWindow: { label: "main" }, windows: [] },
        plugins: { path: { sep: "/" } },
        convertFileSrc: (path: string) => path,
        transformCallback: () => 0,
        invoke: async (cmd: string, args: unknown) => {
          calls.push({ cmd, args });
          switch (cmd) {
            // `bindings.ts` wraps the raw invoke result itself
            // (`{ status: "ok", data: await TAURI_INVOKE(...) }`), so these
            // stubs return the unwrapped value the real backend would.
            case "get_app_settings":
              return settings;
            case "change_telemetry_enabled_setting":
              settings.telemetry_enabled = (
                args as { enabled: boolean }
              ).enabled;
              return null;
            case "get_available_models":
              return [stubModel];
            case "set_active_model":
              settings.selected_model = (
                args as { modelId: string }
              ).modelId;
              return null;
            case "plugin:os|locale":
              return null;
            case "plugin:event|listen":
              return 0;
            case "plugin:event|unlisten":
              return null;
            case "get_audio_devices":
            case "get_output_devices":
              return [];
            default:
              return null;
          }
        },
      };
      (window as unknown as { __calls: unknown }).__calls = calls;
    },
    { telemetryEnabled, modelId: STUB_MODEL_ID, modelName: STUB_MODEL_NAME },
  );
  await page.goto("/");
}

async function recordedCalls(page: Page) {
  return page.evaluate(
    () =>
      (window as unknown as { __calls: Array<{ cmd: string; args: unknown }> })
        .__calls,
  );
}

/** Drives the model step to completion by picking the stubbed model. */
async function completeModelStep(page: Page) {
  await page
    .locator('[role="button"]')
    .filter({ hasText: STUB_MODEL_NAME })
    .click();
}

test.describe("telemetry consent step", () => {
  test("fits the 680x570 window with no horizontal scroll", async ({
    page,
  }) => {
    await bootFreshProfile(page);
    await completeModelStep(page);
    const cont = page.getByTestId("telemetry-continue");
    await expect(cont).toBeVisible();
    expect(
      await page.evaluate(
        () =>
          document.documentElement.scrollWidth <=
          document.documentElement.clientWidth,
      ),
    ).toBe(true);
    await expect(cont).toBeInViewport();
  });

  test("a fresh profile reaches the step with the switch on and Continue writes true", async ({
    page,
  }) => {
    await bootFreshProfile(page);
    await completeModelStep(page);
    const cont = page.getByTestId("telemetry-continue");
    await expect(cont).toBeVisible();
    await expect(page.getByRole("checkbox")).toBeChecked();
    await cont.click();
    const calls = await recordedCalls(page);
    expect(calls).toContainEqual({
      cmd: "change_telemetry_enabled_setting",
      args: { enabled: true },
    });
  });

  test("switching off before Continue writes false", async ({ page }) => {
    await bootFreshProfile(page);
    await completeModelStep(page);
    // The input is visually `sr-only`; the styled switch `<div>` its sibling
    // renders on top of it in the hit-test, but both sit inside the same
    // `<label>`, so a forced click still reaches the input via native label
    // delegation.
    await page.getByRole("checkbox").click({ force: true });
    await page.getByTestId("telemetry-continue").click();
    const calls = await recordedCalls(page);
    expect(calls).toContainEqual({
      cmd: "change_telemetry_enabled_setting",
      args: { enabled: false },
    });
  });

  test("a stored false does not change the switch's default-on position", async ({
    page,
  }) => {
    await bootFreshProfile(page, false);
    await completeModelStep(page);
    await expect(page.getByRole("checkbox")).toBeChecked();
  });
});
