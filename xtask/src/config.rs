use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub build: Build,
    pub qemu: Option<Qemu>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Build {
    pub target: String,
    pub cpu: Option<String>,
    pub kernel_bin_name: Option<String>,
    pub package: String,
    pub kernel_virt_addr: String,
}

impl Default for Build {
    fn default() -> Self {
        Self {
            target: "aarch64-unknown-none".into(),
            cpu: Some("cortex-a53".into()),
            kernel_bin_name: Some("kernel.bin".into()),
            kernel_virt_addr: "0xffff_0000_0000_0000".into(),
            package: "helloworld".into(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            build: Default::default(),
            qemu: Some(Qemu::default()),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
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

impl Config {
    pub fn to_string(&self) -> String {
        toml::to_string(self).unwrap()
    }
}
