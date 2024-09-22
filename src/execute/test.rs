use super::core::ExecutionStatus;
use super::RunError;
use crate::cache_file::{get_config, put_file, Files, Test};
use crate::cache_file::{get_file, FileCache, DEFAULT_CACHE_FILE, DEFUALT_BIN_DIR};
use crate::execute::{core::execute_binary, recompile_binary, ExecutionInput};
use crate::log;
use crate::selector_evaluator::evaluate;
use crate::test_file::{merge_test_file, read_test_file, SimpleTest};
use crate::utils::{append_extension, limited_string, sha256_digest};
use colored::Colorize;
use crossterm::terminal;
use data_encoding::HEXUPPER;
use std::error::Error;
use std::fs::File;
use std::io::{self, BufReader, Write};
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub fn run_at(src_path: &Path, expression: &str) -> Result<(), RunError> {
    assert!(src_path.exists());

    let filename = src_path.file_name().unwrap().to_str().unwrap();

    let target_file = File::open(src_path)?;
    let target_reader = BufReader::new(target_file);
    let target_hashed = HEXUPPER.encode(sha256_digest(target_reader)?.as_ref());

    let config = get_config()?;

    if !config.binary_dir_path.is_dir() {
        return Err(RunError::Other("Binary path not found.".to_string()));
    }

    let file_cache = match get_file(filename) {
        Ok(Some(file_cache)) if file_cache.source_hash == target_hashed => {
            log!(
                info,
                "Cache for {src_path:?} is matched, skip re-compiling.."
            );
            file_cache
        }
        Ok(Some(file_cache)) => {
            log!(warn, "🚧 Re-compiling binary...");

            recompile_binary(src_path).map_err(RunError::CompilationError)?;

            let file_cache = FileCache {
                source_hash: target_hashed,
                tests: file_cache.tests,
            };

            put_file(filename, file_cache.clone())?;

            file_cache
        }
        _ => {
            log!(warn, "🚧 Re-compiling binary...");

            recompile_binary(src_path).map_err(RunError::CompilationError)?;

            let file_cache = FileCache {
                source_hash: target_hashed,
                tests: Vec::new(),
            };

            put_file(filename, file_cache.clone())?;

            file_cache
        }
    };

    if file_cache.tests.is_empty() {
        log!(info, "No test found.");
        return Ok(());
    }

    let range_tests = evaluate(expression)
        .map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?;

    for range_test in range_tests {
        let main_index = range_test.main_test;

        if (file_cache.tests.len() < main_index) || (main_index == 0) {
            log!(
                error,
                "Test main index is not valid. It must be in the range of 1 to {}.",
                file_cache.tests.len()
            );
            return Ok(());
        }

        match &file_cache.tests[main_index - 1] {
            Test::StringTest {
                input: _,
                expected_output: _,
            } => {
                if let Ok(RunResult::SingleTest {
                    status,
                    time_elapsed,
                }) = run_core(
                    &file_cache.tests[main_index - 1],
                    src_path,
                    filename,
                    &config.binary_dir_path,
                ) {
                    if status {
                        println!(
                            "* ✅ {} {} in {:?}.",
                            format!("Test #{}", main_index).blue(),
                            "completed successfully".green(),
                            time_elapsed
                        );
                    } else {
                        println!(
                            "* ❌ {} {} in {:?}.",
                            format!("Test #{}", main_index).blue(),
                            "failed after".red(),
                            time_elapsed
                        );
                    }
                }
            }

            Test::RefTest {
                input,
                expected_output,
            } => match range_test.sub_tests {
                Some(sub_tests) => {
                    let Ok(ref_test_result) = _ref_test_run_core(
                        src_path,
                        _test_iterator(input, expected_output.as_ref()).map_err(RunError::Other)?,
                        filename,
                        &config.binary_dir_path,
                        Some(&sub_tests),
                    ) else {
                        println!(
                            "\r* ❌ Some test in {}.{} - {}.{}  is cooked",
                            main_index,
                            sub_tests.start(),
                            main_index,
                            sub_tests.end()
                        );
                        break;
                    };

                    match ref_test_result {
                        RunResult::RefTest {
                            status: false,
                            total_test: 0,
                            passed_test: 0,
                            detailed_status: _,
                        } => {
                            println!(
                                "\r* ❌ Some test in {}.{} - {}.{}  isn't exists",
                                main_index,
                                sub_tests.start(),
                                main_index,
                                sub_tests.end()
                            );
                            break;
                        }
                        RunResult::RefTest {
                            status,
                            total_test: _,
                            passed_test: _,
                            detailed_status,
                        } => {
                            print_ref_testcases_detailed(
                                _test_iterator(input, expected_output.as_ref())
                                    .map_err(RunError::Other)?,
                                detailed_status.as_slice(),
                            )?;

                            if status {
                                println!(
                                    "\r* ✅ Test {}.{} - {}.{} completed.",
                                    main_index,
                                    sub_tests.start(),
                                    main_index,
                                    sub_tests.end()
                                );
                            } else {
                                println!(
                                    "\r* ❌ Test {}.{} - {}.{} is failed.",
                                    main_index,
                                    sub_tests.start(),
                                    main_index,
                                    sub_tests.end()
                                );
                            }
                        }
                        _ => unreachable!("tests in this loop will always be a ref test"),
                    }
                }
                None => {
                    // _ref_test_run_core(
                    //     input,
                    //     expected_output.as_ref(),
                    //     filename,
                    //     &config.binary_dir_path,
                    //     None,
                    // );
                    if let Ok(RunResult::RefTest {
                        status,
                        total_test,
                        passed_test,
                        detailed_status,
                    }) = _ref_test_run_core(
                        src_path,
                        _test_iterator(input, expected_output.as_ref()).map_err(RunError::Other)?,
                        filename,
                        &config.binary_dir_path,
                        None,
                    ) {
                        print_ref_testcases_detailed(
                            _test_iterator(input, expected_output.as_ref())
                                .map_err(RunError::Other)?,
                            detailed_status.as_slice(),
                        )?;
                        println!(
                            "\r* {} Test [{}] ({} passed out of {}) ",
                            if status { "✅" } else { "❌" },
                            main_index,
                            passed_test,
                            total_test,
                        );
                    }
                }
            },
        };
    }

    Ok(())
}

