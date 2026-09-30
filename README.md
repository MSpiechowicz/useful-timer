# Useful Timer

A native Rust desktop app with independent animated countdown widgets and a charcoal-and-amber timer workspace.

## Features and controls

- Up to 64 independent timers, each with its own label, duration, style, width, sound volume, and mute setting.
- Three built-in styles: **Bomb** bursts into sparks, textured smoke, and dark shell fragments of varied sizes and shapes that settle in an irregular scatter; **Hourglass** drains shaded, granular sand along the curved glass interior and flips, with a larger countdown display; **Rocket** has a flickering engine and textured exhaust, then accelerates out of view, leaving smoke that dissipates. Each has a predefined completion sound. Completion effects are finite, not looping alarms.
- During a countdown, hourglass sand tracks the remaining fraction of the configured duration by modeled 3D volume, using circular cross-sections of the curved glass and sand mound—not screen area or fill height. Half the time remaining means half the volume remains above; 10 seconds left on a one-minute timer means one-sixth remains. At zero, the lower chamber holds the volume that started in the upper chamber. The same volume is conserved throughout the completion flip, within rendering precision. Sand uses one shaded surface with antialiased edges, without a separate oval cap; its surface stays level during the flip and settles against the neck instead of leaving an empty wedge.
- Transparent, borderless widgets request always-on-top placement. A compact name badge sits above the artwork; long names are ellipsized. Right-click the title or artwork for a commands-only menu: **Start / Pause / Resume**, **Reset**, **Remove**, and **Lock / Unlock**. Drag either the title or artwork to move an unlocked widget.
- **Lock** prevents dragging from both the title and artwork without disabling countdown controls. **Unlock** allows movement again. The lock is saved independently for each timer; older saved timers open unlocked.
- In the control panel, choose **+ New timer**, set the name, duration, and artwork, then click **Create timer**. Select a timer card to open its workspace. Click its name to rename it; valid settings changes apply immediately, with no Edit or Apply step. The first saved timer is selected when the app opens.
- **Start** begins an idle countdown. **Pause** freezes its remaining time; **Resume** continues from that time. Timers run independently, including when the control panel is minimized.
- **Reset** cancels the current countdown and returns the timer to its full configured duration, idle. Completed timers are marked **Done** in the workspace; Start stays disabled until reset. The bomb remains as debris, the hourglass remains flipped, and the rocket stays off-screen; its title remains available for right-click controls and, when unlocked, dragging.
- Use the full-sized **5 / 15 / 25 / 45 min** duration presets or the time fields to set the configured duration. Preset buttons match the remaining-time adjustment buttons in size. Changing duration returns the timer to idle at the new full duration; changing its label, style, width, or volume preserves the countdown.
- Open **Adjust remaining time** under a running or paused countdown, type or paste separate **Hours**, **Minutes**, and **Seconds**, then click **Set remaining time**. Quick **−1 / +1 min** and **−5 / +5 min** buttons adjust the current countdown. Running timers keep running; paused timers stay paused. Reducing the remaining time to zero completes the timer once. These changes do not alter the configured reset duration; adjustments are capped at 24 hours.
- Choose **Bomb / Hourglass / Rocket** in the single **Desktop artwork** picker to switch an existing timer immediately without resetting its countdown.
- **Mute completion sound** silences that timer without disabling its animation. Its volume control is disabled while muted, but its configured volume is retained for unmuting. New timers use the setting chosen before **Create timer**.
- The workspace's **•••** menu contains **Restore defaults** and **Remove timer**. Restoring defaults resets the selected timer to `Focus`, 25 minutes, Bomb, 300 logical pixels wide, volume `0.60`, unmuted, and idle. It preserves identity, position, and lock state. **Reset form** only resets a new-timer form.
- **Remove** in the widget menu, **Remove timer** in the workspace menu, or closing a native widget removes that timer. Closing the **Useful Timer** control panel quits the app and every widget; it does not continue as a background service.

