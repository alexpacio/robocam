# Changelog

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
