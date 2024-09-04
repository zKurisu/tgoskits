mod common;

// #[cfg_attr(target_arch = "aarch64", path = "arch/aarch64/mod.rs")]
// mod arch;


// #[cfg_attr("test", path = "arch/aarch64/mod.rs")]
// pub mod arch;




const fn level_idx(level: usize, addr: u64) -> usize {
    let bits = 12 + (level as u64) * 9;
    (addr >> bits & 0x1FF) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_level_idx() {
        assert_eq!(level_idx(1, 0xfff0_000_0ff_f000), 0);
    }
}


