use crate::cache_file::{get_config, template_config_replacement};
use crate::log;
use colored::Colorize;
use shell_words;
use std::ffi;
use std::io;
use std::io::BufWriter;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::Duration;
use std::time::Instant;

pub enum ExecutionInput {
    InheritFromTerminal,
    CustomInput(String),
}

pub enum ExecutionStatus {
    Successful {
        output: Output,
        time_elapsed: Duration,
    },
    NeedRecompilation,
    Failed(String),
}

pub fn recompile_binary(src_path: &Path) -> Result<(), String> {
    let file_type = src_path
        .extension()
        .and_then(ffi::OsStr::to_str)
        .ok_or_else(|| "Failed to determine file type".to_string())?;

    let mut config = get_config().map_err(|err| format!("Config error: {}", err))?;

    let mut sys_call = config
        .languages_config
        .remove(file_type)
        .ok_or_else(|| format!("File type \"{}\" is not supported.", file_type))?;

    template_config_replacement(&mut sys_call, config.binary_dir_path.as_path(), src_path)
        .map_err(|err| format!("Template error: {}", err))?;

    let sys_call: Vec<String> =
        shell_words::split(&sys_call).map_err(|_| "Failed to parse command".to_string())?;

    if sys_call.is_empty() {
        return Err("System call command is empty".to_string());
    }

    let command = &sys_call[0];
    let args = &sys_call[1..];

    let output = Command::new(command)
        .args(args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .output()
        .map_err(|err| format!("Command execution error: {}", err))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(format!("Failed to compile {:?}", src_path))
    }
}

pub fn execute_binary(
    binary_dir_path: &Path,
    filename: &str,
    input: ExecutionInput,
) -> Result<ExecutionStatus, io::Error> {
    let binary_name = if cfg!(windows) {
        format!("{filename}.exe")
    } else {
        format!("{filename}.out")
    };

    let binary_path = binary_dir_path.join(&binary_name);

    if !binary_path.exists() {
        return Ok(ExecutionStatus::NeedRecompilation);
    }

    let now = Instant::now();

    let mut child = Command::new(&binary_path)
        .stdin(match &input {
            ExecutionInput::InheritFromTerminal => Stdio::inherit(),
            ExecutionInput::CustomInput(_) => Stdio::piped(),
        })
        .stdout(match &input {
            ExecutionInput::InheritFromTerminal => Stdio::inherit(),
            ExecutionInput::CustomInput(_) => Stdio::piped(),
        })
        .spawn()
        .map_err(|err| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to spawn binary: {err}"),
            )
        })?;

    // Handle custom input in a separate thread
    if let ExecutionInput::CustomInput(input_data) = input {
        if let Some(child_stdin) = child.stdin.take() {
            let input_data = input_data.clone();

            // Spawn a new thread to write input in chunks
            thread::spawn(move || {
                let mut writer = BufWriter::new(child_stdin);
                const CHUNK_SIZE: usize = 1024;

                for chunk in input_data.as_bytes().chunks(CHUNK_SIZE) {
                    if writer.write_all(chunk).is_err() {
                        log!(error, "failed to write chunk to child process stdin. The process may have closed its stdin or encountered an error.");
                        break;
                    }
                }

                if writer.flush().is_err() {
                    log!(
                        error,
                        "failed to flush remaining data to child process stdin."
                    );
                }
            });
        }
    }
    let output = child.wait_with_output().map_err(|err| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("Failed to wait for binary execution: {err}"),
        )
    })?;

    let elapsed = now.elapsed();

    if output.status.success() {
        Ok(ExecutionStatus::Successful {
            output,
            time_elapsed: elapsed,
        })
    } else {
        Ok(ExecutionStatus::Failed(format!(
            "Binary execution failed with status: {:?}",
            output.status
        )))
    }
}
