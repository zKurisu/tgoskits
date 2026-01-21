#![cfg_attr(target_os = "none", no_main)]
#![cfg_attr(target_os = "none", no_std)]
#![cfg(not(target_os = "none"))]

fn main() {
    println!("Hello, world!");
}
