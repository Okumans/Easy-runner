use super::core::ExecutionStatus;
use super::RunError;
use crate::cache_file::{get_config, put_file, Test};
use crate::cache_file::{get_file, FileCache};
use crate::execute::{core::execute_binary, recompile_binary, ExecutionInput};
use crate::log;
use crate::selector_evaluator::evaluate;
use crate::test_file::{merge_test_file, read_test_file, SimpleTest};
use crate::utils::{padded_string, sha256_digest};
use colored::Colorize;
use crossterm::terminal;
use data_encoding::HEXUPPER;
use std::error::Error;
use std::fs::File;
use std::io::{self, BufReader};
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub fn run_at(
    src_path: &Path,
    expression: &str,
    force_recompile: bool,
    show_full: bool,
) -> Result<(), RunError> {
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
        Ok(Some(file_cache)) if file_cache.source_hash == target_hashed && !force_recompile => {
            log!(
                info,
                "Cache for {src_path:?} is matched, skip re-compiling.."
            );
            file_cache
        }
        Ok(Some(file_cache)) => {
            log!(warn, "Re-compiling binary...");

            recompile_binary(src_path).map_err(RunError::CompilationError)?;

            let file_cache = FileCache {
                source_hash: target_hashed,
                tests: file_cache.tests,
            };

            put_file(filename, file_cache.clone())?;

            file_cache
        }
        _ => {
            log!(warn, "Re-compiling binary...");

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
                    output,
                }) = run_core(
                    &file_cache.tests[main_index - 1],
                    src_path,
                    filename,
                    &config.binary_dir_path,
                ) {
                    let (input, expected_output) = match &file_cache.tests[main_index - 1] {
                        Test::StringTest {
                            input,
                            expected_output,
                        } => (input, expected_output),
                        _ => unreachable!("Because it's a case of output single-test."),
                    };

                    if status {
                        println!(
                            "* ✅ {}{} {} in {}.",
                            "Test #".purple(),
                            main_index.to_string().yellow(),
                            "completed successfully".green(),
                            format!("{:?}", time_elapsed).green().italic()
                        );
                    } else {
                        let cols = terminal::size()?.0 as usize;
                        let rows = 15;

                        println!(
                            "* ❌ {}{}. Taking: {}\n{}\n{}\n{}\n{}\n{}\n{}",
                            "Test #".purple(),
                            main_index.to_string().yellow(),
                            format!("{:?}", time_elapsed).green().italic(),
                            "Input:".bold(),
                            padded_string(input, cols, rows, input.lines().count() == 1).blue(),
                            "Output:".bold(),
                            padded_string(&output, cols, rows, output.lines().count() == 1).red(),
                            "Expected-output:".bold(),
                            padded_string(
                                expected_output,
                                cols,
                                rows,
                                expected_output.lines().count() == 1
                            )
                            .green(),
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
                            total_test,
                            passed_test,
                            detailed_status,
                        } => {
                            print_ref_testcases_detailed(
                                _test_iterator(input, expected_output.as_ref())
                                    .map_err(RunError::Other)?,
                                detailed_status.as_slice(),
                                show_full,
                            )?;

                            // printing fancy test.
                            println!(
                                "\r* {} [{}]{} {}{} {} in average of {}",
                                if status { "✅" } else { "❌" },
                                (passed_test as f32 / total_test as f32 * 100.0)
                                    .to_string()
                                    .yellow(),
                                _ref_testcases_minimized(detailed_status.as_slice()),
                                "Test #".purple(),
                                if sub_tests.start() == sub_tests.end() {
                                    format!(
                                        "{}.{}",
                                        main_index.to_string().yellow(),
                                        sub_tests.start().to_string().yellow()
                                    )
                                } else {
                                    format!(
                                        "{}.{} - {}{}.{}",
                                        main_index.to_string().yellow(),
                                        sub_tests.start().to_string().yellow(),
                                        "#".purple(),
                                        main_index.to_string().yellow(),
                                        sub_tests.end().to_string().yellow(),
                                    )
                                },
                                if status {
                                    "completed successfully".green()
                                } else {
                                    "failed".red()
                                },
                                format!(
                                    "{:?}",
                                    Duration::from_millis(
                                        (detailed_status
                                            .iter()
                                            .map(|elm| elm.time_elapsed.as_millis())
                                            .sum::<u128>()
                                            / detailed_status.len() as u128)
                                            as u64
                                    )
                                )
                                .green()
                                .italic()
                            );
                        }
                        _ => unreachable!("tests in this loop will always be a ref test"),
                    }
                }
                None => {
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
                            show_full,
                        )?;
                        println!(
                            "\r* {} [{}]{} {}{} {} in average of {}",
                            if status { "✅" } else { "❌" },
                            (passed_test as f32 / total_test as f32 * 100.0)
                                .to_string()
                                .yellow(),
                            _ref_testcases_minimized(detailed_status.as_slice()),
                            "Test #".purple(),
                            main_index.to_string().yellow(),
                            if status {
                                "completed successfully".green()
                            } else {
                                "failed".red()
                            },
                            format!(
                                "{:?}",
                                Duration::from_millis(
                                    (detailed_status
                                        .iter()
                                        .map(|elm| elm.time_elapsed.as_millis())
                                        .sum::<u128>()
                                        / detailed_status.len() as u128)
                                        as u64
                                )
                            )
                            .green()
                            .italic()
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
    ignore_terminal_size: bool,
) -> io::Result<()> {
    let mut current_position = 0; // Track the current position of the iterator

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

            if detailed_status.status {
                println!(
                    "[{}] {}{}. Taking: {}",
                    "P".green(),
                    "SubTest #".purple().bold(),
                    (index + 1).to_string().yellow(),
                    format!("{:?}", detailed_status.time_elapsed)
                        .green()
                        .italic(),
                );
            } else {
                println!(
                    "[{}] {}{}. Taking: {}\n{}\n{}\n{}\n{}\n{}\n{}",
                    "-".red(),
                    "SubTest #".purple().bold(),
                    index.to_string().yellow(),
                    format!("{:?}", detailed_status.time_elapsed)
                        .green()
                        .italic(),
                    "Input:".bold(),
                    if ignore_terminal_size {
                        input.blue()
                    } else {
                        padded_string(&input, cols, rows, input.lines().count() == 1).blue()
                    },
                    "Output:".bold(),
                    if ignore_terminal_size {
                        detailed_status.output.red()
                    } else {
                        padded_string(
                            &detailed_status.output,
                            cols,
                            rows,
                            detailed_status.output.lines().count() == 1,
                        )
                        .red()
                    },
                    "Expected-output:".bold(),
                    if ignore_terminal_size {
                        expected_output.green()
                    } else {
                        padded_string(
                            &expected_output,
                            cols,
                            rows,
                            expected_output.lines().count() == 1,
                        )
                        .green()
                    },
                );
            }

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

pub fn _ref_testcases_minimized(detailed_statuses: &[DetailedStatus]) -> String {
    let mut average_time: Duration = Duration::from_secs(0);
    let mut result = String::new();

    result.push('[');
    for detailed_status in detailed_statuses {
        match detailed_status.status {
            true => result.push_str(&format!("{}", "P".green())),
            false => result.push_str(&format!("{}", "-".red())),
        };
        average_time += detailed_status.time_elapsed;
    }
    result.push(']');

    result
}

pub fn run(src_path: &Path, force_recompile: bool, show_full: bool) -> Result<(), RunError> {
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
        Ok(Some(file_cache)) if file_cache.source_hash == target_hashed && !force_recompile => {
            log!(info, "Cache hit for {src_path:?}. Skipping recompilation.");
            file_cache
        }
        Ok(Some(file_cache)) => {
            log!(warn, "Re-compiling binary...");

            recompile_binary(src_path).map_err(RunError::CompilationError)?;

            let file_cache = FileCache {
                source_hash: target_hashed,
                tests: file_cache.tests,
            };

            put_file(filename, file_cache.clone())?;

            file_cache
        }
        _ => {
            log!(warn, "Re-compiling binary...");

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
        log!(info, "No test found.");
        return Ok(());
    }

    for (index, test) in file_cache.tests.iter().enumerate() {
        score += match run_core(test, src_path, filename, &config.binary_dir_path) {
            Ok(RunResult::SingleTest {
                status,
                time_elapsed,
                output,
            }) => {
                let (input, expected_output) = match test {
                    Test::StringTest {
                        input,
                        expected_output,
                    } => (input, expected_output),
                    _ => unreachable!("Because it's a case of output single-test."),
                };

                if status {
                    println!(
                        "* ✅ {}{} {} in {}.",
                        "Test #".purple(),
                        index.to_string().yellow(),
                        "completed successfully".green(),
                        format!("{:?}", time_elapsed).green().italic()
                    );
                } else {
                    // println!(
                    //     "* ❌ {}{} {} in {}.",
                    //     "Test #".purple(),
                    //     index.to_string().yellow(),
                    //     "failed after".red(),
                    //     format!("{:?}", time_elapsed).green().italic()
                    // );

                    let cols = terminal::size()?.0 as usize;
                    let rows = 15;

                    println!(
                        "* ❌ {}{}. Taking: {}\n{}\n{}\n{}\n{}\n{}\n{}",
                        "Test #".purple(),
                        index.to_string().yellow(),
                        format!("{:?}", time_elapsed).green().italic(),
                        "Input:".bold(),
                        padded_string(input, cols, rows, input.lines().count() == 1).blue(),
                        "Output:".bold(),
                        padded_string(&output, cols, rows, output.lines().count() == 1).red(),
                        "Expected-output:".bold(),
                        padded_string(
                            expected_output,
                            cols,
                            rows,
                            expected_output.lines().count() == 1
                        )
                        .green(),
                    );
                }

                status as usize
            }

            Ok(RunResult::RefTest {
                status,
                total_test,
                passed_test,
                detailed_status,
            }) => {
                let (input, expected_output) = match test {
                    Test::RefTest {
                        input,
                        expected_output,
                    } => (input, expected_output),
                    _ => unreachable!("Because it's a case of ouput of ref-test."),
                };

                print_ref_testcases_detailed(
                    _test_iterator(input, expected_output.as_ref()).map_err(RunError::Other)?,
                    detailed_status.as_slice(),
                    show_full,
                )?;

                // printing fancy test.
                println!(
                    "\r* {} [{}]{} {}{} {} in average of {}",
                    if status { "✅" } else { "❌" },
                    (passed_test as f32 / total_test as f32 * 100.0)
                        .to_string()
                        .yellow(),
                    _ref_testcases_minimized(detailed_status.as_slice()),
                    "Test #".purple(),
                    index.to_string().yellow().italic(),
                    if status {
                        "completed successfully".green()
                    } else {
                        "failed".red()
                    },
                    format!(
                        "{:?}",
                        Duration::from_millis(
                            (detailed_status
                                .iter()
                                .map(|elm| elm.time_elapsed.as_millis())
                                .sum::<u128>()
                                / detailed_status.len() as u128) as u64
                        )
                    )
                    .green()
                    .italic()
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
        log!(success, "Successfuly add test.");

        return Ok(());
    }

    log!(warn, "Cache for {path:?} not found, start Re-compiling..");

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

    log!(success, "Successfuly add test.");

    Ok(())
}

pub enum RunResult {
    SingleTest {
        status: bool,
        time_elapsed: Duration,
        output: String,
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
                    let output = String::from_utf8_lossy(&output.stdout);
                    if expected_output != output.trim() {
                        return Ok(RunResult::SingleTest {
                            status: false,
                            time_elapsed,
                            output: output.into_owned(),
                        });
                    }
                    return Ok(RunResult::SingleTest {
                        status: true,
                        time_elapsed,
                        output: output.into_owned(),
                    });
                }

                ExecutionStatus::Failed(_) => {
                    return Ok(RunResult::SingleTest {
                        status: false,
                        time_elapsed: Duration::from_secs(0),
                        output: String::new(),
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
        log!(success, "Successfuly add test.");

        return Ok(());
    }

    log!(warn, "Cache for {path:?} not found, start Re-compiling..");

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

    log!(success, "Successfuly add test.");

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
        log!(success, "Successfuly add test.");

        return Ok(());
    }

    log!(warn, "Cache for {path:?} not found, start Re-compiling..");

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

    log!(success, "Successfuly add test.");

    Ok(())
}
