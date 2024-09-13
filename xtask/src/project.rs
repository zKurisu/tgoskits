use std::{fs, io::Write, path::PathBuf};

use anyhow::Result;
use sparreal_build::ProjectConfig;

use crate::{compile::Compile, qemu::Qemu, shell::*};

pub enum Arch {
    Aarch64,
    Riscv64,
    X86_64,
}
impl Arch {
    pub fn qemu_arch(&self) -> String {
        let arch = match self {
            Arch::Aarch64 => "aarch64",
            Arch::Riscv64 => "riscv64",
            Arch::X86_64 => "x86_64",
        };

        format!("qemu-system-{}", arch)
    }

    fn from_target(target: &str) -> Result<Arch> {
        if target.contains("aarch64") {
            return Ok(Arch::Aarch64);
        }

        if target.contains("riscv64") {
            return Ok(Arch::Riscv64);
        }

        if target.contains("x86_64") {
            return Ok(Arch::X86_64);
        }

        Err(anyhow::anyhow!("Unsupportedtarget: {}", target))
    }
}

pub struct Project {
    pub target: String,
    pub kernel_bin: String,
    pub config_path: PathBuf,
    pub kernel_linux_image: PathBuf,
    pub output_dir: PathBuf,
    pub arch: Arch,
    pub config_toml: toml::Value,
    pub debug: bool,
    pub config: ProjectConfig,
    pub compile: Option<Compile>,
}

impl Project {
    pub fn new(config: Option<&str>, debug: bool) -> Result<Self> {
        let config = config.unwrap_or(".project.toml");
        if !fs::exists(config).unwrap() {
            let mut file = fs::File::create(config).unwrap();
            let config_str = ProjectConfig::default().to_string();
            file.write_all(config_str.as_bytes()).unwrap();
        }

        let config_path = std::fs::canonicalize(config).unwrap();

        let content = std::fs::read_to_string(&config_path)?;

        let config = toml::from_str(&content).unwrap();

        let tb: toml::Value = toml::from_str(&content)?;

        let build = tb["build"].as_table().unwrap();

        let target = build["target"].as_str().unwrap().to_string();

        let kernel_bin = build
            .get("kernel_bin")
            .map(|o| o.as_str().unwrap())
            .unwrap_or("kernel.bin")
            .to_string();

        let pwd = std::fs::canonicalize(".")?;

        let output_dir =
            pwd.join("target")
                .join(&target)
                .join(if debug { "debug" } else { "release" });

        let arch = Arch::from_target(&target)?;

        Ok(Self {
            target,
            kernel_bin,
            config_path,
            output_dir,
            arch,
            config_toml: tb,
            debug,
            config,
            compile: None,
            kernel_linux_image: Default::default(),
        })
    }

    pub fn build(&mut self) {
        self.compile = Some(Compile::run(self));
    }

    fn bin_path(&self) -> String {
        self.output_dir.join(&self.kernel_bin).display().to_string()
    }

    pub fn qemu(&mut self, dtb: bool) -> Qemu {
        self.build();
        Qemu::run(self, dtb)
    }
}
