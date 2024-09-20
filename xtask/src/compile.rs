use std::{path::PathBuf, process::Command};

use anyhow::Result;
use byte_unit::Byte;

use crate::{project::Project, shell::Shell as _};

#[derive(Clone)]
pub struct Compile {
    pub debug: bool,
    pub bin: PathBuf,
}

impl Compile {
    pub fn run(project: &Project, debug: bool) -> Result<Self> {
        let bin_name = project
            .config
            .build
            .kernel_bin_name
            .clone()
            .unwrap_or("kernel.bin".to_string());

        let bin_path = project.output_dir(debug).join(bin_name);

        let mut args = vec![
            "rustc",
            "-p",
            &project.config.build.package,
            "--target",
            &project.config.build.target,
        ];

        if !debug {
            args.push("--release");
        }

        Command::new("cargo")
            .args(args)
            .env(
                "RUSTFLAGS",
                "-C link-arg=-Tlink.x -C link-arg=-no-pie -C link-arg=-znostart-stop-gc",
            )
            // .env("BSP_FILE", &project.config_path)
            .exec()?;

        let elf = project
            .output_dir(debug)
            .join(&project.config.build.package)
            .display()
            .to_string();

        let _ = std::fs::remove_file("target/kernel.elf");
        std::fs::copy(&elf, "target/kernel.elf")?;

        let bin = bin_path.display().to_string();

        Command::new("rust-objcopy")
            .args(["--strip-all", "-O", "binary", &elf, &bin])
            .exec()
            .unwrap();

        let img_size = std::fs::metadata(&bin).unwrap().len();
        println!("kernel image size: {:#}", Byte::from_u64(img_size));

        Ok(Self {
            bin: bin_path,
            debug,
        })
    }
}
