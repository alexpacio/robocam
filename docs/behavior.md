# robocam behavior and verification

robocam is a Windows 11 native Rust application. OpenCV detects faces and tracks
optical flow on a worker thread. The pointer uses independently configurable X/Y
speed, acceleration, smoothing, movement threshold, workspace limits and wrapping.

## Camera selection

Configuration lists capture devices by name using DirectShow and Media Foundation.
Save stores the device identity in the active profile. Device order changes do not
change the selected camera. Missing saved cameras are reported instead of silently
substituting another device. With no saved choice, a single camera opens automatically;
multiple cameras require a selection. Refresh discovers connected devices. Forget
saved camera selects automatic mode; Save applies the reset.

Fixed zoom uses the selected camera's `IAMCameraControl` native Zoom property.
The saved value is a count of driver increments above its reported minimum. On
startup/reconnect, after opening/configuring capture, robocam reads the current
value, writes manual zoom only if different, and verifies readback. There is no
software zoom fallback. The Robo default is 282 increments, native value 382 on
the tested Anker C200 (100–400, step 1). A driver without fixed zoom support must
have the option disabled. The control code reuses an existing COM apartment when
OpenCV has already initialized the capture thread.

## Clicking and pause

The toolbar order is click Play/Pause, single left, double left, SketchUp double,
SketchUp triple, right click, drag, middle, scroll, main window and full pause.
Keyboard, configuration, setup
and help are available in the main window. Icons have three mouse buttons with
the wheel inside the middle one; the relevant button or wheel is red. Modifiers sit
at the bottom right: a 2/3 badge for double/triple, four arrows for drag and an
up/down arrow for scroll. SketchUp uses matching cubes with 2 and 3 badges.
State is shown by the button face: mint with a teal marker when selected, a
padlock badge when locked, and amber Play/Pause with dimmed click icons when paused.

Pause disables desktop clicks, releases held buttons and cancels pending actions.
Camera acquisition and head-driven pointer movement continue. Dwelling on Play
still resumes clicking; the full-pause button remains reachable as well. Other
toolbar dwell actions are suspended while click-only pause is active.
Physical mouse actions remain available. The final full-pause button stops both
webcam-driven movement and clicks, cancels pending input and resets motion history.
Camera preview continues. Dwell is suspended during full pause, preventing an
immediate unintended resume under the stationary pointer. Resume with a physical
click or the configurable full-pause hotkey (F8 by default). The previous click-only
pause state is retained when resuming full pause.

Normal selection is temporary; selecting it again locks it. Fast mode selects on
hover and locks on activation. Drag holds the selected single mouse button until
movement followed by another dwell. Scrolling freezes the pointer and maps head
motion to wheel input until the next moved-then-stopped dwell. There is no cursor
progress overlay. Zero-delay clicks are dispatched in the frame that completes
the dwell countdown.

Head-controlled hit testing uses the cursor position confirmed by Windows after
movement. If input is still queued or constrained to another position, dwelling is
reset until the move lands. Absolute input uses the full physical virtual desktop,
including the click bar; the reduced work area is only for placing application
windows. Toolbar buttons run local commands without moving or clicking the pointer
in the window underneath, and gaps/padding also suppress desktop dwell.

The optional black cursor with gray outline is below click sound in Click settings.
It lasts from action dispatch through the last release, including multi-click gaps
and held drag buttons. Cancellation, pause, profile changes and shutdown restore
the normal cursor. Zero-duration actions do not add an artificial highlight delay.
The replacement is a transparent, non-activating window; it does not receive clicks
or change the user's saved Windows cursor scheme. Sound feedback uses the embedded
original eViacam click audio, with its alternate sound when starting a drag.

SketchUp mode checks the application under the pointer, activates its Select tool
with Space, then sends a paced left double or triple click at a fixed position.
Both use the same timing function; the triple adds one down/up pair. If foreground
focus changes during the sequence, pending input is cancelled. It uses SketchUp's
default Space shortcut and is intended for the model viewport.

Native Anker acceptance checks use an isolated profile and `--fully-paused`:
set a different driver value, verify startup restores 382, restart at 382 and
verify `driver_zoom.applied` is false. Both runs must capture frames while
`input_events_sent` remains zero. Restore the original driver value after testing.

## Runtime and startup

Normal operation requests administrator privileges to inject input into elevated
desktop applications. Login startup is an interactive, highest-privilege scheduled
task for the current user. Runtime priority is Realtime and is rechecked each second;
OpenCV internal parallelism is limited to one thread. Realtime scheduling does not
guarantee responsiveness when camera drivers or other hardware stall.

The toolbar is a Shell AppBar. Top/bottom docking spans monitor width and reserves
desktop work area; buttons start at the left. It stays visible through Show Desktop,
Peek and minimization; only application shutdown removes its reservation. The main
window minimizes to the tray and can be restored by the main-window button
(position 10) or tray icon. The last button is now full pause/resume.
Main icons use 28 logical pixels in 36-pixel buttons and click icons
use 40 in 48-pixel buttons. They share a 48-unit grid with round caps and joins,
with per-monitor DPI scaling. GDI does not antialias, so each button is painted at
four times its size and reduced with HALFTONE; compact labels and focus outlines
are drawn afterwards at the real size. Window and tray icons are loaded from the
embedded frame for the current DPI and replaced on WM_DPICHANGED. Calibration uses the same fresh-tracking readiness as
pointer control and pauses movement only during its measurement periods.

## Verification checklist

1. Select a camera, save, restart, then verify that the same device is selected.
   Test two identically named devices, removal, reconnect, Refresh and Forget.
2. Pause: camera frames and pointer movement continue; desktop clicks stop; Play
   can still be activated by dwelling.
3. Test single/double/triple, middle/right, drag, scroll and temporary/locked modes.
   Pausing, losing reliable tracking or exiting must release held buttons.
4. Test SketchUp on a group/component in the model viewport and cancel by switching
   applications during its sequence.
5. Verify clicks in elevated Task Manager and the Windows on-screen keyboard.
6. Confirm the AppBar spans the monitor and maximized windows remain below it.
7. Verify the highest-privilege login task and Realtime priority in diagnostics.
8. Enable cursor feedback, test timed single/multi-clicks and dragging, then cancel
   using pause or tracking loss. Check restoration and independent profile settings.
   Compare light/dark toolbar outlines and the fourth-position SketchUp button.

Unit tests cover timing, selection, gesture actions, device identity, persistence,
version checks and real OpenCV tracking on synthetic frames. Package smoke tests
cover native startup and shutdown without input injection. Hardware, target-app
behavior and monitor combinations also require runtime checks.

The ignored `native_cursor_lifecycle` test requires an interactive Windows desktop;
it briefly displays feedback without injecting input, checks that it cannot take
focus or intercept the click target, then restores the cursor (also on test panic).

`--desktop-test --smoke-report <path>` additionally toggles Windows Show Desktop,
checks that the toolbar remains the actual hit-test target while the main window
is hidden, restores the desktop and verifies reopening the main window. Run this
only in an interactive session; it briefly rearranges that session's windows.
