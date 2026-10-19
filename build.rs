fn main() {
    println!("cargo:rerun-if-changed=assets/icons/logo.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icons/logo.ico");
        res.compile().expect("failed to embed Windows icon");
    }
}
