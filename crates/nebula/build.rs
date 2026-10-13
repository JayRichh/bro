//! Embeds the app icon and version info in `bro.exe` on Windows. Other
//! platforms carry their icon in the installer, not the binary.

fn main() {
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=../../assets/brand/bro.ico");
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/brand/bro.ico")
            .set("ProductName", "bro")
            .set("FileDescription", "bro — mission control for coding agents");
        if let Err(e) = res.compile() {
            println!("cargo:warning=app icon not embedded: {e}");
        }
    }
}
