# Changelog

## 1.1.1 - 2026-10-15

- Added optional machine-local UIAccess provisioning for Robocam installed under
  Program Files. An administrator can create a non-exportable code-signing key,
  trust its public certificate on that PC, enable the embedded UIAccess manifest
  and sign the installed executable. Standard packages remain runnable without
  provisioning; no private keys are distributed.
- Installers reapply a provisioned machine's UIAccess signature before startup
  configuration or restart, including automatic updates. Signing failure fails
  installation so the updater can restore the previous application and settings.
- Diagnostic reports now include the running process's actual `ui_access` token
  flag. A signed manifest alone does not prove Windows granted the privilege.
- CI verifies the real installer's signing-failure exit status and prevents
  application launch after a signing error, using isolated disposable fixtures.
- UIAccess does not guarantee input access to System-integrity windows. The
  SketchUp ghost-window case still needs verification on the affected PC.
- Upgrade note: machine-local signatures expire with the configured certificate
  (three years for a new certificate). Retain its private key in the Windows
  machine certificate store for subsequent updates. See the packaging guide for
  provisioning and recovery instructions.

## 1.1.0 - 2026-10-15

- Fixed **Black cursor with yellow outline during click** showing no change on
  computers whose graphics card draws the pointer (a hardware pointer), notably
  with the Windows accessibility pointer styles. Robocam now builds the black arrow
  from the pointer file Windows actually draws (the custom-colour copy in your
  profile, or your pointer scheme's arrow), at the size Windows draws it (pointer
  size times the monitor's scaling), and applies it as a pointer file. It keeps your
  arrow's shape, hotspot and outline width with a black fill and the #FFE600 toolbar
  outline. Links, text and resize pointers show the same arrow while the button is
  down. Earlier builds hid the pointer behind an overlay (1.0.0) or swapped in a
  32 px image that a hardware pointer never displayed.
- The highlight lasts from the first press to the last release of a double or
  triple click, and for at least half a second, so clicks with a 0 ms press
  duration show it too. Release, cancellation, pause, profile changes and exit
  reload your saved Windows pointer scheme, size and colour; a hidden guard
  reloads it if Robocam terminates during a press.
- Added a desktop guard: windows that end up under the docked click bar, such as
  small dialogs placed at the top of the screen, are moved just below it (or beside
  it for side and bottom bars). It reacts when windows appear or are dragged and
  checks every 2 seconds. Maximized, fullscreen, minimized, hung and shell windows
  are left alone, and a window that keeps moving itself back is left alone for a
  minute. The floating bar is not guarded.
- Added a 2-second watchdog. While head control is active it releases a pointer
  confinement (ClipCursor) left by another application. It restarts the camera
  automatically when it keeps delivering the same image, stops delivering frames or
  fails, waiting longer between repeated attempts; this is deferred while
  Configuration is open. Previously a failed camera needed a manual restart.
- Diagnostics (`--diagnostic-report`) now record camera restarts, how long the
  image has been unchanged, per-second tracking motion, stall episodes (face lost
  while tracking continues, or a frozen image) with the clicks sent during them,
  windows moved from under the bar and pointer confinements released.
- `--cursor-test` now compares every role with the highlight built for the current
  pointer size and also reads the pointer shape handed to the display with DXGI
  Desktop Duplication, which sees hardware pointers that screen captures miss. It
  shows its own small window with the standard arrow under the pointer and saves
  the displayed shapes beside the report. The README and behavior guide describe
  the new mechanism, guard and watchdog.
- Known issue: with **Stop pointer when face is not detected** off, the pointer
  can stop following the head while the face detector does not find the face (for
  example when looking down) even though camera images keep arriving, until the
  face is found again. The watchdog records these episodes; it does not change
  tracking.
- Upgrade note: profiles keep their cursor feedback preference. Pointers drawn by
  applications themselves, such as SketchUp's tools, are not recoloured. The
  highlight pointer file is written to `%LOCALAPPDATA%\robocam\cursor`. Version
  1.0.1 was never published; 1.1.0 includes its changes.

## 1.0.0 - 2026-10-14

- Fixed automatic updates rolling back a successful installation because Windows
  metadata reported `X.Y.Z.0` while the updater expected `X.Y.Z`.
- Verify installation and restart using numeric Windows product version resources
  instead of display-version strings. Windows metadata remains `X.Y.Z.0`;
  packaging checks the numeric and text representations against Cargo.
- Reject nonzero Windows product revisions and genuinely mismatched versions,
  while retaining checksum validation, rollback and saved settings preservation.
- Added successful-install/restart, wrong-version and nonzero-revision coverage
  alongside cancellation, checksum rejection and failed-install recovery tests.
- Breaking upgrade: install 1.0.0 once with its published installer when upgrading
  from 0.x. Older installed helpers cannot handle the four-component display
  metadata; no compatibility shim is provided. Later updates use the new helper.

## 0.8.1 - 2026-10-14

- Relicensed Robocam's original code under Apache License 2.0 to allow broad
  reuse, including proprietary products. Earlier GPL releases retain their
  original license grants; third-party components retain their separate terms.
- Identify Alessandro Bolletta as Robocam's author in
  application notices, Cargo metadata, Windows resources and the installer.
- Added in-app Licenses and disclaimers and Open dependency licenses commands,
  with English/Italian warranty and liability summaries and the original
  English Apache license. Help and about also displays the license/disclaimer.
- Added native documentation captures that verify the full original license is
  displayed read-only, including when the interface uses Italian.
- Explicitly attributed Intel IPP 2021.11.0 and its separate Intel Simplified
  Software License. Added dependency license citations and a complete resolved
  Rust dependency index linking to packaged license texts and upstream crates.
- Include NOTICE and dependency notices in application and source packages;
  preserve license-bearing AUTHORS/README files and supply license texts omitted
  from two upstream crate archives. Packaging rejects missing dependency notices.

## 0.8.0 - 2026-10-14

- Consolidated the existing development history into a single initial commit,
  establishing v0.8.0 as the baseline for future work. Existing published release
  tags and the earlier changelog entries are retained.
- Includes all application features and fixes through 0.7.2; application behavior
  and saved profiles/settings remain unchanged.
- Synchronized the package, Windows metadata and installer versions to 0.8.0.
- Contributors with an existing checkout must realign their local main branch
  with the new history after it is pushed, preserving any unmerged work first.

## 0.7.2 - 2026-10-14

- Disabled manual resizing/maximizing of the main window. Status messages now
  wrap onto multiple lines, and the bottom area and window height adapt to show
  the complete message without ellipsis, using the current font and monitor DPI.
- Main-window dimensions now derive from a stable 96-DPI baseline at startup
  and on monitor/scale changes, including the preview, wrapped status and window
  borders. Moving back from a smaller monitor restores the intended scaled size.
- Reworked click feedback as a black/yellow arrow drawn by a dedicated overlay
  thread, independent of camera/UI processing. Only actual button presses are
  highlighted; Windows draws its normal cursor between presses. System cursor
  images, roles and saved preferences are not replaced.
- Changed the outline to #FFE600, shared with the high-contrast toolbar, and updated
  the option label. Existing profile preferences remain compatible.
- Restore native pointer visibility on release, cancellation, pause and shutdown;
  an independent guard handles unexpected termination during a press.
- Extended compositor checks to held clicks while the UI thread is blocked,
  movement, target cursor changes and repeated releases; updated the documentation.
  Physical display validation on the deployment host is still pending.

## 0.7.1 - 2026-10-13

- Preserve the main window's live visibility when restarting after an update,
  even when the normal startup preference is minimized. The fix also reads the
  handoff written by 0.7.0, so updates from that version restore an open window.
- Added regression coverage for visible/hidden update restarts and missing or
  invalid legacy state. Kept the documented minimum updater version at 0.7.0.
- Added a screenshot of the actual in-app update offer to the README.

## 0.7.0 - 2026-10-13

- Added in-app Download and install to Check for updates, with a modeless progress
  window, cancellation, retry and automatic restart. Head control remains active
  during download and preparation; held input is released before installation.
- Discover only newer stable GitHub Releases with the exact Windows x64 installer
  and published SHA-256 checksums. Download over HTTPS, reject missing/duplicate
  assets, truncated or oversized files and checksum mismatches, and recheck the
  installer under a read lock before running it.
- Preserve saved profiles, UI language, login startup and runtime click/movement
  pause state. Back up the installed application; a failed installation restores
  the previous files and restarts Robocam, with the error reported in the app.
  Required Windows restarts are reported without rebooting the computer.
- Added updater integrity/cancellation tests and CI coverage for failed-install
  recovery. Updated the README and behavior guide with the in-app update workflow.
- Upgrade note: 0.6.0 and older only open the Releases page. Install this version
  once using its installer to enable in-app installation of later releases.

## 0.6.0 - 2026-10-13

- Moved full movement/click pause from the end of the toolbar to the second button
  in the main window, immediately right of click pause. Its confirmation dialog
  keeps head control usable until confirmed; Cancel leaves the state unchanged.
  Resuming with this button also asks for confirmation; the F8 hotkey stays immediate.
- Updated the README with current native screenshots and the new pause workflow.
- Fixed click feedback disappearing: render a complete alpha image before hiding
  the native cursor, using the standard Windows arrow's shape and hotspot with a
  black fill and gray outline. Restore the native cursor before removing feedback.
  Keep the native cursor visible over protected system UI that covers the overlay.
- Added a saved per-profile Show SketchUp buttons option, enabled by default for
  new and existing profiles. Disabling it removes both cubes and compacts the bar;
  a previously selected/locked SketchUp action is reset to left click.
- Keep the main window within usable monitor space and outside the toolbar on
  movement, camera resize, DPI changes, restore and toolbar repositioning, so its
  title bar remains accessible.
- Documented the existing English/Italian UI language selector under General
  settings (restart required), and added actual desktop-pixel cursor checks and
  native window-placement/profile-option regression coverage.

## 0.5.1 - 2026-10-13

- Corrected the detailed toolbar guide: position 10 restores the main window;
  the final button now pauses/resumes both webcam movement and clicks.
- Includes 0.5.0's native webcam zoom (Robo default 382 on Anker C200, written only
  when different), full pause, matching SketchUp double/triple controls, and README
  screenshots. Application behavior is unchanged from 0.5.0.

