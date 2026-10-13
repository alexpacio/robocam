# Changelog

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
  click feedback, eViacam audio and refreshed toolbar artwork from 0.4.0.
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
- Replaced the system beep with eViacam's original click and drag-start audio,
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
