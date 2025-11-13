macro_rules! ext_sym_addr {
    ($sym:expr) => {
        {
            #[allow(unused_unsafe)]
            unsafe{
                let out: usize;
                core::arch::asm!(
                    concat!("la.pcrel    {r}, ", stringify!($sym)),
                    r = out(reg) out,
                );
                out
            }
        }
    };
}

