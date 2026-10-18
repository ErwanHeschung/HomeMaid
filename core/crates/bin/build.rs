fn main() {
    // Windows only: the logo as the exe's icon (resource 1), loaded again by the desktop mode.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("assets/homemaid.rc", embed_resource::NONE).manifest_optional().unwrap();
    }
}
