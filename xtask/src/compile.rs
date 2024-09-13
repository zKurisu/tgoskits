use std::{fs, io::Write, path::PathBuf, process::Command};

use byte_unit::Byte;

use crate::{project::Project, shell::Shell as _};

pub struct Compile {
    pub kernel_bin_path: PathBuf,
}

impl Compile {
    pub fn run(project: &mut Project) -> Self {
        let image_elf_name = project
            .config
            .build
            .kernel_bin_name
            .clone()
            .unwrap_or("kernel.bin".to_string());

        let image_elf_path = project.output_dir.join(image_elf_name);
        project.kernel_linux_image = project.output_dir.join("Image");

        let mut args = vec![
            "rustc",
            "-p",
            &project.config.build.package,
            "--target",
            &project.config.build.target,
        ];

        if !project.debug {
            args.push("--release");
        }

        Command::new("cargo")
            .args(args)
            .env(
                "RUSTFLAGS",
                "-C link-arg=-Tlink.x -C link-arg=-no-pie -C link-arg=-znostart-stop-gc",
            )
            .env("BSP_FILE", &project.config_path)
            .exec()
            .unwrap();

        let elf = project
            .output_dir
            .join(&project.config.build.package)
            .display()
            .to_string();

        let _ = std::fs::remove_file("target/kernel.elf");
        std::fs::copy(&elf, "target/kernel.elf").unwrap();

        let bin = image_elf_path.display().to_string();

        Command::new("rust-objcopy")
            .args(["--strip-all", "-O", "binary", &elf, &bin])
            .exec()
            .unwrap();

        let img_size = std::fs::metadata(&bin).unwrap().len();
        println!("kernel image size: {:#}", Byte::from_u64(img_size));

        let bin_data = std::fs::read(&bin).unwrap();


        Self {
            kernel_bin_path: image_elf_path,
        }
    }
}




