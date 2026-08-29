# Build Instructions

This guide covers how to set up the development environment and build Handy from source across different platforms.

## Prerequisites

### All Platforms

- [Rust](https://rustup.rs/) (latest stable)
- [Bun](https://bun.sh/) package manager
- [Tauri Prerequisites](https://tauri.app/start/prerequisites/)

### Platform-Specific Requirements

#### macOS

> [!IMPORTANT]
> Shorthand requires **macOS 14.6 or later**. The audio backend (cpal 0.18)
> links `AudioHardwareCreateProcessTap`, which does not exist before macOS
> 14.2, and system-audio capture needs 14.6. Older macOS cannot run the app
> at all — not merely without system audio.

- Xcode Command Line Tools
- Install with: `xcode-select --install`

##### Intel Mac (x86_64)

Prebuilt ONNX Runtime binaries are not available for Intel Macs. Install ONNX Runtime via Homebrew and link dynamically:

```bash
brew install onnxruntime
MACOSX_DEPLOYMENT_TARGET=14.6 ORT_LIB_LOCATION=$(brew --prefix onnxruntime)/lib ORT_PREFER_DYNAMIC_LINK=1 bun run tauri dev
```

The same environment variables apply for production builds:

```bash
MACOSX_DEPLOYMENT_TARGET=14.6 ORT_LIB_LOCATION=$(brew --prefix onnxruntime)/lib ORT_PREFER_DYNAMIC_LINK=1 bun run tauri build
```

#### Windows

- Microsoft C++ Build Tools: Visual Studio 2019/2022 with C++ development
  tools, or Visual Studio Build Tools 2019/2022
- [CMake](https://cmake.org/download/) (must be on `PATH`):

  ```powershell
  winget install Kitware.CMake
  ```

- [Vulkan SDK](https://vulkan.lunarg.com/sdk/home) from LunarG — required to
  build the Vulkan GPU backend (`vulkan-shaders-gen` needs the SDK's headers
  and `glslc`):

  ```powershell
  winget install KhronosGroup.VulkanSDK
  ```

  Open a new terminal afterward so `VULKAN_SDK` is set.

> [!NOTE]
> Windows' 260-character path limit used to break the native Vulkan build in
> most checkouts. Since `transcribe-cpp` 0.1.3 the build works around it
> automatically (it compiles through a short NTFS junction — no admin rights
> or setup needed), so a normal checkout just builds. If you still hit
> path-limit errors, see
> [Windows build fails with path-limit errors](#windows-build-fails-with-path-limit-errors-msb3491--ftk1011--msb6003)
> in Troubleshooting.

#### Linux

- Build essentials
- ALSA development libraries
- PipeWire development libraries (system-audio capture). PulseAudio needs no
  package: cpal's PulseAudio backend is a pure-Rust reimplementation of the
  wire protocol and links no `libpulse`.
- Install with:

  ```bash
  # Ubuntu/Debian
  sudo apt update
  sudo apt install build-essential clang libclang-dev libevdev-dev libasound2-dev libpipewire-0.3-dev pkg-config libssl-dev libvulkan-dev vulkan-tools glslc spirv-headers glslang-tools libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libgtk-layer-shell0 libgtk-layer-shell-dev patchelf cmake

  # Fedora/RHEL
  sudo dnf groupinstall "Development Tools"
  sudo dnf install alsa-lib-devel pipewire-devel pkgconf openssl-devel vulkan-devel glslc \
    clang clang-devel libevdev-devel \
    spirv-headers-devel spirv-tools-devel glslang \
    gtk3-devel webkit2gtk4.1-devel libappindicator-gtk3-devel librsvg2-devel \
    gtk-layer-shell gtk-layer-shell-devel \
    cmake

  # Arch Linux
  sudo pacman -S base-devel clang libevdev shaderc spirv-headers glslang alsa-lib libpipewire pkgconf openssl vulkan-devel \
    gtk3 webkit2gtk-4.1 libappindicator-gtk3 librsvg gtk-layer-shell \
    cmake
  ```

## Setup Instructions

### 1. Clone the Repository

```bash
git clone git@github.com:cjpais/Handy.git
cd Handy
```

### 2. Install Dependencies

```bash
bun install
```

### 3. Start Dev Server

```bash
bun tauri dev
```

### 4. Build for Production

```bash
bun run tauri build
```

This compiles a release binary and generates platform-specific bundles (deb, rpm, AppImage on Linux; dmg on macOS; msi on Windows).

## Linux Install (from source)

The raw binary (`src-tauri/target/release/handy`) cannot run standalone — it needs Tauri resource files (tray icons, sounds, VAD model) to be co-located at the expected path.

**Install from the deb bundle** (works on any Linux distro):

```bash
cd /tmp
ar x /path/to/Handy/src-tauri/target/release/bundle/deb/Handy_*_amd64.deb data.tar.gz
tar xzf data.tar.gz
sudo cp usr/bin/handy /usr/bin/
sudo cp -a usr/lib/. /usr/lib/
sudo cp -r usr/share/icons/hicolor/* /usr/share/icons/hicolor/
sudo cp usr/share/applications/Handy.desktop /usr/share/applications/
```

The runtime libraries live in the app-private `/usr/lib/Handy/` (on the binary's rpath), so no `ldconfig` step is needed.

After subsequent rebuilds, copy the binary and any refreshed runtime libraries:

```bash
sudo cp src-tauri/target/release/handy /usr/bin/
sudo mkdir -p /usr/lib/Handy
sudo cp -a src-tauri/transcribe-libs/. /usr/lib/Handy/
```

Resources only need re-copying if they change upstream (new icons, sounds, models, etc.).

## Windows: "The directory name is invalid (os error 267)"

If `cargo` dies in `transcribe-cpp-sys`'s build script with

```
failed to execute command: The directory name is invalid. (os error 267)
```

it is not your MSBuild, cmake, Visual Studio generator or long-path setting.
Verified 2026-08-29: MSBuild 17.14 runs fine, cmake lists the VS 2022
generator, LongPathsEnabled is already on, and the identical cmake configure
succeeds in an ordinary directory.

`transcribe-cpp-sys` shortens paths by creating an NTFS junction at
`%LOCALAPPDATA%	cs\<hash>` pointing at `OUT_DIR`, because MSBuild's
FileTracker ignores LongPathsEnabled (FTK1011). On some machines nested
`mkdir` through that junction fails silently, so `build/` never exists and
cmake is spawned with it as its working directory.

Use the crate's own fallback: when it cannot create the junction it builds in
`OUT_DIR` instead. Point `LOCALAPPDATA` at a _file_ so that creation fails,
and keep `CARGO_TARGET_DIR` short so `OUT_DIR` stays under `MAX_PATH`:

```bash
printf x > /c/temp/not-a-dir.txt
CARGO_TARGET_DIR=C:/ct LOCALAPPDATA='C:	emp
ot-a-dir.txt' cargo check --all-targets
```

That compiles the whole crate locally in about five minutes, which beats
waiting twenty for CI to report the same thing. Set both per invocation; do
not export them.

## Running the CI workflows locally

CI runs the real cross-platform builds, so a broken workflow costs 20-40 minutes
per attempt to discover. [`act`](https://github.com/nektos/act) runs the same
workflow files in Docker on your machine. Use it before pushing a workflow
change.

```bash
act push -W .github/workflows/test.yml
```

`act` only runs the Linux jobs — the Windows and macOS matrix entries need real
runners. That still covers `test`, `code-quality` and `nix-check`, which is where
most workflow mistakes show up.

Two flags earn their keep:

```bash
# List what would run, without running it
act push -W .github/workflows/nix-check.yml --list

# Some actions install system-level tooling and need more than the default
act push -W .github/workflows/nix-check.yml --privileged
```

Give secrets an explicit empty value rather than leaving them unset, so the run
exercises the same "no credentials" path CI takes:

```bash
act push -W .github/workflows/nix-check.yml --privileged -s CACHIX_AUTH_TOKEN=""
```

### On Windows, run act from inside WSL

Not from PowerShell, Git Bash, or the Windows `act.exe` — those fail in ways that
look like bugs in the workflow but are not.

`act` caches each action's repository on disk and copies it into the container.
On NTFS there is no executable bit to copy, so any composite action that runs a
script from its own checkout dies immediately:

```
/var/run/act/actions/cachix-install-nix-action@v31/install-nix.sh: Permission denied
```

Working inside the distro removes the problem, because the cache and the checkout
both live on ext4. Docker Desktop's WSL integration already exposes `docker`
there, so only `act` itself needs installing:

```bash
curl -sL https://api.github.com/repos/nektos/act/releases/latest \
  | grep -o 'https://[^"]*act_Linux_x86_64\.tar\.gz' | head -1 \
  | xargs curl -sL -o /tmp/act.tgz \
  && tar xzf /tmp/act.tgz -C ~/bin act && chmod +x ~/bin/act
```

Clone the repository inside the distro as well. Running act against a
`/mnt/d/...` path bind-mounts it back through drvfs and reintroduces the same
permission-bit loss:

```bash
git clone /mnt/d/tools/shorthand-repos/shorthand-app ~/shorthand-app
```

Refresh that clone before a run rather than re-cloning:

```bash
git -C ~/shorthand-app fetch --all && git -C ~/shorthand-app reset --hard origin/main
```

The same reasoning applies to any container you drive by hand. Running
`docker run nixos/nix` from Windows against a `D:\` bind mount fails partway
through the bun dependency stage with `EPERM: Operation not permitted: failed to
link package` for every package — with or without a `/nix` volume, privileged or
not, and for upstream's own flake as much as this one. It is the Windows
filesystem boundary, not the build. Run it from inside the distro instead.

## Troubleshooting

### macOS Accessibility remains enabled after a local rebuild

Local builds use the ad-hoc `signingIdentity: "-"`. A rebuild can have a new macOS code
identity while the old **System Settings > Privacy & Security > Accessibility** entry
remains visibly enabled, leaving Handy on `Waiting...`.

After installing the final bundle at `/Applications/Handy.app`, quit Handy, clear only its
stale Accessibility record, then reopen it:

```bash
osascript -e 'tell application id "com.pais.handy" to quit' || true
tccutil reset Accessibility com.pais.handy
open /Applications/Handy.app
```

Grant Accessibility again when prompted. This does not reset Microphone or other TCC
services, and official releases normally do not need it.

For optional diagnosis, compare the designated requirements of the previous and rebuilt
bundles:

```bash
codesign -dr - /path/to/previous/Handy.app 2>&1
codesign -dr - /Applications/Handy.app 2>&1
```

An ad-hoc requirement contains a `cdhash`; a changed requirement confirms the rebuild is
not covered by the old grant. The reset procedure does not require this check.

See [issue #1618](https://github.com/cjpais/Handy/issues/1618) for the related onboarding
and stale-permission report.

### AppImage build fails on Arch / rolling-release distros

`linuxdeploy` bundles its own `strip` binary which is too old to process system libraries built with newer toolchains on rolling-release distros (Arch, CachyOS, Manjaro, EndeavourOS).

The error from Tauri:

```
Bundling Handy_*_amd64.AppImage
failed to bundle project `failed to run linuxdeploy`
```

Tauri swallows the real linuxdeploy error. To see it, run linuxdeploy manually:

```bash
cd src-tauri/target/release/bundle/appimage
~/.cache/tauri/linuxdeploy-x86_64.AppImage --appimage-extract-and-run \
  --appdir Handy.AppDir --plugin gtk --output appimage
```

**Workaround:** The binary, deb, and rpm bundles all build fine — only the AppImage step fails. To skip it:

```bash
bun run tauri build — --bundles deb
```

Then install using the deb extraction method above.

### Windows build fails with path-limit errors (`MSB3491` / `FTK1011` / `MSB6003`)

On Windows the native build can fail partway through `transcribe-cpp-sys` with
any of these (all the same root cause):

```
error MSB3491: Could not write lines to file "...VCTargetsPath.tlog\VCTargetsPath.lastbuildstate".
Path: ... exceeds the OS max path limit. The fully qualified file name must be less than 260 characters.
```

```
FileTracker : error FTK1011: could not create the new file tracking log file:
...\vulkan-shaders-gen-build\...\cmTC_xxxxx.tlog\link.write.1.tlog.
The system cannot find the path specified.
```

```
error MSB6003: The specified task executable "CL.exe" could not be run.
System.IO.DirectoryNotFoundException: Could not find a part of the path ...
```

This is **not** a code or toolchain problem — it's Windows' legacy 260-character
path limit (`MAX_PATH`), overflowed by the Vulkan shader generator's nested
CMake build tree on top of Cargo's already-deep
`target\release\build\<crate>-<hash>\out\build\...` directory.

Since `transcribe-cpp` 0.1.3 this is mitigated automatically: the native build
compiles through a short NTFS junction under `%LOCALAPPDATA%\tcs` (created
without admin rights), so a normal checkout builds with no setup. Enabling
Windows long paths does **not** reliably help here — MSBuild's native
`FileTracker` (`tracker.exe`) ignores the long-paths flag — which is why the
junction, not the registry flag, is the fix.

If you still see the errors above, junction creation was likely blocked
(filesystem or corporate policy) — the failing build's log then contains a
`transcribe-cpp-sys: could not create short build junction ...` warning — or
your checkout is deep enough to overflow even the shortened layout. Work
around either case with a short Cargo target directory:

```powershell
# Per-shell:
$env:CARGO_TARGET_DIR = "C:\h"

# Or persist it for all future terminals (note: redirects ALL your
# Rust projects' build output, not just Handy):
[Environment]::SetEnvironmentVariable('CARGO_TARGET_DIR', 'C:\h', 'User')
```

Artifacts then land in `C:\h\release\...` instead of the repo's
`src-tauri\target\`. Open a **new terminal** if you persisted the variable —
it is only picked up by freshly started processes. Then `bun run tauri dev`
and `bun run tauri build` work normally.

### Windows `tauri build` fails at bundling with a signing error

Historically `tauri.conf.json` carried a custom `signCommand` pointing at
`trusted-signing-cli`, which only existed in CI, so local bundling failed with
`failed to bundle project 'program not found'`.

That setting is gone — this fork signs in the release workflow instead, never
in `tauri.conf.json` (see [SIGNING_AND_UPDATES.md](SIGNING_AND_UPDATES.md)), so
`bun run tauri build` bundles locally without credentials. If you hit a signing
error at the bundling step now, something has reintroduced `signCommand`.

To compile a release binary while skipping bundling entirely:

```powershell
bun run tauri build --no-bundle
```
