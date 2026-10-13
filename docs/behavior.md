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

## Clicking and pause

The first six toolbar actions are single left, double left, triple left, drag,
middle and scroll. The SketchUp action follows, then pause, no-click, right click,
main window, keyboard, configuration, setup and help. Icons have three mouse buttons
and a separate wheel; the relevant button or wheel is red. Drag has four arrows.

Pause disables desktop clicks, releases held buttons and cancels pending actions.
Camera acquisition and head-driven pointer movement continue. Dwelling on Play
still resumes clicking. Paused mode cannot generate other toolbar dwell actions.
Physical mouse actions remain available.

Normal selection is temporary; selecting it again locks it. Fast mode selects on
hover and locks on activation. Drag holds the selected single mouse button until
movement followed by another dwell. Scrolling freezes the pointer and maps head
motion to wheel input until the next moved-then-stopped dwell. There is no cursor
progress overlay. Zero-delay clicks are dispatched in the frame that completes
the dwell countdown.

SketchUp mode checks the application under the pointer, activates its Select tool
with Space, then sends a paced left double click at a fixed position. If foreground
focus changes during the sequence, pending input is cancelled. It uses SketchUp's
default Space shortcut and is intended for the model viewport.

## Runtime and startup

Normal operation requests administrator privileges to inject input into elevated
desktop applications. Login startup is an interactive, highest-privilege scheduled
task for the current user. Runtime priority is Realtime and is rechecked each second;
OpenCV internal parallelism is limited to one thread. Realtime scheduling does not
guarantee responsiveness when camera drivers or other hardware stall.

The toolbar is a Shell AppBar. Top/bottom docking spans monitor width and reserves
desktop work area; buttons start at the left. Hiding/removing the bar releases its
reservation. Main icons use 32 logical pixels and normal click icons use 48, with
per-monitor DPI scaling. Calibration uses the same fresh-tracking readiness as
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

Unit tests cover timing, selection, gesture actions, device identity, persistence,
version checks and real OpenCV tracking on synthetic frames. Package smoke tests
cover native startup and shutdown without input injection. Hardware, target-app
behavior and monitor combinations also require runtime checks.
