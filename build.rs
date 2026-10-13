fn main() {
    println!("cargo:rerun-if-changed=assets/robocam.ico");
    println!("cargo:rerun-if-changed=assets/app.manifest");
    #[cfg(windows)]
    {
        let mut resource = winresource::WindowsResource::new();
        resource
            .set_icon("assets/robocam.ico")
            .set_manifest_file("assets/app.manifest")
            .set("ProductName", "robocam")
            .set("FileDescription", "robocam head-controlled mouse")
            .set("OriginalFilename", "robocam.exe")
            .set("LegalCopyright", "robocam contributors. GPL-3.0-or-later.");
        resource
            .compile()
            .expect("Failed to compile Windows resources");
    }
}
