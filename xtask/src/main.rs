#![cfg(any(windows, unix))]
#![cfg_attr(target_os = "none", no_main)]

fn main() {
    println!("Hello, world!");
}
