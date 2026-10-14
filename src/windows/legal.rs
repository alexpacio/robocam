//! Offline license display. Authoritative license texts are never translated.
use super::*;

pub(super) const DISCLAIMER: &str = "Except as required by applicable law or agreed in writing, Robocam is provided AS IS, without warranties or conditions of any kind. Contributors' liability is limited to the extent permitted by law; see sections 7 and 8 of the Apache license.";

const REFERENCES: &str = concat!(
    "DEPENDENCY LICENSE REFERENCES\n\n",
    "OpenCV 4.10.0: Apache License 2.0\n",
    "https://github.com/opencv/opencv/blob/4.10.0/LICENSE\n",
    "Local copy: licenses/opencv/LICENSE.txt\n\n",
    "Intel IPP (IPP-ICV and IPP IW) 2021.11.0: Intel Simplified Software License, October 2022\n",
    "Copyright (C) 1997 Intel Corporation\n",
    "https://www.intel.com/content/www/us/en/developer/articles/tool/intel-integrated-performance-primitives-faq.html\n",
    "Local copies: licenses/opencv/third-party/ippicv-EULA.rtf and ippiw-EULA.rtf\n\n",
    "Face detector: Intel License Agreement for Open Source Computer Vision Library\n",
    "Copyright (C) 2000 Intel Corporation; created by Rainer Lienhart\n",
    "https://github.com/opencv/opencv/blob/4.10.0/data/haarcascades/haarcascade_frontalface_default.xml\n",
    "Local copy: licenses/face-models/haarcascade_frontalface_default.xml (license in XML header)\n\n",
    "Rust dependencies: see licenses/RUST_DEPENDENCIES.md for every resolved version, original license file and upstream reference.\n",
    "Direct dependencies: serde, serde_json, semver, sha2, windows, windows-sys and ureq (MIT OR Apache-2.0), opencv Rust bindings and winresource (MIT).\n",
    "The index also covers transitive/build dependencies, including ring (Apache-2.0 AND ISC), webpki-roots (CDLA-Permissive-2.0) and Unicode notices.\n\n",
    "Microsoft Visual C++ runtime: Microsoft Software License Terms\n",
    "https://visualstudio.microsoft.com/license-terms/\n\n",
    "Inno Setup installer engine: Inno Setup license\n",
    "https://github.com/jrsoftware/issrc/blob/is-6_2_2/license.txt\n",
    "Local copy: licenses/installer/license.txt\n\n",
    "See THIRD_PARTY_NOTICES.md for more attribution and license citations. Original dependency terms and disclaimers control over these summaries.\n\n"
);

pub(super) unsafe fn show(owner: HWND) {
    let content = format!(
        "robocam {}\n\n{}\n\n{}\n\n{}\n\n{}\n\n{}\n\n{}",
        env!("CARGO_PKG_VERSION"),
        crate::i18n::translate("Author: Alessandro Bolletta. Apache License 2.0."),
        crate::i18n::translate(DISCLAIMER),
        crate::i18n::translate(
            "Third-party components retain their own licenses and disclaimers, including proprietary Intel IPP and Microsoft runtime libraries."
        ),
        crate::i18n::translate(
            "Use Tools > Open dependency licenses to read the original dependency license files offline. The authoritative license texts below are in English."
        ),
        REFERENCES,
        concat!(
            include_str!("../../NOTICE"),
            "\n\n",
            include_str!("../../LICENSE")
        )
    );
    messages::show_verbatim(owner, "Licenses and disclaimers", &content);
}

pub(super) unsafe fn open_files(owner: HWND) {
    let directory = std::env::current_exe()
        .ok()
        .and_then(|executable| executable.parent().map(|parent| parent.join("licenses")));
    if let Some(directory) = directory.filter(|directory| directory.is_dir()) {
        let result = ShellExecuteW(
            owner,
            wide_raw("open").as_ptr(),
            wide_raw(&directory.to_string_lossy()).as_ptr(),
            null(),
            null(),
            SW_SHOWNORMAL,
        );
        if result as usize > 32 {
            return;
        }
    }
    message(
        owner,
        "Could not open the dependency license directory. Installed and portable packages include a licenses folder beside robocam.exe; see THIRD_PARTY_NOTICES.md for upstream references.",
    );
}
