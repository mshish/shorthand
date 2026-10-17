// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use clap::Parser;
use shorthand_app_lib::CliArgs;

fn main() {
    #[cfg(target_os = "linux")]
    {
        // DMABUF renderer causes crashes on various GPU/display server configurations
        // See: https://github.com/tauri-apps/tauri/issues/9394
        // Preserve explicit overrides so affected users can opt out with =0 (#2166).
        if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }

    #[cfg(target_os = "windows")]
    {
        // Avoid overlay/capture layer crashes (#2049). Set before backend
        // initialization, preserving user overrides.
        if std::env::var_os("VK_LOADER_LAYERS_DISABLE").is_none()
            && !shorthand_app_lib::env_flag_enabled("HANDY_KEEP_VULKAN_IMPLICIT_LAYERS")
        {
            std::env::set_var("VK_LOADER_LAYERS_DISABLE", "~implicit~");
        }
    }

    // Transcription worker: runs transcribe.cpp in isolation and never touches
    // Tauri, the CLI, or single-instance handling. Checked after the env setup
    // above so the worker inherits it (e.g. the Vulkan layer opt-out), and
    // before the CLI is parsed, which would reject the worker's flag.
    if shorthand_app_lib::engine_supervisor::is_worker_invocation() {
        std::process::exit(shorthand_app_lib::engine_supervisor::run_worker());
    }

    let cli_args = CliArgs::parse();

    if let Some(mode) = cli_args.follow_stream {
        std::process::exit(shorthand_app_lib::follow_stream::run_client(mode));
    }

    shorthand_app_lib::run(cli_args)
}