## 0.5.0 - 2026-10-13

- Added saved, fixed webcam zoom through the native Windows camera driver. The
  driver range and step are displayed in Camera settings. Zoom is read at camera
  startup/reconnect and only written if different; the result is read back and
  verified. Unsupported drivers report an error; no software zoom is substituted.
- Set the Robo default to 282 native increments: value 382 on the Anker PowerConf
  C200 (range 100–400, step 1). Other existing profiles keep driver zoom unchanged
  until enabled; explicit saved preferences are preserved.
- Appended a separate pause/resume control for both head movement and clicks.
  It releases held input, discards pending clicks and keeps camera preview active.
  Resume using a physical click or the configurable full-pause hotkey (default F8).
- Replaced the old triple-click toolbar button in place with SketchUp triple click:
  the same cube icon as SketchUp double click, with a 3 badge, and exactly the same
  paced timing. Both activate Select with Space and cancel on focus changes.
- Added native UI screenshots and feature explanations to README, plus repeatable
  screenshot capture and camera-driver diagnostics for hardware verification.

## 0.4.2 - 2026-10-13

- Kept the click bar visible and topmost through Show Desktop, desktop Peek,
  minimize and window arrangement. Removed the hide-bar option; old hidden-bar
  preferences are ignored. Only exiting robocam removes the bar.
