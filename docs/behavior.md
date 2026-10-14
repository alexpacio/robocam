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
SketchUp triple, right click, drag, middle, scroll and main window.
This is the default layout. Each profile can hide both SketchUp controls; their
buttons are removed and the remaining controls close the gaps. Hiding an active
SketchUp selection/lock resets the click action to left click.
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
Physical mouse actions remain available. The second main-window button, immediately
right of click Play/Pause, asks for confirmation before stopping webcam movement and
clicks, cancelling pending input and resetting motion history. The modeless dialog
keeps head movement and its confirmation/cancel dwell controls usable until confirmed,
even when ordinary clicks are paused. Cancel or closing the dialog changes nothing.
Camera preview continues. Dwell is suspended during full pause, preventing an
immediate unintended resume under the stationary pointer. Resume with a physical
click followed by confirmation, or the configurable full-pause hotkey (F8 by default).
The hotkey acts immediately and closes any pending confirmation. The previous click-only
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

The optional black cursor with yellow outline is below click sound in Click settings.
Feedback is active only while an injected mouse button is down, including a held
drag. Pending actions and gaps between multiple clicks leave the Windows pointer
alone. Cancellation, pause, profile changes and shutdown release held input and
stop the overlay. There is no additional minimum duration for the highlight.

A dedicated thread owns the unowned, topmost, non-activating, click-through layered
window, its bitmap and all visibility transitions. It supplies premultiplied alpha
with UpdateLayeredWindow and follows GetCursorPos independently of the main UI's
message pump. The standard arrow silhouette, bitmap size and hotspot are retained;
its border uses the shared toolbar RGB(255, 230, 0) constant. After presentation
and DwmFlush succeed, MagShowSystemCursor suppresses native drawing only during
the press. On release the renderer restores native visibility before hiding the
overlay. It does not call SetSystemCursor, reload a cursor scheme, change cursor
roles or write cursor preferences. At rest it makes no cursor visibility calls.
If presentation fails or trusted system UI covers the overlay, feedback falls
back to the normal pointer until the next press.

A hidden guard receives an armed/disarmed byte through a pipe. If the renderer's
process exits while armed, pipe closure restores native visibility. Normal shutdown
waits for the guard before an installer can replace the executable. This is a
visibility fallback, not a cursor scheme reset. See Microsoft's documentation for
[layered windows](https://learn.microsoft.com/en-us/windows/win32/winmsg/window-features#layered-windows)
and [MagShowSystemCursor](https://learn.microsoft.com/en-us/windows/win32/api/magnification/nf-magnification-magshowsystemcursor).
Sound feedback uses the embedded original eViacam click audio, with its alternate
sound when starting a drag.

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
window minimizes to the tray and can be restored by the robot button
at the end of the toolbar or by the tray icon. Its bounds are constrained to monitor work
area and away from the bar on move, resize, restore, DPI and toolbar changes.
Main icons use 28 logical pixels in 36-pixel buttons and click icons
use 40 in 48-pixel buttons. They share a 48-unit grid with round caps and joins,
with per-monitor DPI scaling. GDI does not antialias, so each button is painted at
four times its size and reduced with HALFTONE; compact labels and focus outlines
are drawn afterwards at the real size. Window and tray icons are loaded from the
embedded frame for the current DPI and replaced on WM_DPICHANGED. Calibration uses the same fresh-tracking readiness as
pointer control and pauses movement only during its measurement periods.

The main window is not manually resizable. Its preview dimensions are retained in
logical pixels at 96 DPI, then scaled by the current monitor DPI. Startup,
WM_GETDPISCALEDSIZE/WM_DPICHANGED and status updates share the same size calculation;
window borders and the menu use AdjustWindowRectExForDpi. A monitor's work-area
constraints never overwrite the logical baseline, so repeated monitor changes do
not accumulate rounding or preserve a temporary size reduction. The position
suggested by Windows on a DPI change is retained, subject to topbar/work-area bounds.
Status text uses native word wrapping;
its height is measured with the displayed font and available width. On a change
in line count, the window is sized again from the baseline and measured footer,
preserving the preview where monitor space permits. Camera fitting and DPI layout
also account for the measured footer. There is no ellipsis or tooltip-only text.

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

`--desktop-test --smoke-report <path>` additionally toggles Windows Show Desktop,
checks that the toolbar remains the actual hit-test target while the main window
is hidden, restores the desktop and verifies reopening the main window. Run this
only in an interactive session; it briefly rearranges that session's windows.

`--cursor-test <report.json>` requires an interactive desktop and an idle physical
mouse. It injects real presses and drags only into its own white test window, moves
within that window, then restores the original pointer position. It checks composited
black/yellow pixels and hotspot position during held clicks with the main thread
blocked, target cursor changes, repeated presses/releases, click-through behavior,
focus retention and destruction of the overlay on exit. Standard cursor role images
must remain unchanged. It saves only its own fixture and overlay. GDI captures do
not prove that a hardware cursor is visible: physical display checks in Chrome,
on the desktop and on the deployment machine remain necessary. Stop the normal
Robocam instance before this test to avoid competing input.

UI language is selected in General settings and saved application-wide; it takes
effect at the next launch. SketchUp visibility and click feedback are per-profile.

## In-app updates

The startup preference enables release checks, not unattended installation. Tools >
Check for updates opens a modeless panel. Download and install authorizes the full
download/install/restart sequence; its buttons also accept local head dwell while
ordinary desktop clicks are paused. Network work runs off the UI thread. Cancel
discards the pending download or cancels helper preparation while leaving Robocam running.

Only exact `vX.Y.Z` stable releases newer than the running package are eligible.
The installer and SHA256SUMS URLs must match this repository and tag. HTTPS is
required through redirects. Downloads are bounded by the published size (at most
128 MiB), checked for completeness and hashed before renaming the partial file.
The independent helper rechecks SHA-256 while holding a read-only sharing lock.
Its script, job and installer are staged outside the installed directory, under
Program Files, so the installer can replace the running application's files.

The helper prepares a recovery copy and signals readiness before the GUI releases
held input and exits. It waits for that process to finish, snapshots saved settings,
then runs the published installer silently into the current installation directory.
`/ROBOCAMUPDATE=1` preserves the existing startup task; `/NORESTART` prevents a
Windows reboot. See the [Inno Setup command-line documentation](https://jrsoftware.org/ishelp/topic_setupcmdline.htm).
The helper restores previous files/settings on failure and restarts with the prior
click/full-pause and visibility state. A process-matched diagnostic heartbeat verifies
the restart; result.json and install.log retain the outcome. Recovery is best effort
if Windows refuses file access; an error is reported rather than claiming success.

Run `scripts/test-update-helper.ps1` in Windows PowerShell to test checksum rejection,
pre-install cancellation and rollback/restart after a deliberately failing fixture
installer. These tests use disposable programs and profiles, not the installed app.
Release acceptance additionally requires an older updater-enabled build to discover
the published newer installer, download it, upgrade and restart with settings intact.