pub fn print_ref_testcases_detailed(
    mut test_iterator: TestIterator, // Mutable iterator so we can advance it
    detailed_statuses: &[DetailedStatus],
) -> io::Result<()> {
    let mut current_position = 0; // Track the current position of the iterator

    let padded_string = |content: &str, cols: usize, rows: usize, single_line: bool| -> String {
        limited_string(content, cols - 2, rows, single_line)
            .lines()
            .map(|line| format!("  {}", line)) // Add 2 spaces padding to each line
            .collect::<Vec<String>>()
            .join("\n")
    };

    let cols = terminal::size()?.0 as usize;
    let rows = 15;

    for detailed_status in detailed_statuses.iter() {
        let index = detailed_status.ref_test_index;

        // Move the iterator to the specified index
        if index >= current_position {
            // Calculate how many elements we need to skip
            let steps = index - current_position;
            let (input, expected_output) = if let Some(test_case) = test_iterator.nth(steps) {
                match test_case {
                    Ok(test_case) => {
                        current_position = index + 1; // Update the current position after consuming nth
                        (test_case.input, test_case.expected_output)
                    }
                    Err(e) => {
                        log!(error, "Error processing test case: {}", e);
                        continue;
                    }
                }
            } else {
                log!(error, "No test case found at index {}", index);
                continue;
            };

            println!(
                "[{}] SubTest #{}. Taking: {:?}\n{}\n{}\n{}\n{}\n{}\n{}\n",
                if detailed_status.status {
                    "P".green()
                } else {
                    "-".red()
                },
                index,
                detailed_status.time_elapsed,
                "Input:".bold(),
                padded_string(&input, cols, rows, input.lines().count() == 1).blue(),
                "Output:".bold(),
                padded_string(
                    &detailed_status.output,
                    cols,
                    rows,
                    detailed_status.output.lines().count() == 1
                )
                .red(),
                "Expected-output:".bold(),
                padded_string(
                    &expected_output,
                    cols,
                    rows,
                    expected_output.lines().count() == 1
                )
                .green(),
            );

            println!("{}", "-".repeat(cols));
        } else {
            log!(
                error,
                "Cannot go backwards in an iterator (current position: {}, requested index: {})",
                current_position,
                index
            );
        }
    }

    Ok(())
}

pub fn print_ref_testcases_minimized() {}

