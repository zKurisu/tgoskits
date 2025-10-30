fn main() {
    println!("cargo::rustc-link-arg=-Tlink.x");
    println!("cargo::rustc-link-arg=-znostart-stop-gc");
}
