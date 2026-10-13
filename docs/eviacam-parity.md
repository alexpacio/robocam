# eViacam compatibility reference

The reference is [eViacam](https://github.com/cmauri/eviacam), revision
`a4032ed9c59def5399a93e74f5ea84513d2f42b1`, inspected while implementing robocam.
This document records the upstream behavior and sizing contract. It is a source
audit, not a claim that a camera or every Windows application has been tested.

## Pointer motion and tracking

| Area | Upstream behavior and source |
| --- | --- |
| Camera | Select a capture device, display live video and measured FPS, expose device settings. `viacamcontroller.cpp`, `wviacam.cpp`, `wcameradialog.cpp`. |
| Tracking | Automatically locate the face, track a region with image motion, and allow manual movement/resizing of the region. `visionpipeline.cpp`, `wxcamwindow/camwindow.cpp`. |
| Lost face | Optional stop after a configurable face-detection timeout; independent CPU-usage setting controls detection frequency. `visionpipeline.cpp`, `wconfiguration.cpp`. |
| Sensitivity | Independent X/Y speed (0–30), acceleration (0–5), smoothing (0–8), motion threshold (0–10). Defaults: 10, 10, 2, 2, 1. `pointeraction.cpp`, `wconfiguration.cpp`. |
| Workspace | Optional limits on each edge, expressed as 0–50% of the workspace; initial edge margins 1%, with limits disabled. Optional pointer wrapping. `pointeraction.cpp`. |
| Calibration | Assisted horizontal/vertical motion calibration and a confirmation/test step; setup wizard introduces camera, motion and clicking. `cmotioncalibration.cpp`, `wmotioncalibrationx.cpp`, `wmotioncalibrationy.cpp`, `wwizard.cpp`. |

## Clicks

Upstream supports no click, single left/middle/right click, double left click and
left drag. robocam additionally requires triple left click, wheel scrolling and
dragging with the selected single-click button.

The upstream dwell defaults are 1,000 ms, tolerance level 3, consecutive clicks
disabled, visual feedback enabled and audible feedback enabled. The dwell-time
control spans 200–5,000 ms. `dwellclick.cpp`, `pointeraction.cpp` and the underlying
mouse-control implementation define the conversion from tolerance level to pixels.

The click window has a selected action and a locked action. In normal mode, the
first selection is temporary; selecting the same temporary action again locks it.
After an action outside the toolbar, the selection returns to the locked action.
In fast mode, entering a button temporarily selects it, while clicking it locks it.
The no-click button suspends click generation but allows itself to be activated
again. A drag remains selected through mouse-down and resets only after the next
dwell releases the button. See `clickwindowcontroller.cpp:179–232,326–398`.

Click feedback, consecutive clicks, showing/hiding the click window, normal/thin
design, normal/fast selection and a configurable 0–50 ms delay between native
button events are profile settings.

## Windows and platform-specific features

| Feature | Upstream scope |
| --- | --- |
| Profiles | Add/delete/select independent profiles, including motion, dwell, tracking and hotkeys. |
| Hotkeys | Toggle control (F11), center pointer (F10), toggle workspace limits (F9), and optional arrow-key sensitivity adjustment. Keys can be reassigned and individually enabled. |
| Main window | Enable/disable, click-window visibility, on-screen keyboard, configuration, help, wizard, About, system-tray icon and FPS. |
| Startup | Enable control at startup, startup minimization and the first-run wizard. Windows keyboard command defaults to `osk.exe`. |
| Camera settings | Camera selection and controls offered by the camera/backend. |
| Language | Translation selection using the upstream localization catalog. |
| Updates | Optional compile-time update-check support and its preference. A new product needs its own release feed. |
| Gesture clicks | Directional gesture selection and key actions are exposed under `__WXGTK__` in the upstream options UI. They are not upstream Windows options. |
| Bar docking/autohide | Floating horizontal/vertical, four docking edges and autohide preferences are exposed under `__WXGTK__`; upstream Windows forces a docked click window. |
| Login startup | The upstream configuration checkbox is GTK-only. robocam's Windows installer explicitly provides a per-user login startup option. |

## Dimensions

Upstream uses wxWidgets layout and native control metrics. Some requested sizes
are fixed bitmap dimensions; other window dimensions are computed from font,
language, theme and DPI. They cannot all be represented by one fixed pixel size.

| Element | Upstream source dimensions |
| --- | --- |
| Normal click icons | **48 × 48** pixels, including their selected/disabled/locked variants. `src/resources/button*.xpm`. |
| Normal bar logo | **48 × 48** pixels. |
| Normal button margins | **2 pixels** on each edge of each bitmap button. |
| Normal bar separators | Three **5 × 5** spacers with **5-pixel** margins. |
| Normal bar content | One logo + seven buttons + three separators. Minimum arithmetic content size **457 × 52** before any native button padding; vertical design transposes the layout. Adding triple and scroll buttons while retaining the same button sizes requires 104 additional pixels along its long edge. |
| Thin bar | Native text toggle buttons with **3-pixel** margins, plus **16 × 16** logo; text/font dependent, not a hardcoded width. `clickWindowText*.xrc`. |
| Main toolbar | **32 × 32** bitmaps. `wviacam.cpp`. |
| Main frame | Nominal **300 × 300**, then camera preview changes the client area to the image dimensions. Initial preview **160 × 144**. `wviacam.h`, `wviacam.cpp`, `camwindow.cpp`. |
| Configuration | `wxDefaultSize`, fitted to its contents; no fixed upstream width/height. |
| Calibration dialogs | Nominal **400 × 300**, then fitted native content. |
| Camera properties | Inner scrolled controls area **500 × 400**. |
| About | Inner HTML area **600 × 450**. |
| Calibration illustrations | **64 × 64**. |
| Setup wizard | Native fitted pages; introductory logo **303 × 100**, click test button **50 pixels** high. |

The robocam interface uses new drawn icons rather than copying upstream artwork.
Windows scaling should preserve these logical dimensions while adapting to DPI.
The final docked robocam toolbar uses the full monitor width (or height for a
vertical edge) and registers a Windows AppBar to reserve its workspace. This
follows the requested full-width layout while preserving the 52-pixel normal
thickness. Hiding or closing the bar releases that reservation; autohide keeps
the reserved strip so maximized application windows do not overlap it.

## Implemented behavior and remaining differences

robocam implements face/optical-flow tracking, motion transfer, workspace limits
and wrapping, dwell and directional-gesture clicking, temporary/locked selection,
all requested click types, two-stage dragging/scrolling, profiles, calibration,
live preview, camera controls, tray operation, keyboard launching, hotkeys,
feedback, login startup and optional release notifications. Its input engine
releases held buttons on tracking loss, pause and mode changes. Physical mouse
use temporarily pauses head control; startup control is off by default.

The main toolbar keeps the 32-pixel icon scale and normal click targets keep the
48-pixel icon scale. The added triple/scroll actions and pause control extend the
button group's length. Options and setup pages use native controls with newly arranged
content, rather than the original wxWidgets widgets.

The rewrite does not yet reproduce every upstream setting or translation:

| Area | robocam behavior / difference |
| --- | --- |
| Languages | English and Italian are available. The rest of eViacam's translated catalogs have **not** been ported. |
| Camera selection | A configurable numeric capture-device index, rather than the upstream list of named devices. Driver settings depend on the selected camera/backend. |
| Hotkeys | Configurable function keys for enable/center/workspace and optional arrow sensitivity keys. Arbitrary upstream key capture is not reproduced. |
| Keyboard command | An executable path opened by Windows, rather than an arbitrary command line with arguments. |
| Face loss | Stops control and releases held buttons; the timeout range is 100–3,000 ms. Upstream exposes an optional stop policy with a longer timeout range. |
| Close button | Hides the main window to the notification area; use Exit to terminate. Upstream prompts to quit. |
| Updates | Checks this project's own GitHub stable releases, with a five-second timeout; does not download/install updates. No update exists until a release is published. |
| Gestures | Adds directional gesture clicking on Windows; mapping to arbitrary keyboard events from the upstream Linux interface is not implemented. |

These differences are part of the current implementation, not completed parity
claims. Unit and smoke tests cover deterministic behavior and native startup;
the camera/application checks below still need target-hardware acceptance.

## Acceptance checks on a Windows 11 machine

1. With the camera permitted, obtain steady tracking at normal head distance;
   move horizontally/vertically, verify direction, smoothing and acceleration.
2. Hold still, verify one dwell click, move away to rearm, then test consecutive
   clicking. Test no-click, left, middle, right, double and triple separately.
3. Start each supported drag, move onto a target and stop to release. Disable,
   quit or lose tracking mid-drag and verify no mouse button remains held.
4. Enter wheel mode, move vertically, stop to finish and verify normal pointer
   control resumes. Test scrollbar/application behavior with a real camera.
5. Check temporary versus locked selections and fast hover mode. Dwell on the
   toolbar must not send a click through to another application.
6. Test hotkeys, keyboard, manual tracking area, profiles, calibration, workspace
   edges, monitor boundaries, and physical mouse recovery.
7. Install as an ordinary user, verify the startup option is selected, confirm
   its HKCU Run entry contains `--minimized`, and verify uninstall removes the entry.
   Reinstall with startup unchecked and verify no stale login entry remains.

Camera lighting, camera drivers, elevated applications, secure desktops and
multi-monitor DPI behavior require hardware/manual validation beyond unit tests.
