use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Child;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

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
        .kill_on_drop(true)
        .output()
        .await?;

    parse_command_output(
        output.status.success(),
        output.status.code(),
        &output.stdout,
        &output.stderr,
    )
}

pub async fn run_adb_command_with_timeout(args: &[&str], duration: Duration) -> AdbResult<String> {
    timeout(duration, run_adb_command(args))
        .await
        .map_err(|_| {
            AdbError::new(format!(
                "ADB command timed out after {} seconds",
                duration.as_secs()
            ))
        })?
}

pub async fn run_shell_command(device_id: &str, shell_args: &[&str]) -> AdbResult<String> {
    let mut args = vec!["-s", device_id, "shell"];
    args.extend_from_slice(shell_args);
    run_adb_command(&args).await
}

pub async fn run_shell_command_with_timeout(
    device_id: &str,
    shell_args: &[&str],
    duration: Duration,
) -> AdbResult<String> {
    timeout(duration, run_shell_command(device_id, shell_args))
        .await
        .map_err(|_| {
            AdbError::new(format!(
                "ADB shell command timed out after {} seconds",
                duration.as_secs()
            ))
        })?
}

pub async fn run_shell_raw(device_id: &str, command: &str) -> AdbResult<String> {
    run_adb_command(&["-s", device_id, "shell", command]).await
}

pub async fn run_shell_raw_with_timeout(
    device_id: &str,
    command: &str,
    duration: Duration,
) -> AdbResult<String> {
    timeout(duration, run_shell_raw(device_id, command))
        .await
        .map_err(|_| {
            AdbError::new(format!(
                "ADB shell command timed out after {} seconds",
                duration.as_secs()
            ))
        })?
}

fn parse_command_output(
    success: bool,
    exit_code: Option<i32>,
    stdout: &[u8],
    stderr: &[u8],
) -> AdbResult<String> {
    let stdout = String::from_utf8_lossy(stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(stderr).trim().to_string();

    if !success {
        let details = [stdout.as_str(), stderr.as_str()]
            .into_iter()
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n");

        let message = if details.is_empty() {
            match exit_code {
                Some(code) => format!("ADB command failed with exit code {code}"),
                None => "ADB command was terminated before it completed".to_string(),
            }
        } else {
            details
        };

        return Err(AdbError::new(message));
    }

    // ADB normally writes successful command output to stdout. Some versions emit
    // informational text on stderr, so preserve it when stdout is empty.
    if stdout.is_empty() {
        Ok(stderr)
    } else {
        Ok(stdout)
    }
}

#[cfg(test)]
mod tests {
    use super::parse_command_output;

    #[test]
    fn reports_failed_command_when_error_is_only_on_stdout() {
        let error = parse_command_output(
            false,
            Some(1),
            b"Failure [INSTALL_FAILED_VERSION_DOWNGRADE]",
            b"",
        )
        .expect_err("non-zero exit status must be an error");

        assert!(error.message.contains("INSTALL_FAILED_VERSION_DOWNGRADE"));
    }

    #[test]
    fn reports_failed_command_when_adb_produces_no_output() {
        let error = parse_command_output(false, Some(7), b"", b"")
            .expect_err("non-zero exit status must be an error");

        assert_eq!(error.message, "ADB command failed with exit code 7");
    }

    #[test]
    fn trims_successful_output() {
        let output = parse_command_output(true, Some(0), b"Success\r\n", b"")
            .expect("successful command should return its output");

        assert_eq!(output, "Success");
    }
}
