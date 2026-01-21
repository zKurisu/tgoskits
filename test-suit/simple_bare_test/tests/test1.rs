#![no_std]
#![no_main]
#![feature(used_with_arg)]

#[bare_test::tests]
mod tests {

    use bare_test::*;

    #[test]
    fn test2() {
        println!("test2 hello");
    }
}
