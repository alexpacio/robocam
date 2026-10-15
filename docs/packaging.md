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

Application and source packages include Apache-2.0 `LICENSE`, `NOTICE`, the
changelog and `THIRD_PARTY_NOTICES.md`. The app displays its license and warranty
disclaimer under **Tools > Licenses and disclaimers** and in Help and about;
**Tools > Open dependency licenses** opens the local `licenses` collection.
English license texts remain verbatim when the interface language is Italian.

Application and installer Windows version metadata use `X.Y.Z.0`. Packaging
checks both the text and numeric resources. The updater derives `X.Y.Z` from the
numeric product version resource for installation and restart verification;
it rejects a nonzero fourth component. Version 1.0.0 requires a direct installer
upgrade from 0.x, whose updater helpers compare display-version strings.

`licenses/DEPENDENCIES.md` links to the preserved OpenCV, IPP, face-model and
installer notices. `licenses/RUST_DEPENDENCIES.md` and `rust-dependencies.json`
record all resolved crate versions, declared licenses, attribution, upstream
references and links to local license files. Packaging also preserves nested
license files and license-bearing AUTHORS/README files. Two omitted upstream
license texts are supplemented from `assets/licenses`; see its README for
versioned provenance. Packaging stops if any dependency has no license notice
or if the installer engine's original license is missing.

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

## Machine-local UIAccess

Robocam can opt into Windows' assistive-technology UIAccess privilege on a
particular PC. This requires an Authenticode signature trusted by that machine
and installation in a protected location such as Program Files. See Microsoft's
[assistive technology security requirements](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-securityoverview).
UIAccess does not grant general access to System-integrity windows or the secure
desktop. Verify the specific blocked-input scenario separately.

From an elevated **Windows PowerShell 5.1** in the user's interactive session:

```powershell
# Release held input and close cleanly. Wait until all robocam processes exit.
& "$env:ProgramFiles\robocam\robocam.exe" --request-exit

# Run once after installing version 1.1.1 or later.
& "$env:ProgramFiles\robocam\enable-uiaccess.ps1" -Provision

# Restart using the usual elevated interactive startup task or desktop shortcut.
```

Provisioning creates a SHA-256/RSA code-signing certificate valid for three years
in `Cert:\LocalMachine\My`, with a non-exportable private key. Its public
certificate is trusted in that PC's machine Root and TrustedPublisher stores.
The script enables only the embedded manifest's UIAccess attribute, signs a
temporary copy in the protected installation directory, verifies the signature,
then replaces `robocam.exe`. Existing profiles and startup preferences are retained.
The administrator-owned `HKLM\SOFTWARE\robocam\UIAccess\CertificateThumbprint`
setting selects the certificate for later installs. No certificate or private key
is placed in the repository, source archive or GitHub Release.

On a provisioned PC, the installer invokes the bundled script with `-Reapply`
after copying application files and before startup configuration or application
restart. A missing, expired or unusable configured certificate fails installation;
the automatic updater then restores its backed-up application and settings.
On other PCs this step is a no-op and the standard executable keeps
`uiAccess="false"`. Published installer checksums are verified before installation;
the installed executable is subsequently signed locally, so its hash will differ
from the executable in the downloaded archive. Portable copies are not provisioned.

Use `Get-AuthenticodeSignature "$env:ProgramFiles\robocam\robocam.exe"` to verify
the installed signature. Launch with `--diagnostic-report <path>` and confirm
`ui_access: true` in the running process report; also verify normal launch and
interactive logon startup. `null` means the token query failed. An embedded
manifest and valid signature alone do not prove Windows granted UIAccess.

Keep the machine private key available for updates. To remove this configuration,
close Robocam, remove the `HKLM\SOFTWARE\robocam\UIAccess` registry key and
reinstall the standard package before restarting. The retained public certificate
and private key may then be removed by thumbprint from the three machine stores.
For certificate expiry or loss, first restore the standard package this way, then
run `-Provision` again. Uninstall intentionally retains the signing configuration
and saved profiles so a subsequent installation can reuse them.

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
