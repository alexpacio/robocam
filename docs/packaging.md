# Building and packaging on Windows

Use an x64 Windows machine with Rust's MSVC toolchain, Visual Studio C++ build
tools (including the Windows SDK), LLVM/libclang, and 7-Zip. Native dependencies
are pinned to OpenCV **4.10.0**; its official Windows SDK uses `vc16` paths and is
compatible with the x64 MSVC toolchain.

```powershell
# Downloads and extracts into .tools; no installation or elevation required.
.\scripts\bootstrap.ps1 -OpenCV -Installer

# Tests and builds a release executable.
.\scripts\build.ps1

# Builds/tests again unless -SkipBuild is specified, then packages the result.
.\scripts\package.ps1 -SkipBuild
```

`build-env.ps1` looks for `.tools\opencv`, then `C:\tools\opencv`. Set
`ROBOCAM_OPENCV_ROOT` to use another OpenCV 4.10.0 directory. Set `LIBCLANG_PATH`
to the directory containing `libclang.dll` if it is not in `C:\Program Files\LLVM\bin`.
All environment changes apply only to the current PowerShell process.

The package script discovers the x64 redistributable CRT under the latest Visual
Studio C++ installation. Override discovery using `-VCRuntimeDirectory` if needed.
It uses a local Inno compiler, an `ISCC.exe` on PATH, or downloads/extracts the
pinned compiler. `-InnoCompiler` selects an explicit executable.

Outputs in `dist`:

- `robocam-<version>-windows-x64-setup.exe`: Windows 11 installer.
- `robocam-<version>-windows-x64.zip`: portable folder with app-local DLLs.
- `robocam-<version>-source.zip`: the exact application source snapshot.
- `SHA256SUMS.txt`: checksums for these three artifacts.

The installer targets Windows build 22000 or later and installs under
`%ProgramFiles%\robocam`. It requests administrator privileges.
The **Start robocam when I sign in to Windows** task is checked by default and
registers a `robocam-<user SID>` interactive logon task with highest privileges
and `--minimized`. The application uses the same task for its startup setting.
The obsolete unelevated Run entry is removed. Unchecking startup removes the task.
Uninstall removes the task and
installed program files; application-created user profiles are retained.

The installer and portable folder bundle OpenCV's world DLL, Media Foundation
plugin, the MSVC runtime, dependency notices and the source archive. End users do
not need Rust, LLVM, the OpenCV SDK, Visual Studio or an internet connection.
Packaging smoke-tests the staged executable with SDK directories removed from
PATH, isolated settings and input/camera capture disabled. It requires the native
windows/options/toolbar to open and shut down cleanly before creating artifacts.

CI runs formatting, Clippy, tests, release compilation and installer packaging
on `windows-2022`. Its artifacts are unsigned builds. Tags named `vX.Y.Z` must
match Cargo, installer, Windows manifest and a dated changelog entry, and the
tagged commit must belong to `main`. After building that exact commit, CI uploads
the installer, portable archive, source archive and checksums to a draft GitHub
Release, then publishes it as latest. The version's changelog entry is used as
the release notes. Published tags/releases are never overwritten.
Hardware-based camera acceptance and release signing remain separate checks.
The application checks `alexpacio/robocam` GitHub releases when requested; a 404
means that no public release has been published yet.

For remote diagnostics, `--paused --diagnostic-report <path> --exit-after-ms 15000`
runs the real camera and interface for 15 seconds with desktop clicks paused;
head-driven pointer movement remains active.
Use `--fully-paused` instead to block both movement and clicks while testing the
live camera. Reports include native zoom range, current value and whether a
driver write was actually performed.
The JSON report records frame rate, tracking validity, input delivery and AppBar
geometry; it contains no camera images. Omit the time limit for an ongoing session.
`--export-default-settings <path>` writes the built-in profile configuration
without opening the camera or interface.
`--list-cameras <path>` writes device names, stable identities and backend indices.
`--camera-name "Anker PowerConf C200" --camera-zoom-report <path>` reads native
driver zoom without starting capture. Adding `--camera-zoom-increments <count>`
conditionally sets it; unsupported/out-of-range values fail explicitly.
`--capture-ui <directory>` exports BMP screenshots of robocam's native windows,
without camera capture, input injection or settings writes. Convert these losslessly
to PNG for README; it never captures the surrounding desktop or webcam frames.
`--request-exit` asks the existing instance in the same interactive session to
release held input and shut down cleanly.
`--configure-startup enable|disable` manages elevated login startup.
Normal operation requests elevation and maintains Realtime priority; diagnostic
reports include the actual elevation state and Windows priority class.

Installer behavior follows Inno Setup's
[administrative installation mode](https://jrsoftware.org/ishelp/topic_admininstallmode.htm)
and [task selection](https://jrsoftware.org/ishelp/topic_taskssection.htm).
