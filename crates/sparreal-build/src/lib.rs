use std::{fmt::Display, str::FromStr};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProjectConfig {
    pub build: Build,
    pub qemu: Option<Qemu>,
    pub uboot: Option<Uboot>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Build {
    pub target: String,
    pub cpu: Option<String>,
    pub kernel_bin_name: Option<String>,
    pub hart_stack_size: Option<usize>,
    pub package: String,
    pub smp: usize,
    pub log_level: LogLevel,
}

impl Default for Build {
    fn default() -> Self {
        Self {
            target: "aarch64-unknown-none".into(),
            cpu: Some("cortex-a53".into()),
            kernel_bin_name: Some("kernel.bin".into()),
            package: "helloworld".into(),
            smp: 1,
            hart_stack_size: None,
            log_level: LogLevel::Info,
        }
    }
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            build: Default::default(),
            qemu: Some(Qemu::default()),
            uboot: None,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Qemu {
    pub machine: Option<String>,
}

impl Default for Qemu {
    fn default() -> Self {
        Self {
            machine: Some("virt".into()),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct Uboot {
    pub serial: Option<String>,
    pub net: Option<String>,
    pub dtb_file: String,
}

impl Display for ProjectConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", toml::to_string(self).unwrap())
    }
}

impl FromStr for ProjectConfig {
    type Err = toml::de::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        toml::from_str(s)
    }
}

#[derive(Debug)]
pub enum Arch {
    Aarch64,
    Riscv64,
    X86_64,
}

impl Default for Arch {
    fn default() -> Self {
        match std::env::var("CARGO_CFG_TARGET_ARCH").unwrap().as_str() {
            "aarch64" => Arch::Aarch64,
            "riscv64" => Arch::Riscv64,
            "x86_64" => Arch::X86_64,
            _ => unimplemented!(),
        }
    }
}