pub fn run(src_path: &Path) -> Result<(), RunError> {
    assert!(src_path.exists());

    let filename = src_path.file_name().unwrap().to_str().unwrap();

    let target_file = File::open(src_path)?;
    let target_reader = BufReader::new(target_file);
    let target_hashed = HEXUPPER.encode(sha256_digest(target_reader)?.as_ref());

    let config = get_config()?;

    if !config.binary_dir_path.is_dir() {
        let env_path = std::env::current_dir().expect("Cannot find current directory.");
        let mut binary_dir_path = env_path.join(DEFUALT_BIN_DIR);

        println!(
            "{}",
            "❌ Configured compiled Binary directory path not found.".bright_red()
        );

        if !env_path.join(DEFUALT_BIN_DIR).is_dir() {
            print!("❓ Where would you like the compiled binary to be located? (Leave blank for default location): ");
            io::stdout().flush().unwrap();

            let mut location = String::new();

            io::stdin().read_line(&mut location)?;

            let trimmed = location.trim();
            binary_dir_path = if trimmed.is_empty() {
                binary_dir_path.join(DEFUALT_BIN_DIR)
            } else {
                binary_dir_path.join(trimmed)
            };
        } else {
            println!(
                "{}",
                "ℹ️ Located default compiled binary directory.".bright_blue()
            );
        }

        let files: Files = Files {
            binary_dir_path,
            ..config
        };

        let file = File::create(env_path.join(DEFAULT_CACHE_FILE))?;
        let mut writer = io::BufWriter::new(file);

        serde_json::to_writer_pretty(&mut writer, &files).map_err(|err| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to write data to json : {}", err),
            )
        })?;
        writer.flush()?;

        println!(
            "{}",
            "✅ Successfuly updated compiled binary directory path.".green()
        )
    }

    let file_cache = match get_file(filename) {
        Ok(Some(file_cache)) if file_cache.source_hash == target_hashed => {
            println!(
                "{}",
                format!("ℹ️ Cache for {src_path:?} is matched, skip re-compiling..").bright_blue()
            );
            file_cache
        }
        Ok(Some(file_cache)) => {
            println!("{}", "🚧 Re-compiling binary...".yellow());

            recompile_binary(src_path).map_err(RunError::CompilationError)?;

            let file_cache = FileCache {
                source_hash: target_hashed,
                tests: file_cache.tests,
            };

            put_file(filename, file_cache.clone())?;

            file_cache
        }
        _ => {
            println!("{}", "🚧 Re-compiling binary...".yellow());

            recompile_binary(src_path).map_err(RunError::CompilationError)?;

            let file_cache = FileCache {
                source_hash: target_hashed,
                tests: Vec::new(),
            };

            put_file(filename, file_cache.clone())?;

            file_cache
        }
    };

    let mut score: usize = 0;

    if file_cache.tests.is_empty() {
        println!("{}", "ℹ️ No test found.".bright_blue());
        return Ok(());
    }

    for (index, test) in file_cache.tests.iter().enumerate() {
        score += match run_core(test, src_path, filename, &config.binary_dir_path) {
            Ok(RunResult::SingleTest {
                status,
                time_elapsed,
            }) => {
                println!(
                    "{} Test [{}/{}]",
                    if status { "✅" } else { "❌" },
                    index + 1,
                    file_cache.tests.len()
                );
                status as usize
            }

            Ok(RunResult::RefTest {
                status,
                total_test,
                passed_test,
                detailed_status: _,
            }) => {
                println!(
                    "\r{} Test [{}/{}] ({} passed out of {}) ",
                    if status { "✅" } else { "❌" },
                    index + 1,
                    file_cache.tests.len(),
                    passed_test,
                    total_test,
                );
                status as usize
            }

            Err(error) => {
                println!("{}", error);
                0
            }
        };
    }

    if score == file_cache.tests.len() {
        println!(
            "{}",
            format!("* ✅ Test completed, {score} tests passed out of a total of {score}.").green()
        );
    } else {
        println!(
            "{}",
            format!(
                "* ❌ Test failed, {score} tests passed out of a total of {}.",
                file_cache.tests.len()
            )
            .red()
        );
    }

    Ok(())
}

pub fn add(path: &Path, input: &str, expected_output: &str) -> Result<(), io::Error> {
    assert!(path.exists());

    let filename = path.file_name().unwrap().to_str().unwrap();

    if let Ok(Some(mut file_cache)) = get_file(filename) {
        file_cache.tests.push(Test::StringTest {
            input: input.to_string(),
            expected_output: expected_output.to_string(),
        });

        put_file(filename, file_cache)?;
        println!("{}", "✅ Successfuly add test.".green());

        return Ok(());
    }

    println!(
        "{}",
        "🚧 cache for {path:?} not found, start Re-compiling..".yellow()
    );

    let file = File::open(path)?;
    let reader = io::BufReader::new(file);
    let hashed = HEXUPPER.encode(sha256_digest(reader)?.as_ref());

    put_file(
        filename,
        FileCache {
            source_hash: hashed,
            tests: vec![Test::StringTest {
                input: input.to_string(),
                expected_output: expected_output.to_string(),
            }],
        },
    )?;

    println!("{}", "✅ Successfuly add test.".green());

    Ok(())
}

