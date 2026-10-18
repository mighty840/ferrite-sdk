fn main() {
    // Linker args here, not in .cargo/config.toml, so builds from the repo
    // root (`--manifest-path`, as CI does) link the same way.
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rustc-link-search={dir}");
    println!("cargo:rustc-link-arg=-Tlink.x");
    println!("cargo:rerun-if-changed=memory.x");
}
