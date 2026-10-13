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

- `robocam-<version>-windows-x64-setup.exe`: per-user Windows 11 installer.
- `robocam-<version>-windows-x64.zip`: portable folder with app-local DLLs.
- `robocam-<version>-source.zip`: the exact application source snapshot.
- `SHA256SUMS.txt`: checksums for these three artifacts.

The installer targets Windows build 22000 or later and installs under
`%LOCALAPPDATA%\Programs\robocam`. It does not request administrator privileges.
The **Start robocam when I sign in to Windows** task is checked by default and
creates the `robocam` value in the current user's `Software\Microsoft\Windows\CurrentVersion\Run`
registry key with `--minimized`. The application uses the same entry for its
startup setting. Unchecking this task on an upgrade removes the previous entry.
Uninstall removes the entry and
installed program files; application-created user profiles are retained.

The installer and portable folder bundle OpenCV's world DLL, Media Foundation
plugin, the MSVC runtime, dependency notices and the source archive. End users do
not need Rust, LLVM, the OpenCV SDK, Visual Studio or an internet connection.
Packaging smoke-tests the staged executable with SDK directories removed from
PATH, isolated settings and input/camera capture disabled. It requires the native
windows/options/toolbar to open and shut down cleanly before creating artifacts.

CI runs formatting, Clippy, tests, release compilation and installer packaging
on `windows-2022`. Its artifacts are unsigned builds. Release signing, publishing
GitHub releases and hardware-based camera acceptance are separate release steps.
The application checks `alexpacio/robocam` GitHub releases when requested; a 404
means that no public release has been published yet.

Installer behavior follows Inno Setup's
[non-administrative mode](https://jrsoftware.org/ishelp/topic_admininstallmode.htm)
and [task selection](https://jrsoftware.org/ishelp/topic_taskssection.htm).
