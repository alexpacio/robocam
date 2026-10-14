# Third-party notices

Robocam's original code is Apache-2.0. Dependencies retain their own copyrights,
licenses and disclaimers; this document does not relicense them or imply
endorsement by their authors. Original license texts control over these summaries.
The application and source packages include the `licenses` directory referenced
below. In Robocam, use **Tools > Open dependency licenses** to access it offline.

## OpenCV

- OpenCV 4.10.0: [upstream source](https://github.com/opencv/opencv/tree/4.10.0).
- License: [Apache License 2.0](https://github.com/opencv/opencv/blob/4.10.0/LICENSE).
- Packaged text: `licenses/opencv/LICENSE.txt`.
- Bundled components retain their separate licenses. Their original notices are
  preserved in `licenses/opencv/third-party`; `licenses/DEPENDENCIES.md` links
  to every file in that collection, including libraries used by other OpenCV modules.

Windows packages include `opencv_world4100.dll` and the Media Foundation camera
plugin. No FFmpeg plugin is shipped; the SDK's FFmpeg notices are retained as part
of its complete notice collection, without claiming that FFmpeg is distributed.

## Intel Integrated Performance Primitives (IPP)

The pinned OpenCV Windows runtime contains **Intel IPP 2021.11.0 (IPP-ICV)** and
**IPP IW 2021.11.0**. These are separate from the Intel-licensed face model below.

- Copyright (C) 1997 Intel Corporation.
- License: **Intel Simplified Software License, Version October 2022**.
- Controlling packaged texts: `licenses/opencv/third-party/ippicv-EULA.rtf` and
  `licenses/opencv/third-party/ippiw-EULA.rtf`.
- Additional original notices: the adjacent `ippicv-readme.htm`,
  `ippicv-third-party-programs.txt`, `ippiw-support.txt` and
  `ippiw-third-party-programs.txt` files.
- [Intel's official licensing guidance](https://www.intel.com/content/www/us/en/developer/articles/tool/intel-integrated-performance-primitives-faq.html)
  confirms that the license bundled with the downloaded component controls.

IPP is proprietary software, not Apache-2.0 software. Its license permits use and
redistribution of unmodified binaries with the copyright notice and terms retained.
It prohibits reverse engineering, decompilation, disassembly and modification,
and prohibits using Intel's or its suppliers' names to endorse derived products
without permission. Its original warranty disclaimer, liability limitations and
other conditions remain applicable. See the bundled EULAs for the complete terms.

## Face detector

The `haarcascade_frontalface_default.xml` model was created by Rainer Lienhart and
comes from [OpenCV 4.10.0](https://github.com/opencv/opencv/blob/4.10.0/data/haarcascades/haarcascade_frontalface_default.xml).
Its XML header contains the **Intel License Agreement for Open Source Computer
Vision Library**, Copyright (C) 2000 Intel Corporation.

The model and original license/disclaimer are preserved in
`licenses/face-models/haarcascade_frontalface_default.xml` and in the source asset.
Redistribution requires retaining its copyright, conditions and disclaimer, and
Intel's name cannot be used for endorsement without permission. This model's
license is distinct from the proprietary IPP license.

## Rust dependencies

`Cargo.lock` records the resolved versions. The following are Robocam's direct
dependencies; links cite their versioned upstream license texts:

| Dependency | Version | License references |
| --- | --- | --- |
| serde | 1.0.229 | [MIT](https://docs.rs/crate/serde/1.0.229/source/LICENSE-MIT) OR [Apache-2.0](https://docs.rs/crate/serde/1.0.229/source/LICENSE-APACHE) |
| serde_json | 1.0.154 | [MIT](https://docs.rs/crate/serde_json/1.0.154/source/LICENSE-MIT) OR [Apache-2.0](https://docs.rs/crate/serde_json/1.0.154/source/LICENSE-APACHE) |
| semver | 1.0.28 | [MIT](https://docs.rs/crate/semver/1.0.28/source/LICENSE-MIT) OR [Apache-2.0](https://docs.rs/crate/semver/1.0.28/source/LICENSE-APACHE) |
| sha2 | 0.10.9 | [MIT](https://docs.rs/crate/sha2/0.10.9/source/LICENSE-MIT) OR [Apache-2.0](https://docs.rs/crate/sha2/0.10.9/source/LICENSE-APACHE) |
| windows | 0.62.2 | [MIT](https://docs.rs/crate/windows/0.62.2/source/license-mit) OR [Apache-2.0](https://docs.rs/crate/windows/0.62.2/source/license-apache-2.0) |
| windows-sys | 0.61.2 | [MIT](https://docs.rs/crate/windows-sys/0.61.2/source/license-mit) OR [Apache-2.0](https://docs.rs/crate/windows-sys/0.61.2/source/license-apache-2.0) |
| ureq | 3.4.2 | [MIT](https://docs.rs/crate/ureq/3.4.2/source/LICENSE-MIT) OR [Apache-2.0](https://docs.rs/crate/ureq/3.4.2/source/LICENSE-APACHE) |
| opencv (Rust bindings) | 0.101.1 | [MIT](https://docs.rs/crate/opencv/0.101.1/source/LICENSE) |
| winresource (build dependency) | 0.1.34 | [MIT](https://docs.rs/crate/winresource/0.1.34/source/LICENSE) |

The complete dependency list, including transitive and build dependencies and
target-specific packages resolved by Cargo, is generated during packaging:

- `licenses/RUST_DEPENDENCIES.md`: exact versions, declared license expressions,
  upstream crate/repository links and links to each preserved license/notice file.
- `licenses/rust-dependencies.json`: the same metadata and license-file paths in
  machine-readable form, including original Cargo author attribution.
- `licenses/rust/<name>-<version>/`: original license texts, nested notices,
  license-bearing AUTHORS and README files, plus reviewed supplements when the
  crate archive omits license texts. See `assets/licenses/README.md` in the source.

These include `ring` (Apache-2.0 AND ISC, with its original additional notices),
`webpki-roots` 1.0.9 ([CDLA-Permissive-2.0](https://docs.rs/crate/webpki-roots/1.0.9/source/LICENSE)),
and Unicode data notices. `OR` denotes alternative licenses; `AND` denotes
combined requirements. For winsplit 0.1.0, the Apache-2.0 option is selected.

## Microsoft Visual C++ runtime

Windows packages include x64 Microsoft Visual C++ runtime libraries from Visual
Studio's redistributable CRT directory. Copyright and other rights remain with
Microsoft. These proprietary components are not licensed under Robocam's Apache
license. Windows system libraries are supplied by Windows itself.

Distribution is subject to the applicable
[Microsoft Software License Terms](https://visualstudio.microsoft.com/license-terms/)
and [Visual C++ redistribution rules](https://learn.microsoft.com/en-us/cpp/windows/redistributing-visual-cpp-files?view=msvc-170).
Use the terms and REDIST list for the Visual Studio edition/version supplying the
runtime. No Microsoft endorsement is implied.

## Build tools and installer engine

The optional bootstrap downloads pinned tools from official releases and verifies
their SHA-256 hashes. Tool notices remain in their extraction directories:

- [Inno Setup 6.2.2 license](https://github.com/jrsoftware/issrc/blob/is-6_2_2/license.txt).
  The installer engine is supplied by Inno Setup; its use does not change the
  license of Robocam or its dependencies. Its original license is also included
  in the package under `licenses/installer/license.txt`.
- [innoextract 1.9 license](https://github.com/dscharrer/innoextract/blob/1.9/LICENSE).
  This is a build-time extraction tool, not an application runtime dependency.
