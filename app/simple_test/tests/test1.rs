#![no_std]
#![no_main]
#![feature(used_with_arg)]

#[bare_test::tests]
mod tests {
    use bare_test::TestCase;

    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4)
    }

    #[test]
    fn test2() {
        assert_eq!(2 + 2, 4)
    }
}
