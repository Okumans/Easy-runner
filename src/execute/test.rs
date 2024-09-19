use crate::cache_file::{get_config, put_file, Files, Test};
use crate::cache_file::{get_file, FileCache, DEFAULT_CACHE_FILE, DEFUALT_BIN_DIR};
use crate::execute::{excute_binary, recompile_binary, ExecutionInput};
use crate::selector_evaluator::evaluate;
use crate::test_file::{merge_test_file, read_test_file, SimpleTest};
use crate::utils::{append_extension, limited_print, sha256_digest};
use colored::Colorize;
use crossterm::terminal;
use data_encoding::HEXUPPER;
use itertools::Itertools;
use std::error::Error;
use std::fs::File;
use std::io::{self, BufReader, Write};
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};

pub fn run_at(path: &Path, expression: &str) -> io::Result<()> {
    assert!(path.exists());

    let filename = path.file_name().unwrap().to_str().unwrap();

    let target_file = File::open(path)?;
    let target_reader = BufReader::new(target_file);
    let target_hashed = HEXUPPER.encode(sha256_digest(target_reader)?.as_ref());

    let mut config = get_config()?;

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

        config = files.clone();

        let file = File::create(env_path.join(DEFAULT_CACHE_FILE))?;
        let mut writer = io::BufWriter::new(file);

        serde_json::to_writer_pretty(&mut writer, &files)?;
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

    if file_cache.tests.is_empty() {
        println!("{}", "ℹ️ No test found.".bright_blue());
        return Ok(());
    }

    let range_tests = evaluate(expression)
        .map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?;

    for range_test in range_tests {
        let main_index = range_test.main_test;

        if (file_cache.tests.len() < main_index) || (main_index == 0) {
            println!(
                "{} {}",
                "❌ Test main index is not valid. It must be in the range of 1 to.".bright_red(),
                file_cache.tests.len()
            );
            return Ok(());
        }

        match &file_cache.tests[main_index - 1] {
            Test::StringTest {
                input: _,
                expected_output: _,
            } => {
                if let Ok(RunResult::StringTest(success)) = run_core(
                    &file_cache.tests[main_index - 1],
                    filename,
                    &config.binary_dir_path,
                ) {
                    if success {
                        println!("* ✅ Test-{} completed.", main_index);
                    } else {
                        println!("* ❌ Test-{} failed.", main_index);
                    }
                }
            }

            Test::RefTest {
                input,
                expected_output,
            } => match range_test.sub_tests {
                Some(sub_tests) => {
                    let Ok(ref_test_result) = _ref_test_run_core(
                        input,
                        expected_output.as_ref(),
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
                            not_success: 0,
                            total: 0,
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
                            not_success: _,
                            total: _,
                        } => {
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
                        not_success,
                        total,
                    }) = _ref_test_run_core(
                        input,
                        expected_output.as_ref(),
                        filename,
                        &config.binary_dir_path,
                        None,
                    ) {
                        println!(
                            "\r* {} Test [{}] ({} passed out of {}) ",
                            if status { "✅" } else { "❌" },
                            main_index,
                            total - not_success,
                            total,
                        );
                    }
                }
            },
        };
    }

    Ok(())
}