pub enum RunResult {
    SingleTest {
        status: bool,
        time_elapsed: Duration,
    },
    RefTest {
        status: bool,
        total_test: usize,
        passed_test: usize,
        detailed_status: Vec<DetailedStatus>,
    },
}

pub struct DetailedStatus {
    pub ref_test_index: usize,
    pub status: bool,
    pub time_elapsed: Duration,
    pub output: String,
}

type TestIterator = Box<dyn Iterator<Item = Result<SimpleTest, Box<dyn Error>>>>;

fn _test_iterator(
    input_path: &Path,
    expected_output_path: Option<&PathBuf>,
) -> Result<TestIterator, String> {
    if !input_path.exists()
        || (expected_output_path.is_some() && !expected_output_path.as_ref().unwrap().exists())
    {
        return Err(format!(
            "The input file {} {} the expected output file {}. Please ensure both files exist and the paths are correct.",
            if input_path.exists() { "exists" } else { "does not exist" },
            if !input_path.exists() && expected_output_path.is_some() && !expected_output_path.unwrap().exists() { "and" } else { "but" },
            if expected_output_path.unwrap().exists() { "exists" } else { "does not exist" }
        ));
    }

    // Attempt to read the input test file
    let input_tests =
        read_test_file(input_path).map_err(|_| "Failed to read the input file.".to_string())?;

    // Attempt to read the expected output file, if provided
    let expected_output_tests = match expected_output_path {
        Some(path) => Some(
            read_test_file(path)
                .map_err(|_| "Failed to read the expected output file.".to_string())?,
        ),
        None => None,
    };

    // Create the test iterator
    let test_iterator: Box<dyn Iterator<Item = Result<SimpleTest, Box<dyn Error>>>> =
        match expected_output_tests {
            Some(output_tests) => Box::new(
                merge_test_file(input_tests, output_tests)
                    .map_err(|_| "Failed to merge the input and expected output files.")?,
            ),
            None => Box::new(input_tests),
        };

    Ok(test_iterator)
}

