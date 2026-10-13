# Third-party notices

## eViacam

robocam is a Rust reimplementation informed by Enable Viacam (eViacam), including
its pointer-motion and dwell-click behavior and interface dimensions.

- Project: <https://github.com/cmauri/eviacam>
- Audited revision: `a4032ed9c59def5399a93e74f5ea84513d2f42b1`
- Copyright: Cesar Mauri Loba / CREA Software Systems and the eViacam contributors.
- License: GNU General Public License, version 3 or any later version.

robocam is distributed under GPL-3.0-or-later; the complete license is in
`LICENSE`. The package contains a `robocam-<version>-source.zip` with the
corresponding robocam sources and build scripts. eViacam's original icons and
sounds are not included in robocam.

## OpenCV and face detector

- OpenCV 4.10.0: <https://github.com/opencv/opencv/tree/4.10.0>
- License: Apache License 2.0, with separately licensed bundled components.
- Face cascade: `data/haarcascades/haarcascade_frontalface_default.xml` from
  OpenCV; the model's original Intel license and copyright notice are retained
  within the model file.

Windows packages include the OpenCV runtime and Media Foundation camera plugin.
The package's `licenses/opencv` directory contains the OpenCV license and the
complete third-party notice collection shipped with that SDK. The model and its
notice are also included under `licenses/face-models`. No FFmpeg plugin is shipped.

## Rust dependencies

Resolved dependency versions are recorded in `Cargo.lock`. Packaging gathers
license and notice files from the resolved crates into `licenses/rust`, including
build dependencies, and emits `licenses/rust-dependencies.json` with SPDX license
expressions and upstream repository links. This includes the OpenCV Rust bindings,
Microsoft's `windows-sys` bindings, Serde and their dependencies.

## Microsoft Visual C++ runtime

Windows packages include the x64 Microsoft Visual C++ runtime libraries from the
Visual Studio redistributable CRT directory. These components remain Microsoft
software, distributed under the applicable Visual Studio redistribution terms;
they are not licensed under robocam's GPL. Windows system libraries are supplied
by Windows itself.

## Build tools

The optional bootstrap downloads OpenCV 4.10.0, Inno Setup 6.2.2 and innoextract
1.9 from their projects' official GitHub releases and verifies pinned SHA-256
hashes. These tools are extracted into `.tools` without registering/installing
them globally. Their original notices remain in the extraction directories.

- Inno Setup: <https://github.com/jrsoftware/issrc/tree/is-6_2_2>
- innoextract: <https://github.com/dscharrer/innoextract/tree/1.9>

The installer engine is supplied by Inno Setup. Its use does not change the
license of robocam or its dependencies.
