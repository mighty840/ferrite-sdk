fn main() {
    // Generate a build ID at compile time
    let build_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    println!("cargo:rustc-env=FERRITE_BUILD_ID={}", build_id);
}
