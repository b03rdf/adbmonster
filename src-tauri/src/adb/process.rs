use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Child;
use tokio::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use crate::error::{AdbError, AdbResult};

const CREATE_NO_WINDOW: u32 = 0x08000000;

pub fn prepare_command(adb_path: &std::path::Path) -> Command {
    let mut cmd = Command::new(adb_path);
    #[cfg(windows)]
    cmd.as_std_mut().creation_flags(CREATE_NO_WINDOW);
    cmd
}

#[allow(dead_code)]
pub struct AdbProcess {
    child: Child,
}

#[allow(dead_code)]
impl AdbProcess {
    pub fn spawn(args: &[&str]) -> AdbResult<Self> {
        let adb_path = super::manager::find_adb()?;
        let child = prepare_command(&adb_path)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;
        Ok(Self { child })
    }

    pub fn take_stdout(&mut self) -> Option<tokio::process::ChildStdout> {
        self.child.stdout.take()
    }

    pub async fn kill(&mut self) -> AdbResult<()> {
        self.child.kill().await?;
        Ok(())
    }

    pub async fn wait(&mut self) -> AdbResult<std::process::ExitStatus> {
        Ok(self.child.wait().await?)
    }
}

#[allow(dead_code)]
pub struct AdbLineReader {
    reader: tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
}

#[allow(dead_code)]
impl AdbLineReader {
    pub fn new(stdout: tokio::process::ChildStdout) -> Self {
        Self {
            reader: BufReader::new(stdout).lines(),
        }
    }

    pub async fn next_line(&mut self) -> Option<AdbResult<String>> {
        match self.reader.next_line().await {
            Ok(Some(line)) => Some(Ok(line)),
            Ok(None) => None,
            Err(e) => Some(Err(e.into())),
        }
    }
}

pub async fn run_adb_command(args: &[&str]) -> AdbResult<String> {
    let adb_path = super::manager::find_adb()?;
    let output = prepare_command(&adb_path)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if !stderr.is_empty() {
            return Err(AdbError::new(stderr));
        }
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub async fn run_shell_command(device_id: &str, shell_args: &[&str]) -> AdbResult<String> {
    let mut args = vec!["-s", device_id, "shell"];
    args.extend_from_slice(shell_args);
    run_adb_command(&args).await
}

pub async fn run_shell_raw(device_id: &str, command: &str) -> AdbResult<String> {
    run_adb_command(&["-s", device_id, "shell", command]).await
}
