//! Gives the Windows executable DeadTune's mark as its file icon.

#[allow(dead_code)]
#[path = "src/mark.rs"]
mod mark;

fn main() {
    println!("cargo:rerun-if-changed=src/mark.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let ico = out.join("deadtune.ico");
    std::fs::write(&ico, mark::ico()).unwrap();
    let rc = out.join("deadtune.rc");
    let ico = ico.display().to_string().replace('\\', "/");
    std::fs::write(&rc, format!("1 ICON \"{ico}\"\n")).unwrap();
    embed_resource::compile(&rc, embed_resource::NONE)
        .manifest_optional()
        .unwrap();
}
