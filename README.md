# robocam

A Windows 11 head-controlled mouse written in Rust, with a native interface and
OpenCV face detection and optical-flow tracking. Camera images stay on the PC.

## Install and start

Run the installer from `dist`. It installs under `Program Files\robocam` and
requests administrator permission so input can reach elevated desktop applications,
including Task Manager and the Windows on-screen keyboard. Login startup is checked
by default and uses a highest-privilege interactive task for the current user.

Normal launches request elevation. robocam sets its own process priority to
**Realtime** and rechecks it each second. OpenCV parallelism is limited to one thread.
Realtime is an aggressive scheduling class; it cannot prevent hardware/driver stalls
and can reduce CPU time available to other applications.

## Camera

Open **Configuration > Camera** to choose a device by name, then **Save**. The
selection is saved per profile using device identity, so changing device order does
not select the wrong camera. A missing saved camera is reported explicitly.

With no saved camera, a single detected camera opens automatically; multiple cameras
prompt for a choice. **Refresh cameras** updates the list. **Forget saved camera**
selects automatic mode; press **Save** to apply the reset.

## Pointer and click toolbar

Move your head to move the pointer and stop over a target to click. **Pause stops
screen clicks only**: video and head-driven pointer movement continue. Dwell on
**Play** to resume; other toolbar dwell actions are disabled while paused.

The toolbar fills the screen width, keeps buttons on the left and reserves desktop
space so maximized windows do not overlap it. The order is:

1. Play/pause (one button that changes with the current state).
2. Single left, double left, triple left.
3. Drag (four-arrow cross), middle click, wheel scrolling.
4. SketchUp group/component editing, right click, main window.

The bar stays visible when the webcam window is minimized or hidden. Automatic
hiding is no longer used, including for saved profiles that enabled it. The main
window button always restores and brings the webcam window to the foreground.
Keyboard, configuration, setup and help remain available in the main window.

In **Configuration > Click**, **Dwell time** controls the wait before a click;
**Mouse press duration (0-1000 ms)** controls how long each mouse button press
lasts, including double/triple and SketchUp clicks. Both are saved per profile.
Existing profiles keep their previous press timing until edited. Very long presses
can exceed Windows' double-click interval and be recognized as separate clicks.

Mouse icons have three buttons plus a central wheel; the relevant button/wheel is
red. There is no floating cursor progress bar.

For greater visibility, enable **Configuration > Click bar > High-contrast bar
(black background)** and press **Save**. The bar uses thicker white icons, yellow
selection markers, and bright red mouse-button highlights on black. This option
also works with compact text buttons and is remembered separately for each profile.

Select an action once to use it for the next click; select it again to lock it.
Fast mode selects on hover and locks on activation. Drag applies to the selected
single mouse button: dwell to hold, move, then dwell again to release. Wheel mode
freezes the pointer while head motion scrolls; move and stop again to finish.

The **SketchUp** action uses Space to select the Select tool, then a paced double
left click at the target. Use it over a group/component in the model viewport.
It checks the target application and cancels pending input if focus changes.
This uses SketchUp's [default shortcuts](https://help.sketchup.com/en/default-keyboard-shortcuts)
and [group editing behavior](https://help.sketchup.com/en/sketchup/grouping-geometry).

Default hotkeys are F11 (pause/resume clicks), F10 (center pointer) and F9 (workspace
limits). They can be changed or disabled. Closing the main window hides it in the
tray; use File > Exit or the tray menu to terminate.

## Robo profile

The built-in **Robo** profile is available in new and upgraded configurations.
Existing selections and customizations are preserved. It can be edited or copied
with Save as; the built-in profile cannot be deleted.

| Setting | Robo |
| --- | --- |
| X / Y speed | 9 / 9 |
| Acceleration / smoothing / movement sensitivity | 3 / 3 / 2 |
| Workspace limits / wrapping | Off / off; margins stored as 1% |
| Clicks enabled at startup | On |
| Dwell / tolerance / consecutive clicks | 400 ms / 2 / off |
| Click sound / physical mouse cooldown | Off / 0 ms |
| Hotkeys | Disabled |
| Keyboard / event delay | `osk.exe` / 0 ms |
| Automatic face location / detection interval | On / 100 ms |
| Stop on classifier face loss | Off; timeout stored as 5 seconds |

Camera failure or loss of reliable optical-flow features always releases held
buttons. Calibration and the live preview use the same tracking-readiness check
as pointer movement. Movement pauses only during calibration measurements.

Settings are stored in `%APPDATA%\robocam\settings.json`; uninstall retains them.
The interface offers English and Italian.

## Updates and privacy

Check for updates uses [robocam GitHub Releases](https://github.com/alexpacio/robocam/releases)
through `https://api.github.com/repos/alexpacio/robocam/releases/latest`. Stable tags
are compared with the installed version. Open GitHub Releases opens the selected
release page, or the releases list when no update is available. No executable is
automatically downloaded or installed. Camera images are never uploaded.

## Build

Install Rust x64 MSVC, Visual Studio C++ build tools with Windows SDK, LLVM and 7-Zip.

```powershell
.\scripts\bootstrap.ps1 -OpenCV -Installer
.\scripts\build.ps1
.\scripts\package.ps1 -SkipBuild
```

The installer, portable ZIP, corresponding source and checksums are written to
`dist`. For development, dot-source `scripts/build-env.ps1`, then run `cargo test`,
`cargo clippy --all-targets -- -D warnings` and `cargo fmt --all -- --check`.
See [packaging](docs/packaging.md) and [behavior and verification](docs/behavior.md).

The project is GPL-3.0-or-later. See [LICENSE](LICENSE) and
[third-party notices](THIRD_PARTY_NOTICES.md). Packages include corresponding source.
