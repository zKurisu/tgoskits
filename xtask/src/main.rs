#![cfg_attr(all(target_os = "none", not(test)), no_std)]
#![cfg_attr(all(target_os = "none", not(test)), no_main)]

#[cfg(not(target_os = "none"))]
mod cli;

#[cfg(target_os = "none")]
mod dump;

#[cfg(not(target_os = "none"))]
fn main() {
    cli::main();
}
