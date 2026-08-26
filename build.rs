fn main() {
    println!("cargo:rerun-if-changed=assets/icons/windows/icon.rc");
    println!("cargo:rerun-if-changed=assets/icons/icon.ico");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("assets/icons/windows/icon.rc", embed_resource::NONE)
            .manifest_optional()
            .expect("failed to embed the Windows application icon");
    }
}
