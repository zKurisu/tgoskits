use std::path::PathBuf;

use sparreal_build::Arch;

fn main() {
    let config = Config::new();

    config.gen_const();
    // // Put the linker script somewhere the linker can find it.
    config.gen_linker_script();

    println!("cargo:rustc-link-search={}", config.out_dir.display());
    println!("cargo:rerun-if-changed=Link.ld");
    // println!("cargo:rerun-if-changed=build.rs");
}

struct Config {
    // smp: usize,
    hart_stack_size: usize,
    out_dir: PathBuf,
    arch: Arch,
}

// 8MiB stack size per hart
const DEFAULT_HART_STACK_SIZE: usize = 8 * 1024 * 1024;

// const KERNEL_VADDR: u64 = 0x0;
// const KERNEL_VADDR: u64 = 0xb208_0000;
const KERNEL_VADDR: u64 = 0xffff_ff00_0008_0000;
// const KERNEL_VADDR: u64 = 0x0000_0000_0008_0000;
// const KERNEL_VADDR: u64 = 0x0000_0000_40080000;

impl Config {
    fn new() -> Self {
        let arch = Arch::default();

        Self {
            // smp: cfg.build.smp,
            hart_stack_size: DEFAULT_HART_STACK_SIZE,
            out_dir: PathBuf::from(std::env::var("OUT_DIR").unwrap()),
            arch,
        }
    }

    fn gen_linker_script(&self) {
        let output_arch = if matches!(self.arch, Arch::X86_64) {
            "i386:x86-64".to_string()
        } else if matches!(self.arch, Arch::Riscv64) {
            "riscv".to_string() // OUTPUT_ARCH of both riscv32/riscv64 is "riscv"
        } else {
            format!("{:?}", self.arch)
        };

        let ld_content = std::fs::read_to_string("Link.ld").unwrap();
        let ld_content = ld_content.replace("%ARCH%", &output_arch);
        let ld_content = ld_content.replace(
            "%KERNEL_VADDR%",
            &format!("{:#x}", KERNEL_VADDR),
            // &format!("{:#x}", self.va_offset + self.kernel_load_addr),
        );
        let ld_content =
            ld_content.replace("%STACK_SIZE%", &format!("{:#x}", self.hart_stack_size));
        let ld_content =
            ld_content.replace("%CPU_STACK_SIZE%", &format!("{:#x}", self.hart_stack_size));
        std::fs::write(self.out_dir.join("link.x"), ld_content).expect("link.x write failed");
    }

    fn gen_const(&self) {
        let const_content = format!(
            r#"

            pub const SMP: usize = {:#x};
            pub const HART_STACK_SIZE: usize = {:#x};
            "#,
            1, self.hart_stack_size
        );

        std::fs::write(self.out_dir.join("constant.rs"), const_content)
            .expect("const write failed");
    }
}

#[allow(unused)]
fn parse_int(s: &str) -> std::result::Result<u64, std::num::ParseIntError> {
    let s = s.trim().replace('_', "");
    if let Some(s) = s.strip_prefix("0x") {
        u64::from_str_radix(s, 16)
    } else if let Some(s) = s.strip_prefix("0o") {
        u64::from_str_radix(s, 8)
    } else if let Some(s) = s.strip_prefix("0b") {
        u64::from_str_radix(s, 2)
    } else {
        s.parse::<u64>()
    }
}
