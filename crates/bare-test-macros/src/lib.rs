extern crate proc_macro;
#[macro_use]
extern crate quote;
extern crate core;
extern crate proc_macro2;
#[macro_use]
extern crate syn;

use proc_macro::TokenStream;

/// Setup the test entry point.
/// # Example
///
///```rust
///#![no_std]
///#![no_main]
///#![feature(custom_test_frameworks)]
///#![test_runner(bare_test::test_runner)]
///#![reexport_test_harness_main = "test_main"]
///
///bare_test::test_setup!();
///```
#[proc_macro]
pub fn test_setup(_input: TokenStream) -> TokenStream {
    quote! {

    #[bare_test::entry]
    fn main() {
        test_main();
    }


        }
    .into()
}

#[proc_macro]
pub fn build_test_setup(_input: TokenStream) -> TokenStream {
    quote! {
    println!("cargo::rustc-link-arg-tests=-Tlink.x");
    println!("cargo::rustc-link-arg-tests=-no-pie");
    println!("cargo::rustc-link-arg-tests=-znostart-stop-gc");
    }
    .into()
}