Duration uses separate **Hours / Minutes / Seconds** inputs; click a value to type or paste it. Configured durations range from `00:00:01` to `24:00:00`. Minutes and seconds each range from 0 to 59. The animation displays `H:MM:SS` at an hour or more (including two-digit hours), and `MM:SS` below an hour. Labels accept up to 80 Unicode characters. Widget width ranges from 220 to 460 logical pixels. Volume ranges from `0.0` (silent) to `1.0`; overlapping completions mix rather than wait in a queue. Sounds are generated locally and do not require audio asset downloads.

The app uses a reusable U-shaped clock mark in its header and native window icons. The scalable source is [`assets/useful-timer.svg`](assets/useful-timer.svg); [`assets/useful-timer.png`](assets/useful-timer.png) is the transparent 256×256 app icon, embedded in the binary. The header uses mipmapped linear filtering to preserve antialiased edges when reducing the icon to its display size. Both assets are available under the repository license.

## Build and run

Run the following commands from this repository's checkout. Build on the operating system where you intend to run the app; these instructions do not configure cross-compilation.

### Prerequisites

Install [Rust using rustup](https://rustup.rs/). Rust **1.95 or newer** is required. With rustup already installed:

```sh
rustup toolchain install 1.95.0 --profile minimal --component rustfmt --component clippy
rustup default 1.95.0
```

This changes the default Rust toolchain for your user account.

**Linux (Ubuntu/Debian):**

```sh
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libasound2-dev libx11-dev libxcursor-dev libxrandr-dev libxi-dev libxkbcommon-dev libxkbcommon-x11-dev libgl-dev
```

**Linux (Arch):**

```sh
sudo pacman -S --needed base-devel pkgconf alsa-lib libx11 libxcursor libxrandr libxi libxkbcommon libxkbcommon-x11 mesa
```

Linux needs an X11 display and OpenGL drivers. On a Wayland desktop, install and enable XWayland as well:

```sh
# Ubuntu/Debian
sudo apt-get install -y xwayland

# Arch
sudo pacman -S --needed xorg-xwayland
```

The app explicitly uses X11, including through XWayland. Native Wayland overlays are not supported. Your window manager/compositor must support transparency and honor always-on-top requests.

**macOS:** install the Xcode Command Line Tools, then Rust:

```sh
xcode-select --install
```

**Windows:** install [Visual Studio Build Tools](https://visualstudio.microsoft.com/downloads/) with **Desktop development with C++**, the MSVC compiler, and a Windows SDK. Install Rust with the MSVC host toolchain using rustup, then run the Rust commands above in a new PowerShell or developer terminal. This is a native app; it does not require WebView2.

### Development run

```sh
cargo run --locked
```

### Release run and binaries

```sh
cargo run --release --locked
```

To build without launching:

```sh
cargo build --release --locked
```

Run the resulting binary on Linux or macOS:

```sh
./target/release/useful-timer
```

On Windows, run it from PowerShell or double-click the executable:

```powershell
.\target\release\useful-timer.exe
```

For a local installation into Cargo's binary directory, use:

```sh
cargo install --path . --locked
useful-timer
```

Cargo installs the release binary into `~/.cargo/bin` on Linux/macOS or `%USERPROFILE%\.cargo\bin` on Windows by default. Ensure that directory is on `PATH`. No packaged installer, macOS app bundle, desktop launcher, signing, or automatic updater is supplied. Linux binaries still need the system libraries and display environment described above.

## Local settings and privacy

The app has no accounts, cloud synchronization, telemetry, or network-dependent timer features. Timer labels and settings are stored locally as unencrypted RON data; do not treat them as secret storage.

Timer identities, settings, widget positions, and per-widget movement locks are saved in one eframe-managed `app.ron` file under the `useful-timer.state` key (format version 1). Widget positions use physical desktop pixels; restoration selects and clamps to monitor bounds in that same coordinate space, accounting for the target monitor's scale and UI zoom. The same file also holds eframe UI/window state. The app requests autosave every five seconds and saves on normal exit.

Default locations follow [eframe's native storage directory rules](https://docs.rs/eframe/0.36.2/eframe/fn.storage_dir.html):

| Platform | Settings file |
| --- | --- |
| Linux | `$XDG_DATA_HOME/useful-timer/app.ron` when `XDG_DATA_HOME` is absolute; otherwise `~/.local/share/useful-timer/app.ron` |
| macOS | `~/Library/Application Support/useful-timer/app.ron` |
| Windows | `%APPDATA%\useful-timer\data\app.ron` (the user's roaming AppData folder) |

Only Linux's path has been verified in a running desktop session; macOS and Windows paths are derived from eframe 0.36.2's native implementation.

On restart, every saved timer returns **idle at its full duration**, with its settings and position restored. Running/paused progress is not persisted, and reopening does not play completion sounds. Removed timers stay removed. There is no countdown while the process is closed.

Mute settings are saved with the other timer settings, independently of volume. Settings files created before the mute option remain supported. Live remaining-time edits are runtime-only: reset and restart continue to use the configured duration.

Saved timer data is validated, with limits of 64 timers and 256 KiB for the timer-state payload. Invalid or unsupported timer-state data is rejected with a control-panel warning. Unavailable persistence or audio also produces a warning when detected; timers remain usable without sound. Failures reading or writing eframe's outer file may instead be logged by eframe.

To clear all saved settings, quit the app first, then delete the `app.ron` file at the appropriate location. This also clears saved control-panel/UI geometry. Abrupt termination can lose changes made since the last save.

## Platform status and limitations

- **Linux desktop verified:** a debug build was exercised on KWin Wayland through XWayland. Observed behavior includes transparent corners, always-on-top state, native widget dragging, all three styles, independent pause/resume while the control panel was minimized, finite completion animations and audible output, zero-volume silence, removal, default restoration, and settings/positions restored after a normal close and restart.
- **Updated Linux release verified:** clipboard entry into all three time fields, a named hour-long bomb with no running control strip, minute adjustments, exact remaining-time edits, and live style/mute changes. Playback-monitor captures showed silent muted completion and audible unmuted completion; the muted timer still completed its animation.
- **Artwork update verified on Linux:** actual compositor captures exercised bomb fragmentation and settled debris, hourglass draining/flipping, and rocket ascent with textured smoke that clears without the rocket reappearing. Checks also covered a 24-hour bomb and long caption at the minimum 220-point width, a 460-point rocket, lowered-caption dragging, and context-menu start/pause/resume/reset/removal. Custom sphere, hull, and digit meshes use edge coverage; the hull fill and outline share a smooth contour. Smoke textures are generated once locally and reused. Transparent-window rendering does not require a multisampled GL configuration.
- **Contour corrections verified on Linux:** centered bomb cap, raised display between shallow seams that terminate inside the sphere, and dark, varied debris without diagonal rows. Hourglass captures covered partial fill, a nearly full lower chamber, and the completed flip; grains stay within the sand, and subtle reflections follow the glass walls. The rocket's lower band endpoints derive from the same curve as its hull. Bomb caption clearance includes the fuse stroke and was checked at both the 220- and 460-point widget-size limits.
- **Rocket detailing verified on Linux:** the hull has a continuous closed bottom rim, wing highlights share mirrored geometry with different lighting, and metallic shading replaces the hard vertical stripe beside the porthole and progress bar. Native captures covered 220- and 460-point widgets, running exhaust, ascent, and the cleared launch site.
- **macOS and Windows:** source and CI are configured for native builds, but local builds and desktop behavior on these platforms have not been verified. Their transparency, positioning, always-on-top behavior, and audio still need real-desktop validation.
- No native Wayland widget support, tray mode, background service, cloud features, import/export, custom animation/sound editor, or updater. Desktop placement and stacking remain subject to the operating system and window manager.
- Audio uses the default output device. A Linux smoke run with an unavailable audio device verified the visible warning and a countdown still reaching **Done** with its completion animation. Initialization and stream errors are reported when detected; physical device disconnection and reconnection have not been desktop-tested.

## Development checks and CI

```sh
cargo fmt --all -- --check
cargo check --locked
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo build --release --locked
```

The GitHub Actions workflow in [`.github/workflows/build.yml`](.github/workflows/build.yml) runs these checks on Ubuntu 24.04, macOS, and Windows with Rust 1.95.0 and locked dependencies. Its permissions are read-only; it does not sign, publish, or upload releases. The workflow is configured, but no completed CI runs are claimed here. Deterministic automated tests do not replace desktop testing of compositor and audio behavior.

## License

[GNU General Public License v3.0 only](LICENSE) (`GPL-3.0-only`).
