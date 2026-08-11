fn main() {
    let profile = std::env::var("PROFILE").expect("Cargo must provide PROFILE to build scripts");
    println!("cargo:rustc-env=NEATXLSX_BUILD_PROFILE={profile}");
}
