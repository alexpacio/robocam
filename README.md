# robocam

robocam is a Windows 11 head-controlled mouse written in Rust, using native Win32
windows and OpenCV face detection and optical-flow tracking. It follows eViacam's
relative head-motion control and dwell-click behavior, with additional triple
click, wheel scrolling and middle/right-button dragging.

## Start using it

Run the installer from `dist`, or extract the portable ZIP and run `robocam.exe`.
The installer includes the native runtime dependencies and needs no administrator
account. **Start robocam when I sign in to Windows** is selected by default; it
can be unchecked in setup or changed in the application's General settings.

Place your camera at eye level, ensure your face is well lit and visible, and
follow the setup/calibration wizard. Allow desktop applications to access the
camera in Windows **Settings → Privacy & security → Camera** if necessary.

Control starts **paused** by default. Press **F11** or the enable button to start
moving the pointer. Move your head gently left/right and up/down; hold it still
over a target for the dwell interval (initially one second) to click. Press F11
again to pause. Moving the physical mouse temporarily gives it priority.

- **F11:** enable/pause head control.
- **F10:** center the pointer.
- **F9:** toggle workspace limits.

The keys can be changed in configuration. The system tray menu brings back the
main window and provides pause and exit. The keyboard button opens Windows'
on-screen keyboard; another keyboard command can be configured.
Optional arrow hotkeys adjust horizontal/vertical speed. General settings also
provide a 0–50 ms delay between button events for applications that need it.

## Click toolbar

The toolbar has separate controls for no-click, left, middle, right, double,
triple, drag and scroll actions, plus showing the main window.
The docked bar spans the monitor edge and reserves workspace, so maximized
application windows remain below it. Its normal height is 52 logical pixels.

Select a click type to use it once. Select the same temporary type again to keep
it locked. In fast mode, hovering selects a temporary type and clicking locks it.
**No click** pauses dwell clicking while pointer movement continues; selecting
it again restores clicking.

To drag, select the desired single button (left, middle or right), then **Drag**.
The first dwell presses and holds that button. Move to the destination and stop
again to release it. Drag stays active until that second dwell. Selecting a
different mode, pausing or losing tracking releases any held button.

For scrolling, select **Scroll**, dwell over the target to enter scrolling, then
move your head vertically. Stop again to finish scrolling. Double and triple
clicks always use the left button.

Configuration controls dwell time, tolerance, consecutive clicking, click sound,
visual feedback, normal/thin toolbar design and normal/fast selection. Motion
controls include independent horizontal/vertical sensitivity, acceleration,
smoothing, a motion threshold, workspace limits and wrapping. Profiles store
separate settings for different users or setups.

## Camera and settings

Open configuration to choose a camera, adjust tracking and configure the
face-loss timeout. Automatic face detection initializes the tracking region;
manual region selection is available in the video preview. Assisted calibration
helps choose sensitivity for your comfortable range of motion.

Settings are stored in `%APPDATA%\robocam\settings.json`. Uninstall keeps these
profiles. Login startup launches with `--minimized`; whether pointer control is
enabled on launch is a separate setting and remains off until you enable it.

The application works locally: camera frames are processed on the computer and
are not uploaded. **Check for updates** uses the latest stable release from
[alexpacio/robocam on GitHub Releases](https://github.com/alexpacio/robocam/releases)
via `https://api.github.com/repos/alexpacio/robocam/releases/latest`. It compares
the release tag with the installed version and skips drafts and prereleases.
The **Open GitHub Releases** button opens the new version's release page, or
the repository's release list when no update is available or the check fails.
The request sends the application version as its user agent; robocam never
downloads or installs application code automatically. No update is offered
until a release is published. Windows' secure desktop and applications running at a higher
privilege level do not accept ordinary application input injection.

The interface offers **English and Italian**; change language in General and
restart. The remainder of eViacam's translation catalogs have not been ported.

## Build

Install Rust (x64 MSVC), Visual Studio C++ build tools with the Windows SDK,
LLVM/libclang and 7-Zip. Then run:

```powershell
.\scripts\bootstrap.ps1 -OpenCV -Installer
.\scripts\build.ps1
.\scripts\package.ps1 -SkipBuild
```

This builds a tested release binary, a per-user installer, a portable ZIP,
corresponding application source and checksums under `dist`. Native SDK and
installer tools are extracted locally into `.tools` with pinned checksums.
See [packaging.md](docs/packaging.md) for dependency overrides and details.

Development tests can also be run with the native libraries on PATH:

```powershell
. .\scripts\build-env.ps1
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --all -- --check
```

The control-engine tests cover pointer transfer, dwell timing, selection/locking,
drag release and wheel scrolling. Camera response, click behavior in individual
applications, and monitor/DPI combinations still need manual validation on the
target hardware; see [the compatibility checklist](docs/eviacam-parity.md).

## Compatibility and license

The [eViacam reference audit](docs/eviacam-parity.md) records upstream behaviors,
exact icon dimensions and which settings are specific to its Linux interface.
The normal click icons retain 48 × 48 logical pixels and main toolbar icons retain
32 × 32; artwork is new. Additional actions extend the row of click buttons.

This is an independent Rust application, not the original eViacam executable.
The core control behavior is implemented, but the interface is a native rewrite:
camera selection uses device indices, hotkey assignment uses function keys,
and the on-screen keyboard setting takes an executable path. See the compatibility
document for retained behavior and deliberate differences.
The project is GPL-3.0-or-later. See [LICENSE](LICENSE) and
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for attribution and dependency
licenses. Packages include their corresponding robocam source archive.