fn _ref_test_run_core(
    src_path: &Path,
    test_iterator: TestIterator,
    guarantree_filename: &str,
    guarantree_binary_dir_path: &Path,
    run_range: Option<&RangeInclusive<usize>>,
) -> Result<RunResult, RunError> {
    let mut inner_score: usize = 0;
    let mut total_inner_tests: usize = 0;
    let mut tests_pass = true;
    let mut only_run_at_ran: bool = false;
    let mut detailed_status: Vec<DetailedStatus> = Vec::new();

    #[allow(clippy::explicit_counter_loop)]
    for (inner_index, test) in test_iterator.enumerate() {
        if let Some(run_range) = run_range {
            if !run_range.contains(&(inner_index + 1)) {
                continue;
            }
            only_run_at_ran = true;
        }

        let Ok(SimpleTest {
            input,
            expected_output,
        }) = test
        else {
            break;
        };

        total_inner_tests += 1;
        let (status, executed_output, time_elapsed) = loop {
            let execution_status = execute_binary(
                guarantree_binary_dir_path,
                guarantree_filename,
                ExecutionInput::CustomInput(input.clone()),
            )?;

            match execution_status {
                ExecutionStatus::Successful {
                    output,
                    time_elapsed,
                } => {
                    let output_str =
                        String::from_utf8(output.stdout).expect("Unable to convert stdout to utf8");
                    let success = expected_output.trim() == output_str.trim();
                    break (success, output_str, time_elapsed); // Exit loop with success and output
                }
                ExecutionStatus::NeedRecompilation => {
                    log!(warn, "Recompiling need. pending recompilation.");
                    recompile_binary(src_path).map_err(RunError::CompilationError)?;
                    log!(success, "Successful compiling {src_path:?}");
                }
                ExecutionStatus::Failed(_) => {
                    // Log the error and skip the current test
                    break (false, String::new(), Duration::from_secs(0));
                }
            }
        };

        tests_pass &= status;
        inner_score += status as usize;
        detailed_status.push(DetailedStatus {
            ref_test_index: inner_index,
            status,
            time_elapsed,
            output: executed_output,
        });
    }

    fn format_lines(text: &str) -> String {
        text.split('\n')
            .map(|line| format!("  {line}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    if run_range.is_some() && !only_run_at_ran {
        return Ok(RunResult::RefTest {
            status: false,
            total_test: 0,
            passed_test: 0,
            detailed_status,
        });
    }

    Ok(RunResult::RefTest {
        status: tests_pass,
        total_test: total_inner_tests,
        passed_test: inner_score,
        detailed_status,
    })
}

fn run_core(
    test: &Test,
    src_path: &Path,
    guarantree_filename: &str,
    guarantree_binary_dir_path: &Path,
) -> Result<RunResult, RunError> {
    match test {
        Test::StringTest {
            input,
            expected_output,
        } => loop {
            match execute_binary(
                guarantree_binary_dir_path,
                guarantree_filename,
                ExecutionInput::CustomInput(input.clone()),
            )? {
                ExecutionStatus::Successful {
                    output,
                    time_elapsed,
                } => {
                    if expected_output != String::from_utf8_lossy(&output.stdout).as_ref() {
                        return Ok(RunResult::SingleTest {
                            status: false,
                            time_elapsed,
                        });
                    }
                    return Ok(RunResult::SingleTest {
                        status: true,
                        time_elapsed,
                    });
                }

                ExecutionStatus::Failed(_) => {
                    return Ok(RunResult::SingleTest {
                        status: false,
                        time_elapsed: Duration::from_secs(0),
                    });
                }

                ExecutionStatus::NeedRecompilation => {
                    log!(warn, "Recompiling need. pending recompilation.");
                    recompile_binary(src_path).map_err(RunError::CompilationError)?;
                    log!(success, "Successful compiling {src_path:?}");
                }
            }
        },

        Test::RefTest {
            input,
            expected_output,
        } => _ref_test_run_core(
            src_path,
            _test_iterator(input, expected_output.as_ref()).map_err(RunError::Other)?,
            guarantree_filename,
            guarantree_binary_dir_path,
            None,
        ),
    }
}

pub fn add_file_link(path: &Path, file_tests: &Path) -> io::Result<()> {
    assert!(path.exists() && file_tests.exists());
    let filename = path.file_name().unwrap().to_str().unwrap();

    if let Ok(Some(mut file_cache)) = get_file(filename) {
        file_cache.tests.push(Test::RefTest {
            input: file_tests.to_path_buf(),
            expected_output: None,
        });

        put_file(filename, file_cache)?;
        println!("{}", "✅ Successfuly add test.".green());

        return Ok(());
    }

    println!(
        "{}",
        format!("🚧 cache for {path:?} not found, start Re-compiling..").yellow()
    );

    let file = File::open(path)?;
    let reader = io::BufReader::new(file);
    let hashed = HEXUPPER.encode(sha256_digest(reader)?.as_ref());

    put_file(
        filename,
        FileCache {
            source_hash: hashed,
            tests: vec![Test::RefTest {
                input: file_tests.to_path_buf(),
                expected_output: None,
            }],
        },
    )?;

    println!("{}", "✅ Successfuly add test.".green());

    Ok(())
}

pub fn add_standalone_file_link(
    path: &Path,
    file_input: &Path,
    file_expected_output: &Path,
) -> io::Result<()> {
    assert!(path.exists() && file_input.exists() && file_expected_output.exists());
    let filename = path.file_name().unwrap().to_str().unwrap();

    if let Ok(Some(mut file_cache)) = get_file(filename) {
        file_cache.tests.push(Test::RefTest {
            input: file_input.to_path_buf(),
            expected_output: Some(file_expected_output.to_path_buf()),
        });

        put_file(filename, file_cache)?;
        println!("{}", "✅ Successfuly add test.".green());

        return Ok(());
    }

    println!(
        "{}",
        format!("🚧 cache for {path:?} not found, start Re-compiling..").yellow()
    );

    let file = File::open(path)?;
    let reader = io::BufReader::new(file);
    let hashed = HEXUPPER.encode(sha256_digest(reader)?.as_ref());

    put_file(
        filename,
        FileCache {
            source_hash: hashed,
            tests: vec![Test::RefTest {
                input: file_input.to_path_buf(),
                expected_output: Some(file_expected_output.to_path_buf()),
            }],
        },
    )?;

    println!("{}", "✅ Successfuly add test.".green());

    Ok(())
}

pub fn config(path: &Path) {}
