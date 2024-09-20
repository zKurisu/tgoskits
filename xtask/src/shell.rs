use ansi_rgb::{magenta_pink, Foreground};
use anyhow::Result;
use std::{
    ffi::OsString,
    io::{BufRead, BufReader},
    process::{Command, Stdio},
};

pub trait Shell {
    fn exec(&mut self) -> Result<()>;
}

impl Shell for Command {
    fn exec(&mut self) -> Result<()> {
        let mut cmd_str = self.get_program().to_string_lossy().to_string();

        for arg in self.get_args() {
            cmd_str += " ";
            cmd_str += &arg.to_string_lossy().to_string();
        }

        println!("{}", cmd_str.fg(magenta_pink()));

        let mut child = self.stdout(Stdio::piped()).spawn()?;

        let stdout = BufReader::new(child.stdout.take().unwrap());
        for line in stdout.lines() {
            let line = line.expect("Failed to read line");
            // 解析输出为UTF-8
            println!("{}", line);
        }
        let out = child.wait_with_output()?;

        if !out.status.success() {
            unsafe {
                return Err(anyhow::anyhow!(
                    "{}",
                    OsString::from_encoded_bytes_unchecked(out.stderr).to_string_lossy()
                ));
            }
        }

        Ok(())
    }
}
