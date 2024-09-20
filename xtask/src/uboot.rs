use anyhow::Result;
use network_interface::Addr;
use network_interface::NetworkInterface;
use network_interface::NetworkInterfaceConfig;
use std::{
    fs::{remove_file, File},
    io::{self, stdin, stdout, Read, Write},
    sync::{Arc, Mutex},
    thread::{self, sleep},
    time::Duration,
};

use crate::project::Project;

pub struct UBoot {}

impl UBoot {
    pub fn run(project: &Project) -> Result<()> {
        let mut config = project.config.clone();
        let uboot = config.uboot.get_or_insert_default();

        if uboot.net.is_none() {
            println!("请选择网卡：");

            let interfaces = NetworkInterface::show().unwrap();
            for (i, interface) in interfaces.iter().enumerate() {
                let addr: Vec<Addr> = interface.addr;
                println!(
                    "{}. [{}] - [{:?}]",
                    i,
                    interface.name,
                    match addr {
                        Addr::V4(v4_if_addr) => v4_if_addr.ip.to_string(),
                        Addr::V6(v6_if_addr) => v6_if_addr.ip.to_string(),
                    }
                );
            }
        }

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

        println!("使用串口： {}", port.name().unwrap_or_default());

        let config_str = toml::to_string(&config)?;

        {
            remove_file(&project.config_path)?;
            let mut file = File::create(&project.config_path)?;
            file.write_all(config_str.as_bytes())?;
        }

        let mut buf = [0u8; 1];
        let mut history = Vec::new();

        let input = Arc::new(Mutex::new(Vec::<u8>::new()));

        thread::spawn({
            let input = input.clone();
            move || loop {
                let mut buf = [0u8; 1];
                let _ = stdin().read_exact(&mut buf);
                input.lock().unwrap().push(buf[0]);
            }
        });

        let mut in_shell = false;

        println!("等待 U-Boot 启动...");

        loop {
            match port.read(&mut buf) {
                Ok(_t) => {
                    let ch = buf[0];
                    if ch == b'\n' && history.last() != Some(&b'\r') {
                        stdout().write(b"\r")?;
                        if in_shell {
                            history.clear();
                        }
                    }
                    history.push(ch);
                    if !in_shell {
                        let s = String::from_utf8(history.to_vec())?;
                        if s.contains("Hit any key to stop autoboot") {
                            in_shell = true;
                            port.write(b"a")?;
                            sleep(Duration::from_secs(1));
                            port.write_all(b"dhcp 0x90600000 10.3.10.22:phytium.dtb;fdt addr 0x90600000;bootp  10.3.10.22:kernel.bin;booti $loadaddr - 0x90600000\r\n")?;
                        }
                    }

                    stdout().write(&buf)?;
                }
                Err(ref e) if e.kind() == io::ErrorKind::TimedOut => {
                    stdout().flush()?;

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
    }
}
