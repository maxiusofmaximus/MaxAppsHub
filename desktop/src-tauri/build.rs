//! Embeds the Hub's icon in the program file. GPUI takes the window and taskbar icon
//! from resource 1, so the ID must be "1".

fn main() {
    println!("cargo:rerun-if-changed=icons/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resource = winres::WindowsResource::new();
        resource.set_icon_with_id("icons/icon.ico", "1");
        resource.compile().expect("no pude incrustar el icono en el programa");
    }
}
