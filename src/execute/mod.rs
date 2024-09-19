use crate::cache_file::{get_config, put_file, template_config_replacement};
use crate::cache_file::{get_file, FileCache};
use crate::utils::sha256_digest;

use colored::Colorize;
use data_encoding::HEXUPPER;
use std::ffi;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::Instant;

pub mod test;

enum ExecutionInput {
    InheritFromTerminal,
    CustomInput(String),
}

pub fn run(path: &Path) -> io::Result<()> {
    assert!(path.exists());

    let filename = path.file_name().unwrap().to_str().unwrap();

    let target_file = fs::File::open(path)?;
    let target_reader = io::BufReader::new(target_file);
    let target_hashed = HEXUPPER.encode(sha256_digest(target_reader)?.as_ref());

    let config = get_config()?;

    let file_cache = match get_file(filename) {
        Ok(Some(file_cache)) if file_cache.source_hash == target_hashed => {
            println!(
                "{}",
                format!("ℹ️ Cache for {path:?} is matched, skip re-compiling..").bright_blue()
            );
            file_cache
        }
        Ok(Some(file_cache)) => {
            println!("{}", "🚧 Re-compiling binary...".yellow());

            recompile_binary(path)?;

            let file_cache = FileCache {
                source_hash: target_hashed,
                tests: file_cache.tests,
            };

            put_file(filename, file_cache.clone())?;

            file_cache
        }
        _ => {
            println!("{}", "🚧 Re-compiling binary...".yellow());

            recompile_binary(path)?;

            let file_cache = FileCache {
                source_hash: target_hashed,
                tests: Vec::new(),
            };

            put_file(filename, file_cache.clone())?;

            file_cache
        }
    };

    put_file(filename, file_cache)?;
    let _ = excute_binary(
        &config.binary_dir_path,
        filename,
        ExecutionInput::InheritFromTerminal,
        false,
    )?;

    Ok(())
}

fn excute_binary(
    binary_dir_path: &Path,
    filename: &str,
    input: ExecutionInput,
    quiet: bool,
) -> io::Result<Option<Output>> {
    let binary_name = if cfg!(windows) {
        format!("{filename}.exe")
    } else {
        format!("{filename}.out")
    };

    if !binary_dir_path.join(&binary_name).exists() {
        println!("🚧 Re-compilation needed, run \"{filename}\" again.");
        if let Ok(Some(file_cache)) = get_file(filename) {
            let file_cache = FileCache {
                source_hash: "PENDING_RECOMPILATION".to_string(),
                ..file_cache
            };
            put_file(filename, file_cache)?;
        } else {
            put_file(
                filename,
                FileCache {
                    source_hash: "PENDING_RECOMPILATION".to_string(),
                    tests: Vec::new(),
                },
            )?;
        }
        return Ok(None);
    }

    if !quiet {
        println!();
    }

    let now = Instant::now();

    let mut child = Command::new(binary_dir_path.join(&binary_name))
        .stdin(match input {
            ExecutionInput::InheritFromTerminal => Stdio::inherit(),
            ExecutionInput::CustomInput(_) => Stdio::piped(),
        })
        .stdout(match input {
            ExecutionInput::InheritFromTerminal => Stdio::inherit(),
            ExecutionInput::CustomInput(_) => Stdio::piped(),
        })
        .spawn()?;

    if let ExecutionInput::CustomInput(input) = input {
        let child_stdin = child.stdin.as_mut().unwrap();
        child_stdin.write_all(input.as_bytes())?;
        let _ = child_stdin;
    }

    let output = child.wait_with_output()?;

    let elapsed = now.elapsed();

    if !quiet {
        println!();
    } else {
        return Ok(Some(output));
    }

    if output.status.success() {
        println!("✅ Execution of \"{binary_name}\" succeeded, taking {elapsed:?}");
    } else {
        println!("❌ Execution of \"{binary_name}\" failed");
    }

    Ok(Some(output))
}

fn recompile_binary(path: &Path) -> io::Result<()> {
    let file_type = path.extension().and_then(ffi::OsStr::to_str).unwrap();

    let mut config = get_config()?;

    if !config.languages_config.contains_key(file_type) {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "❌ File type \"{file_type}\" is not supported.",
        ));
    }

    let mut sys_call = config.languages_config.remove(file_type).unwrap();
    if let Err(err) =
        template_config_replacement(&mut sys_call, config.binary_dir_path.as_path(), path)
    {
        return Err(io::Error::new(io::ErrorKind::Other, err));
    }

    let sys_call: Vec<&str> = sys_call.split_whitespace().collect();
    assert_ne!(sys_call.len(), 0);

    let command = sys_call[0];
    let args = &sys_call[1..];

    let output = Command::new(command)
        .args(args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .output()?;

    if output.status.success() {
        println!("✅ Compilation succeeded");
    } else {
        println!("❌ Compilation failed");
    }

    Ok(())
}