- Minimizing the main camera window now hides its taskbar button. Restore it from
  the last toolbar button or tray icon; the toolbar remains visible independently.
- Included the toolbar order and pointer-coordinate fixes from 0.4.1, and the
  click feedback, audio and refreshed toolbar artwork from 0.4.0.
- Added release metadata checks and a tag pipeline that verifies main ancestry,
  builds/tests the exact commit, and publishes the installer, portable package,
  source archive and SHA-256 checksums only after all assets are uploaded.
- Known limitation: the in-app updater currently offers the release page;
  automatic installer download and deployment are not yet implemented.

## 0.4.1 - 2026-10-13

- Reordered the bar to Play/Pause, left, double, SketchUp, triple, right, drag,
  middle, scroll and main window.
- Made head-controlled toolbar hit testing use the actual Windows cursor position
  after movement. Pending or clipped movement cannot activate controls or trigger
  desktop dwell at a different position.
- Corrected absolute pointer conversion to physical pixel centres over the full
  virtual desktop, including reserved toolbar space and monitors with negative
  origins. Pointer moves preserve individual native mouse-move notifications.
- Kept toolbar mouse activation from taking focus, and excluded covered buttons
  from dwell activation while keeping gaps and padding protected from desktop clicks.
- Added native regression checks for every toolbar button, unchanged cursor/bar
  positions, and the work area reserved below the bar, plus DPI coordinate tests.

