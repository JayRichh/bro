//! Embeds the app icon and version info in `kete.exe` (and its `bro.exe`
//! alias) on Windows. Other platforms carry their icon in the installer,
//! not the binary.

fn main() {
    #[cfg(windows)]
    {
        const ICON: &str = "../../assets/brand/kete/kete.ico";
        println!("cargo:rerun-if-changed={ICON}");
        let mut res = winresource::WindowsResource::new();
        res.set_icon(ICON)
            .set("ProductName", "kete")
            .set("FileDescription", "kete: sweet as agent management");
        if let Err(e) = res.compile() {
            println!("cargo:warning=app icon not embedded: {e}");
        }
    }
}
