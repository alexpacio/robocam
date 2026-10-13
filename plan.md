PLAN

---

starting from the idea of eviacam (clone it, see https://github.com/cmauri/eviacam) I want you to recreate something similar to it, possibly in Qt6 or in native GUI, written in Rust, targetting Windows 11. It must be called "robocam".

Face motion detection should be done in OpenCV or something similar. When face moves, the mouse pointer moves accordingly. Try to mimic very closely the eviacam behaviour on this.

finally, i want you to create an installer that provides to automatically start it at the user login.

Mouse functionalities click (a button for each):

- single left click
- double left click
- triple left click
- single central mouse click
- mouse wheel drag (to scroll)
- single right click
- drag functionality -> when clicked, shouldn't be reclaimed until the cursor stops again (as if it clicks again). it can apply to already selected click -> it applies to single clicks
- ...and the other eviacam functionalities

All the other eviacam functionalities should be ported as is. Change icons to another style. Make still sure that sizes for bar, icon, window from eviacam are kept.