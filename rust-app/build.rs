fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../assets/app.ico");
        res.set("FileDescription", "Grain Size Determine (Rust)");
        res.set("ProductName", "Grain Size Determine");
        let _ = res.compile();
    }
}
