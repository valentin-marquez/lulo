fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=lulo.rc");
        println!("cargo:rerun-if-changed=../../assets/icons/lulo.ico");
        embed_resource::compile("lulo.rc", embed_resource::NONE)
            .manifest_optional()
            .unwrap();
    }
}
