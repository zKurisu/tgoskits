# Bare Test

A test framework for testing the bare metal.

## Usage

1. Install `ostool`.

    ```shell
    cargo install ostool
    ```

2. setup `.cargo/config.toml`

    ```toml
    [target.'cfg(all(target_os = "none"))']
    runner = "ostool cargo-test"
    [build]
    target = "aarch64-unknown-none"
    ```

3. setup `cargo.toml`.

    ```toml
    [dev-dependencies]
    bare-test = "0.2"

    [build-dependencies]
    bare-test-macros = "0.2"
    ```

4. setup `build.rs`.

    ```rust
    fn main() {
        bare_test_macros::build_test_setup!();
    }
    ```

5. new `tests` dir and add `tests.rs`.

    ```rust
    #![no_std]
    #![no_main]
    #![feature(used_with_arg)]

    #[bare_test::tests]
    mod tests {

        #[test]
        fn it_works() {
            assert_eq!(2 + 2, 4)
        }

        #[test]
        fn test2() {
            assert_eq!(2 + 2, 4)
        }
    }
    ```

6. run `cargo test --test tests --  --show-output`.

7. for uboot board test:

```sh
cargo test --test tests --  --show-output --uboot
```
