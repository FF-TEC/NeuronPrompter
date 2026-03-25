// Build script for the NeuronPrompter binary crate.
//
// Embeds the application icon (assets/icon.ico) into the Windows executable
// via winresource. This makes the .exe display the NeuronPrompter gradient "P"
// icon in Windows Explorer, the taskbar, and Alt+Tab. The winresource crate
// is the maintained successor to the deprecated winres crate; the API is
// identical.

fn main() {
    // Re-run this build script when the icon file changes on disk.
    println!("cargo:rerun-if-changed=assets/icon.ico");

    // Embed the application icon into the Windows executable resource
    // section. The icon.ico file contains multiple resolutions (16px through
    // 256px) so Windows can select the appropriate size for Explorer
    // thumbnails (large), taskbar pins (medium), and title bars (small).
    #[cfg(target_os = "windows")]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        if let Err(e) = res.compile() {
            println!("cargo:warning=winresource icon embedding failed: {e}");
        }
    }
}
