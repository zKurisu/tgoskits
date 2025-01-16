use std::{fs, path::PathBuf};

fn main() {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let link = "test_case_link.ld";

    fs::copy("link.ld", out_dir.join(link));

    println!("cargo:rustc-link-search={}", out_dir.display());
    println!("cargo:rerun-if-changed=test_case_link.ld");
}
