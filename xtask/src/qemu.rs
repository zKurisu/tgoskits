use anyhow::Result;
use core::str;
use std::{
    fs::{remove_file, File},
    io::Write,
    process::Command,
};

use crate::{project::Project, shell::Shell as _, QemuArgs};

pub struct Qemu {}

impl Qemu {
    pub fn run(project: &Project, cli: QemuArgs) -> Result<()> {
        let mut machine = "virt".to_string();

        if let Some(qemu) = project.config.qemu.as_ref() {
            if let Some(m) = qemu.machine.as_ref() {
                machine = m.to_string();
            }
        }

        if cli.dtb {
            let _ = remove_file("target/qemu.dtb");
            machine = format!("{},dumpdtb=target/qemu.dtb", machine);
        }

        let bin_path = &project.compile.as_ref().unwrap().bin;
        let img = bin_path.parent().unwrap().join("img");

        let bin = bin_path.display().to_string();

        let drive = format!("file={},if=none,format=raw,id=x0", img.display());
        {
            println!("img: {}", img.display());
            let _ = remove_file(&img);
            let file = File::create(&img).unwrap();
            file.set_len(16384).unwrap();
        }

        let mut args = vec![
            "-nographic",
            "-machine",
            &machine,
            "-kernel",
            &bin,
            // "-dtb",
            // "target/bcm2711-rpi-4-b.dtb"
            // "-global",
            // "virtio-mmio.force-legacy=false",
            // "-drive",
            // &drive,
            // "-device",
            // "virtio-blk-device,drive=x0",
        ];

        if let Some(cpu) = &project.config.build.cpu {
            args.push("-cpu");
            args.push(cpu);
        }

        if cli.debug {
            args.push("-s");
            args.push("-S");
        }

        let mut command = Command::new(project.arch.qemu_arch());
        command.args(args);

        command.exec().unwrap();
        Ok(())
    }
}