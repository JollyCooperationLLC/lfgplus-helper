//! Sets the helper's version, and gives the Windows .exe its icon (assets/app.ico, drawn by tools/make-icon.py) and the
//! name, publisher and version Windows shows in its properties.
//!
//! The version comes from LFGPLUS_HELPER_VERSION, which the release build sets to the commit's time
//! (year.month.day.hourminutesecond). Any other build is `dev`.

fn main() {
    println!("cargo:rerun-if-changed=assets/app.ico");
    println!("cargo:rerun-if-env-changed=LFGPLUS_HELPER_VERSION");
    let version = std::env::var("LFGPLUS_HELPER_VERSION")
        .ok()
        .filter(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit() || b == b'.'))
        .unwrap_or_else(|| "dev".into());
    println!("cargo:rustc-env=LFGPLUS_HELPER_VERSION={version}");
    #[cfg(windows)]
    {
        let mut resource = winresource::WindowsResource::new();
        resource
            .set_icon("assets/app.ico")
            .set("ProductName", "LFG+ Helper")
            .set("FileDescription", "LFG+ Helper")
            .set("ProductVersion", &version)
            .set("CompanyName", "Jolly Cooperation LLC")
            .set("LegalCopyright", "All rights reserved - Jolly Cooperation LLC");
        if let Err(e) = resource.compile() {
            panic!("couldn't add the icon to the .exe: {e}");
        }
    }
}
