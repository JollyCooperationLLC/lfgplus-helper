//! Gives the Windows .exe its icon (assets/app.ico, drawn by tools/make-icon.py) and the name and publisher Windows
//! shows in its properties.

fn main() {
    println!("cargo:rerun-if-changed=assets/app.ico");
    #[cfg(windows)]
    {
        let mut resource = winresource::WindowsResource::new();
        resource
            .set_icon("assets/app.ico")
            .set("ProductName", "LFG+ Helper")
            .set("FileDescription", "LFG+ Helper")
            .set("CompanyName", "Jolly Cooperation LLC")
            .set("LegalCopyright", "All rights reserved - Jolly Cooperation LLC");
        if let Err(e) = resource.compile() {
            panic!("couldn't add the icon to the .exe: {e}");
        }
    }
}
