use std::{fs, path::PathBuf, str::FromStr};

use sparreal_build::{Arch, ProjectConfig};

fn main() {
    println!("cargo:rerun-if-env-changed=BSP_FILE");

    let config_path = std::env::var("BSP_FILE").unwrap_or_else(|_| {
        println!("env BSP_FILE not set, use default config");

        let mf = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());

        mf.parent()
            .unwrap()
            .parent()
            .unwrap()
            .join(".project.toml")
            .display()
            .to_string()
    });

    let config = Config::new(config_path);

    // config.cfg_arch();
    // config.cfg_mmu();

    config.gen_const();
    // // Put the linker script somewhere the linker can find it.
    config.gen_linker_script();

    println!("cargo:rustc-link-search={}", config.out_dir.display());
    println!("cargo:rerun-if-changed=Link.ld");
    // println!("cargo:rerun-if-changed=build.rs");
}

struct Config {
    smp: usize,
    hart_stack_size: usize,
    out_dir: PathBuf,
    arch: Arch,
}

// 8MiB stack size per hart
const DEFAULT_HART_STACK_SIZE: usize = 8 * 1024 * 1024;

// const KERNEL_VADDR: u64 = 0xb200_0000;
const KERNEL_VADDR: u64 = 0xffff_ff00_0008_0000;
// const KERNEL_VADDR: u64 = 0x0000_0000_0008_0000;
// const KERNEL_VADDR: u64 = 0x0000_0000_40080000;

impl Config {
    fn new(config_path: String) -> Self {
        let s = fs::read_to_string(&config_path)
            .unwrap_or_else(|_| panic!("Config file not found in {}", &config_path));

        let cfg = ProjectConfig::from_str(&s).unwrap();
        let arch = Arch::default();

        Self {
            smp: cfg.build.smp,
            hart_stack_size: cfg.build.hart_stack_size.unwrap_or(DEFAULT_HART_STACK_SIZE),
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
        let ld_content = ld_content.replace(
            "%STACK_SIZE%",
            &format!("{:#x}", self.hart_stack_size * self.smp),
        );
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
            self.smp, self.hart_stack_size
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