pub fn run(path: &Path) -> io::Result<()> {
    assert!(path.exists());

    let filename = path.file_name().unwrap().to_str().unwrap();

    let target_file = File::open(path)?;
    let target_reader = BufReader::new(target_file);
    let target_hashed = HEXUPPER.encode(sha256_digest(target_reader)?.as_ref());

    let mut config = get_config()?;

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

        serde_json::to_writer_pretty(&mut writer, &files)?;
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

    let mut score: usize = 0;

    if file_cache.tests.is_empty() {
        println!("{}", "ℹ️ No test found.".bright_blue());
        return Ok(());
    }

    for (index, test) in file_cache.tests.iter().enumerate() {
        score += match run_core(test, filename, &config.binary_dir_path) {
            Ok(RunResult::StringTest(success)) => {
                println!(
                    "{} Test [{}/{}]",
                    if success { "✅" } else { "❌" },
                    index + 1,
                    file_cache.tests.len()
                );
                success as usize
            }

            Ok(RunResult::RefTest {
                status,
                not_success,
                total,
            }) => {
                println!(
                    "\r{} Test [{}/{}] ({} passed out of {}) ",
                    if status { "✅" } else { "❌" },
                    index + 1,
                    file_cache.tests.len(),
                    total - not_success,
                    total,
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

enum RunResult {
    StringTest(bool),
    RefTest {
        status: bool,
        not_success: usize,
        total: usize,
    },
}

fn _ref_test_run_core(
    input: &Path,
    expected_output: Option<&PathBuf>,
    guarantree_filename: &str,
    guarantree_binary_dir_path: &Path,
    only_run_at: Option<&RangeInclusive<usize>>,
) -> io::Result<RunResult> {
    if !input.exists() || (expected_output.is_some() && !expected_output.as_ref().unwrap().exists())
    {
        return Err(
        io::Error::new(
            io::ErrorKind::NotFound,
            format!( "❌ input content file is {}, {} expected_output content file {}, please make sure both of it is a valid filename.",
                if input.exists() {"exists"} else {"not exists"},
                if !input.exists() && ( !expected_output.unwrap().exists()  ){"and"} else {"but"},
                if expected_output.unwrap().exists() {"exists"} else {"not exists"})
            ));
    }

    let Ok(input_tests) = read_test_file(input) else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "❌ input content file cannot be read.",
        ));
    };

    let expected_output_tests = expected_output
        .as_ref()
        .map(|test_file_path| {
            read_test_file(test_file_path).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "❌ output content file cannot be read.",
                )
            })
        })
        .transpose()?;

    let input_filename = input
        .file_stem()
        .unwrap()
        .to_str()
        .unwrap_or("unvalid-filename");

    let mut inner_score: usize = 0;
    let mut total_inner_tests: usize = 0;
    let mut tests_pass = true;

    let test_iterator: Box<dyn Iterator<Item = Result<SimpleTest, Box<dyn Error>>>> =
        match expected_output_tests {
            Some(expected_output_tests) => Box::new(
                merge_test_file(input_tests, expected_output_tests)
                    .expect("Failed to merged test file."),
            ),
            None => Box::new(input_tests),
        };

    let mut only_run_at_ran: bool = false;

    #[allow(clippy::explicit_counter_loop)]
    for (inner_index, test) in test_iterator.enumerate() {
        if let Some(run_at) = only_run_at {
            if !run_at.contains(&(inner_index + 1)) {
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

        if let Some(output) = excute_binary(
            guarantree_binary_dir_path,
            guarantree_filename,
            ExecutionInput::CustomInput(input.clone()),
            true,
        )? {
            let mut status = false;
            let mut executed_output = String::new();

            if output.status.success() {
                executed_output =
                    String::from_utf8(output.stdout).expect("Unable to convert stdout to utf8");
                status = expected_output.trim() == executed_output.trim();
            }

            tests_pass &= status;
            inner_score += status as usize;

            if status {
                print!(
                    "\r✅ {input_filename} {} - {} [{inner_score}/{total_inner_tests}]",
                    "linked test".green(),
                    inner_index + 1,
                );
                io::stdout().flush()?;
            } else {
                println!(
                    "\r{}\n❌ {} - {} linked test [{inner_score}/{total_inner_tests}]",
                    "-".repeat((terminal::size().unwrap_or((20u16, 1u16)).0 as usize) / 2),
                    input_filename.bright_red().bold(),
                    inner_index + 1,
                );
                limited_print(
                    format!(
                        "{}:\n{}\n{}:\n{}\n{}:\n{}",
                        " input".yellow(),
                        input.split('\n').map(|line| format!("  {line}")).join("\n"),
                        " expected-output".bright_green(),
                        expected_output
                            .split('\n')
                            .map(|line| format!("  {line}"))
                            .join("\n"),
                        " output".bright_red(),
                        executed_output
                            .split('\n')
                            .map(|line| format!("  {line}"))
                            .join("\n"),
                    )
                    .as_str(),
                    100,
                    50,
                );
            }
        }
    }

    if only_run_at.is_some() && !only_run_at_ran {
        return Ok(RunResult::RefTest {
            status: false,
            not_success: 0,
            total: 0,
        });
    }

    Ok(RunResult::RefTest {
        status: tests_pass,
        not_success: total_inner_tests - inner_score,
        total: total_inner_tests,
    })
}

fn run_core(
    test: &Test,
    guarantree_filename: &str,
    guarantree_binary_dir_path: &Path,
) -> io::Result<RunResult> {
    assert!(
        guarantree_binary_dir_path.is_dir()
            && (append_extension(
                "out",
                guarantree_binary_dir_path
                    .join(guarantree_filename)
                    .to_path_buf()
            )
            .is_file()
                || append_extension(
                    "exe",
                    guarantree_binary_dir_path
                        .join(guarantree_filename)
                        .to_path_buf()
                )
                .is_file())
    );

    match test {
        Test::StringTest {
            input,
            expected_output,
        } => {
            if let Some(output) = excute_binary(
                guarantree_binary_dir_path,
                guarantree_filename,
                ExecutionInput::CustomInput(input.clone()),
                true,
            )? {
                if !output.status.success() {
                    return Ok(RunResult::StringTest(false));
                }

                if expected_output != String::from_utf8_lossy(&output.stdout).as_ref() {
                    return Ok(RunResult::StringTest(false));
                }
            }

            Ok(RunResult::StringTest(true))
        }

        Test::RefTest {
            input,
            expected_output,
        } => _ref_test_run_core(
            input,
            expected_output.as_ref(),
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
