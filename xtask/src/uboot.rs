use anyhow::Result;
use std::{
    fs::{remove_file, File},
    io::{self, stdin, stdout, Read, Write},
    process::Command,
    sync::{Arc, Mutex},
    thread::{self, sleep},
    time::Duration,
};

use crate::{project::Project, shell::Shell as _};

pub struct UBoot {}

impl UBoot {
    pub fn run(project: &Project) -> Result<()> {
        let mut config = project.config.clone();
        let uboot = config.uboot.get_or_insert_default();

        if uboot.serial.is_none() {
            let ports = serialport::available_ports().expect("No ports found!");
            println!("请选择串口设备:");
            for (i, p) in ports.iter().enumerate() {
                println!("{i}. {}", p.port_name);
            }
            let mut input = String::new();
            stdin().read_line(&mut input)?;

            let port_index = input.trim().parse::<usize>()?;
            println!("选择了 {}", ports[port_index].port_name);

            uboot.serial = Some(ports[port_index].port_name.clone());
        }

        let mut port = serialport::new(uboot.serial.clone().unwrap(), 115_200)
            .timeout(Duration::from_millis(10))
            .open()
            .expect("Failed to open port");

        let mut buf = [0u8; 1];
        let mut history = Vec::new();

        let input = Arc::new(Mutex::new(Vec::<u8>::new()));

        thread::spawn({
            let input = input.clone();
            move || loop {
                let mut buf = [0u8; 1];
                stdin().read_exact(&mut buf);
                input.lock().unwrap().push(buf[0]);
            }
        });

        let mut in_shell = false;

        loop {
            match port.read(&mut buf) {
                Ok(t) => {
                    let ch = buf[0];
                    if ch == b'\n' && history.last() != Some(&b'\r') {
                        stdout().write(b"\r");
                        if in_shell {
                            history.clear();
                        }
                    }
                    history.push(ch);
                    if !in_shell {
                        let mut s = String::from_utf8(history.to_vec())?;
                        if s.contains("Hit any key to stop autoboot") {
                            in_shell = true;
                            port.write(b"a");
                            sleep(Duration::from_secs(1));
                            port.write_all(b"dhcp 0x90600000 10.3.10.22:phytium.dtb;fdt addr 0x90600000;bootp  10.3.10.22:kernel.bin;booti $loadaddr - 0x90600000\r\n");
                        }
                    }

                    stdout().write(&buf)?;
                }
                Err(ref e) if e.kind() == io::ErrorKind::TimedOut => {
                    stdout().flush();

                    let mut input = input.lock().unwrap();
                    if input.is_empty() {
                        continue;
                    }
                    port.write(&input)?;
                    input.clear();
                }
                Err(e) => eprintln!("{:?}", e),
            }
        }

        Ok(())
    }
}
