use std::{
    fs::{remove_file, File},
    io::Write,
    process::Command,
};

use crate::{config::Config, project::Project, shell::Shell as _};

pub struct Qemu<'a> {
    command: Command,
    project: &'a Project,
}

impl<'a> Qemu<'a> {
    pub fn run(project: &'a Project, dtb: bool) -> Self {
        let mut machine = "virt".to_string();

        if let Some(qemu) = project.config.qemu.as_ref() {
            if let Some(m) = qemu.machine.as_ref() {
                machine = m.to_string();
            }
        }

        if dtb {
            let _ = remove_file("target/qemu.dtb");
            machine = format!("{},dumpdtb=target/qemu.dtb", machine);
        }

        let bin_path = &project.compile.as_ref().unwrap().kernel_bin_path;
        let img = bin_path.parent().unwrap().join("img");

        let bin = bin_path.display().to_string();

        let drive = format!("file={},if=none,format=raw,id=x0", img.display());
        {
            println!("img: {}", img.display());
            let _ = remove_file(&img);
            let file = File::create(&img).unwrap();
            file.set_len(16384);
        }

        let mut args = vec![
            "-nographic",
            "-machine",
            &machine,
            "-kernel",
            &bin,
            "-global",
            "virtio-mmio.force-legacy=false",
            "-drive",
            &drive,
            "-device",
            "virtio-blk-device,drive=x0",
        ];

        if let Some(cpu) = &project.config.build.cpu {
            args.push("-cpu");
            args.push(cpu);
        }

        if project.debug {
            args.push("-s");
            args.push("-S");
        }

        let mut command = Command::new(project.arch.qemu_arch());
        command.args(args);

        command.exec().unwrap();

        Self { project, command }
    }
}
