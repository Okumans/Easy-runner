use crate::cache_file::{get_config, put_file, Files, DEFAULT_CACHE_FILE, DEFUALT_BIN_DIR};
use crate::cache_file::{get_file, FileCache, Test};
use crate::log;
use crate::utils::sha256_digest;
use crate::utils::{limited_string, logging::*};

use colored::Colorize;
use crossterm::terminal;
use data_encoding::HEXUPPER;
use std::cmp;
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use thiserror::Error;

pub mod core;
pub mod test;

use core::{execute_binary, recompile_binary, ExecutionInput, ExecutionStatus};

#[derive(Debug, Error)]
pub enum RunError {
    #[error("I/O Error: {0}")]
    Io(#[from] io::Error),

    #[error("Compilation Error: {0}")]
    CompilationError(String),

    #[error("Other Error: {0}")]
    Other(String),
}

pub fn run(path: &Path) -> Result<(), RunError> {
    assert!(path.exists());

    let filename = path.file_name().unwrap().to_str().unwrap();

    let target_file = fs::File::open(path)?;
    let target_reader = io::BufReader::new(target_file);
    let target_hashed = HEXUPPER.encode(sha256_digest(target_reader)?.as_ref());

    let config = get_config()?; // Assume get_config returns io::Result

    match get_file(filename) {
        Ok(Some(file_cache)) if file_cache.source_hash == target_hashed => {
            log!(info, "Cache hit for {path:?}. Skipping recompilation.");
        }
        Ok(Some(file_cache)) => {
            log!(warn, "Source file {path:?} has changed. Recompiling...");

            recompile_binary(path).map_err(RunError::CompilationError)?;

            log!(
                success,
                "Recompilation succeeded. Binary for {path:?} is ready."
            );

            let file_cache = FileCache {
                source_hash: target_hashed,
                tests: file_cache.tests,
            };

            put_file(filename, file_cache.clone())?;
        }
        _ => {
            log!(warn, "No cache entry found for {path:?}. Recompiling...");

            recompile_binary(path).map_err(RunError::CompilationError)?;

            log!(
                success,
                "Compilation succeeded. Binary for {path:?} is ready."
            );

            let file_cache = FileCache {
                source_hash: target_hashed,
                tests: Vec::new(),
            };

            put_file(filename, file_cache.clone())?;
        }
    }

    loop {
        match execute_binary(
            &config.binary_dir_path,
            filename,
            ExecutionInput::InheritFromTerminal,
        )? {
            ExecutionStatus::Successful {
                output: _,
                time_elapsed,
            } => {
                log!(
                    success,
                    "Execution of {path:?} completed successfully in {time_elapsed:?}."
                );
                return Ok(());
            }
            ExecutionStatus::Failed(err) => {
                log!(error, "Execution of {path:?} failed due to error: {err}.");
                return Ok(());
            }
            ExecutionStatus::NeedRecompilation => {
                log!(warn, "Execution failed, recompilation needed for {path:?}.");

                recompile_binary(path).map_err(RunError::CompilationError)?;

                log!(
                    success,
                    "Recompilation succeeded for {path:?}. Retrying execution..."
                );
            }
        }
    }
}

pub fn check_initialized(current_path: &Path) -> bool {
    current_path.join(DEFAULT_CACHE_FILE).is_file()
}

pub fn initialize(current_path: &Path) -> io::Result<()> {
    if !check_initialized(current_path) {
        log!(info, "Initializing Easy Runner...");

        let mut binary_dir_path = current_path.to_path_buf();

        if !Path::new(DEFUALT_BIN_DIR).is_dir() {
            log!(
                question,
                "Specify the location for the compiled binary (Press Enter for default location): "
            );
            io::stdout().flush().unwrap();

            let mut location = String::new();
            io::stdin().read_line(&mut location)?;

            let trimmed = location.trim();
            binary_dir_path = if trimmed.is_empty() {
                binary_dir_path.join(DEFUALT_BIN_DIR)
            } else {
                binary_dir_path.join(trimmed)
            };

            if !binary_dir_path.is_dir() {
                fs::create_dir(&binary_dir_path)?;
                log!(
                    success,
                    "Created directory for binaries at: {binary_dir_path:?}"
                );
            }
        } else {
            binary_dir_path = binary_dir_path.join(DEFUALT_BIN_DIR);
            log!(
                info,
                "Using existing binary directory at: {binary_dir_path:?}"
            );
        }

        let languages_config: HashMap<String, String> = HashMap::from([
            (
                "cpp".to_string(),
                "g++ $(FILE) -o $(BIN_DIR)/$(FILENAME).$(EXE_EXT) --std=c++20".to_string(),
            ),
            (
                "c".to_string(),
                "gcc $(FILE) -o $(BIN_DIR)/$(FILENAME).$(EXE_EXT)".to_string(),
            ),
        ]);

        let files = Files {
            binary_dir_path: binary_dir_path.clone(),
            files: HashMap::new(),
            languages_config,
        };

        let file = fs::File::create(current_path.join(DEFAULT_CACHE_FILE))?;
        let mut writer = io::BufWriter::new(file);

        serde_json::to_writer_pretty(&mut writer, &files)?;
        writer.flush()?;

        log!(
            success,
            "Easy Runner has been successfully initialized with configuration."
        );
    } else {
        log!(info, "Easy Runner is already initialized.");
    }

    Ok(())
}

fn test_types_string(tests: &[Test]) -> String {
    tests
        .iter()
        .map(|test| match test {
            Test::StringTest {
                input: _,
                expected_output: _,
            } => "S".green(),
            Test::RefTest {
                input: _,
                expected_output: _,
            } => "R".yellow(),
        })
        .fold(String::new(), |mut acc, colored_string| {
            acc.push_str(&colored_string.to_string());
            acc
        })
}

pub fn status() -> io::Result<()> {
    let config = get_config()?;

    let tracked_numbers = config.files.len();
    if tracked_numbers == 0 {
        println!("{}", "Tracked nothing..".red());
        return Ok(());
    }

    println!("{}", format!("Tracked {} files.", tracked_numbers).blue());

    let hash_width = cmp::min(cmp::max(0, terminal::size()?.0 as i32) - 44, 65) as usize;

    // Print table headers with colors
    println!(
        "{:<4} {:<20} {:<hash_width$} {:<10} {:<10}",
        "No.".cyan(),
        "Filename".cyan(),
        "Hash".cyan(),
        "Tests".cyan(),
        "Types".cyan()
    );

    // Print each file's details with colors
    for (index, (filename, file_cache)) in config.files.iter().enumerate() {
        println!(
            "{:<4} {:<20} {:<hash_width$} {:<10} [{}]",
            (index + 1).to_string().cyan(),
            limited_string(filename, 20, 1, false).green(),
            limited_string(&file_cache.source_hash, hash_width, 1, false).blue(),
            file_cache.tests.len().to_string().magenta(),
            test_types_string(file_cache.tests.as_slice())
        );
    }

    Ok(())
}