## 0.4.0 - 2026-10-13

- Added a per-profile option below click sound to show a black cursor with a gray
  outline throughout a triggered click, including multi-click gaps and dragging.
  The normal cursor returns on release, cancellation, pause, or shutdown.
- Replaced the system beep with click and drag-start audio,
  embedded in the application, and added upstream attribution.
- Moved the SketchUp toolbar button from eighth to fifth, shifting Drag, Middle
  click and Scroll one place to the right.
- Reduced dark/high-contrast toolbar icon strokes from three to two logical pixels.
- Redrew the toolbar icons on one 48-unit grid with rounded strokes. Mouse icons
  keep three buttons with the wheel inside the middle one; repeat badges and arrows
  sit at the bottom right, single click no longer shows a 1, and SketchUp is a
  cube with a 2 badge. Buttons are rounded: selection uses a mint face and teal
  marker instead of teal icons, locking adds a padlock badge, and pause turns
  Play/Pause amber and dims the click actions. Icons are 40 px on the click bar
  and 28 px in the main window.
- Made toolbar artwork smooth on high-DPI displays: buttons are drawn at four times
  their size and averaged down, so curves and rounded corners are antialiased at
  every scale, while labels and focus outlines stay pixel-exact. The main window's
  title bar and taskbar icons, and the tray icon, now use the embedded frame that
  matches the monitor's DPI and follow the window between monitors.
- Updated the settings and toolbar documentation. Existing profiles keep cursor
  feedback disabled until explicitly enabled.

## 0.3.0 - 2026-10-13

- Kept the click bar visible when the webcam window is minimized or hidden and
  during desktop arrangement/fullscreen notifications. Removed automatic hiding;
  old auto-hide preferences are ignored. Explicit bar visibility remains available.
- Made the webcam button restore and bring the main window to the foreground
  on every activation, including when it is already open behind another window.
- Merged the two pause controls into one Play/Pause button at the far left and
  removed Keyboard, Configuration, Setup and Help from the desktop bar. Those
  commands remain accessible in the main window.
- Added a per-profile mouse press duration in milliseconds in Click settings,
  separate from dwell time. Existing profiles retain their previous timing until
  edited. Long press durations can exceed Windows' double-click interval.
- Updated toolbar and click-timing documentation.

## 0.2.0 - 2026-10-13

- Added a saved, per-profile high-contrast click bar option: black background,
  thicker white icons, yellow selection markers, and brighter red mouse-button
  highlights. Compact text mode also supports the high-contrast appearance.

## 0.1.1 - 2026-10-13

- Added repository instructions requiring version bumps and changelog entries for
  every change, releases from tags on main, downloadable GitHub Release
  installers, and release-based automatic updates.
- Included the changelog in application packages and the repository instructions
  in source packages.
- Made the Help dialog display the package version automatically.

## 0.1.0 - 2026-10-13

- Added head-controlled pointer movement, dwell clicks, a desktop click bar, and
  the built-in Robo profile.
- Added remembered camera selection, setup guidance, mouse actions, and a
  dedicated SketchUp double-click action.
- Added elevated input control, realtime process priority, Windows installer
  packaging, and GitHub Release update notifications.
