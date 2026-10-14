fn main() {
    println!("cargo:rerun-if-changed=assets/robocam.ico");
    println!("cargo:rerun-if-changed=assets/app.manifest");
    #[cfg(windows)]
    {
        let windows_version = format!("{}.0", env!("CARGO_PKG_VERSION"));
        let mut resource = winresource::WindowsResource::new();
        resource
            .set_icon("assets/robocam.ico")
            .set_manifest_file("assets/app.manifest")
            .set("ProductName", "robocam")
            .set("FileDescription", "robocam head-controlled mouse")
            .set("OriginalFilename", "robocam.exe")
            .set("FileVersion", &windows_version)
            .set("ProductVersion", &windows_version)
            .set(
                "Comments",
                "Author: Alessandro Bolletta. Apache License 2.0. See LICENSE and THIRD_PARTY_NOTICES.md.",
            );
        resource
            .compile()
            .expect("Failed to compile Windows resources");
    }
}
