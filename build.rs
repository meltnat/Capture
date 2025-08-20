fn main() {
    println!("cargo:rustc-link-lib=libcrypto");
    println!("cargo:rustc-link-lib=libssl");
    println!("cargo:rustc-link-lib=libx264");
    println!("cargo:rustc-link-lib=mfuuid");
    println!("cargo:rustc-link-lib=ssh");
    println!("cargo:rustc-link-lib=strmiids");
}
