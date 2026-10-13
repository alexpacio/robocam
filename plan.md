# robocam implementation plan

Build a Rust application with a native Windows 11 interface and OpenCV head-motion
tracking. Provide a Windows installer and automatic login startup.

Provide separate single, double and triple left-click controls, middle/right click,
wheel scrolling and a drag action that holds the selected single button until the
next moved-then-stopped dwell. Include profiles, calibration, keyboard access,
hotkeys, workspace options and update checks through GitHub Releases.

Use 48-pixel click icons and 32-pixel main icons with DPI scaling. The top bar must
be fixed, full-width and left-aligned, reserving workspace above other windows.
Draw a three-button mouse with a central wheel and red action indicators.

Pause only desktop clicks; keep camera and pointer movement active and keep Play
accessible through dwell. Provide a dedicated SketchUp group/component editing action.
List cameras by name, remember stable device identity and allow forgetting the choice.
Use elevated input access and Realtime process priority, with bounded vision work.
Ship the Robo profile and support deployment diagnostics and reproducible packaging.
