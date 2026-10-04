fn main() {
    // RustEmbed only tracks files that existed at the last compile, so newly
    // added assets would otherwise be missing from incremental release builds.
    println!("cargo:rerun-if-changed=../../assets");
}
