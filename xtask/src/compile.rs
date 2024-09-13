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
        let _ = fs::remove_file(&project.kernel_linux_image);

        let mut image_file = fs::File::create(&project.kernel_linux_image).unwrap();

        let mut header = ImageHeader {
            magic: MAGIC,
            image_size: img_size,
            text_offset: 0x80000,
            ..Default::default()
        };

        let header_data = unsafe {
            std::slice::from_raw_parts_mut(
                &mut header as *mut _ as *mut u8,
                size_of::<ImageHeader>(),
            )
        };

        header_data[0..8].copy_from_slice(&bin_data[0..8]);

        image_file.write_all(header_data).unwrap();
        image_file.write_all(&bin_data).unwrap();

        println!("Image: {}", project.kernel_linux_image.display());

        Self {
            kernel_bin_path: image_elf_path,
        }
    }
}

const MAGIC: u32 = 0x644d5241;

/* See Documentation/arm64/booting.txt in the Linux kernel */
#[repr(C)]
#[derive(Default)]
struct ImageHeader {
    code0: u32,       /* Executable code */
    code1: u32,       /* Executable code */
    text_offset: u64, /* Image load offset, LE */
    image_size: u64,  /* Effective Image size, LE */
    flags: u64,       /* Kernel flags, LE */
    res2: u64,        /* reserved */
    res3: u64,        /* reserved */
    res4: u64,        /* reserved */
    magic: u32,       /* Magic number */
    res5: u32,
}
