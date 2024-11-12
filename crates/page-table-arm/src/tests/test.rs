#[cfg(test)]
mod test {
    extern crate std;
    use page_table_arm::*;

    #[test]
    fn test_l1() {
        let pte = PTE::from_paddr(0x40000000);
    }
    #[test]
    fn test_l2() {}

    #[test]
    fn test_l3() {}
}
