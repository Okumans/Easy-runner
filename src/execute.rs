use crate::cache_file::{get_config, put_file, template_config_replacement};
use crate::cache_file::{get_file, FileCache};
use data_encoding::HEXUPPER;
use ring::digest;
use std::ffi;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::Instant;

pub mod test {
    use crate::cache_file::{get_config, put_file, Files, Test};
    use crate::cache_file::{get_file, FileCache, DEFAULT_CACHE_FILE, DEFUALT_BIN_DIR};
    use crate::execute::{
        append_extension, excute_binary, limited_print, recompile_binary, sha256_digest,
        ExecutionInput,
    };
    use crate::test_file::{merge_test_file, read_test_file, SimpleTest, TestFileIterator};
    use colored::Colorize;
    use crossterm::{
        event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent},
        execute,
        terminal::{
            disable_raw_mode, enable_raw_mode, size, EnterAlternateScreen, LeaveAlternateScreen,
        },
    };
    use data_encoding::HEXUPPER;
    use itertools::{izip, Itertools};
    use std::error::Error;
    use std::fs::File;
    use std::io::{self, BufRead, BufReader, Write};
    use std::path::{Path, PathBuf};
    use tui::style::Modifier;
    use tui::{
        backend::CrosstermBackend,
        layout::{Constraint, Direction, Layout},
        style::{Color, Style},
        text::{Span, Spans},
        widgets::{Block, Borders, Paragraph},
        Terminal,
    };

    use super::limited_string;

    pub fn run_at(path: &Path, index: usize) -> io::Result<()> {
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
                    "ℹ️ Cache for {path:?} is matched, skip re-compiling..".bright_blue()
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

        if (file_cache.tests.len() < index) || (index == 0) {
            println!(
                "{} {}",
                "❌ Test index is not valid. It must be in the range of 1 to.".bright_red(),
                file_cache.tests.len()
            );
            return Ok(());
        }

        let run_result = match &file_cache.tests[index - 1] {
            Test::StringTest {
                input: _,
                expected_output: _,
            } => run_core(
                &file_cache.tests[index - 1],
                filename,
                &config.binary_dir_path,
            ),
            Test::RefTest {
                input,
                expected_output,
            } => _ref_test_run_core(
                input,
                expected_output.as_ref(),
                filename,
                &config.binary_dir_path,
                Some(1),
            ),
        };

        match run_result {
            Ok(RunResult::StringTest(success)) => {
                if success {
                    println!("* ✅ Test-{} completed.", index);
                } else {
                    println!("* ❌ Test-{} failed.", index);
                }
                success as usize
            }

            Ok(RunResult::RefTest {
                status,
                not_success,
                total,
            }) => {
                println!(
                    "\r{} Test [{}] ({} passed out of {}) ",
                    if status { "✅" } else { "❌" },
                    index,
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
                format!("* ✅ Test completed, {score} tests passed out of a total of {score}.")
                    .green()
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

    fn show_core(lines: Vec<&str>, filename: &str) -> io::Result<()> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let mut offset = 0;
        let mut highlight = 0;
        let display_height: usize = terminal.size()?.height as usize - 2;

        loop {
            terminal.draw(|f| {
                let size = f.size();
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Percentage(100)].as_ref())
                    .split(size);

                let events_display: Vec<Spans> = lines
                    .iter()
                    .enumerate()
                    .skip(offset)
                    .map(|(i, event)| {
                        if event.starts_with("Testcase") {
                            Spans::from(Span::styled(
                                *event,
                                Style::default()
                                    .add_modifier(Modifier::BOLD)
                                    .bg(if i == highlight - offset {
                                        Color::Yellow
                                    } else {
                                        Color::Green
                                    })
                                    .fg(Color::Black),
                            ))
                        } else if event.starts_with(" Testcase") {
                            Spans::from(vec![
                                Span::raw(" "),
                                Span::styled(
                                    event[1..].to_string(),
                                    Style::default()
                                        .add_modifier(Modifier::BOLD)
                                        .bg(if i == highlight - offset {
                                            Color::Yellow
                                        } else {
                                            Color::Blue
                                        })
                                        .fg(Color::Black),
                                ),
                            ])
                        } else if i == highlight - offset {
                            Spans::from(Span::styled(
                                *event,
                                Style::default()
                                    .fg(Color::Yellow)
                                    .add_modifier(Modifier::BOLD),
                            ))
                        } else {
                            Spans::from(Span::raw(*event))
                        }
                    })
                    .collect();

                let events_paragraph = Paragraph::new(events_display).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(format!("Tests from {}", filename)),
                );

                f.render_widget(events_paragraph, chunks[0]);
            })?;

            if let Event::Key(key) = event::read()? {
                match key {
                    KeyEvent {
                        code: KeyCode::Char('q'),
                        ..
                    } => break,
                    KeyEvent {
                        code: KeyCode::Char('j'),
                        ..
                    } => {
                        if highlight < lines.len() - 1 {
                            highlight += 1;
                            if highlight >= offset + display_height / 2 {
                                offset += 1;
                            }
                        } else if highlight - offset < lines.len() - 1 {
                            highlight += 1;
                        }
                    }
                    KeyEvent {
                        code: KeyCode::Char('k'),
                        ..
                    } => {
                        if highlight > 0 {
                            highlight -= 1;
                            if highlight < offset + display_height / 2 && offset > 0 {
                                offset -= 1;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;

        Ok(())
    }

    pub fn show(path: &Path) -> io::Result<()> {
        let filename = path.file_name().unwrap().to_str().unwrap();

        let Ok(Some(file_cache)) = get_file(filename) else {
            println!("ℹ️ Test not found.");
            return Ok(());
        };

        if file_cache.tests.is_empty() {
            println!("ℹ️ No test found.");
            return Ok(());
        }

        let mut show_contents = String::new();

        for (index, test) in file_cache.tests.into_iter().enumerate() {
            match test {
                Test::StringTest {
                    input,
                    expected_output,
                } => {
                    show_contents += &format!("Testcase-{}\n", index + 1);
                    if !input.contains('\n') && !expected_output.contains('\n') {
                        show_contents +=
                            &format!(" Input : {}\n Output: {}\n", input, expected_output);
                    } else {
                        show_contents +=
                            &format!(" Input :\n{}\n Output:\n{}\n", input, expected_output);
                    }
                }
                Test::RefTest {
                    input,
                    expected_output,
                } => {
                    show_contents += &format!("Testcase-linked-{}\n", index + 1);

                    if !input.exists()
                        || (expected_output.is_some()
                            && !expected_output.as_ref().unwrap().exists())
                    {
                        println!( "❌ input content file is {}, {} expected_output content file {}, please make sure both of it is a valid filename.",
                            if input.exists() {"exists"} else {"not exists"},
                            if !input.exists() && ( !expected_output.as_ref().unwrap().exists()  ){"and"} else {"but"},
                            if expected_output.as_ref().unwrap().exists() {"exists"} else {"not exists"});
                        continue;
                    }

                    let Ok(input_tests) = read_test_file(input.as_path()) else {
                        println!("❌ input content file cannot be read.",);
                        continue;
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

                    let test_iterator: Box<
                        dyn Iterator<Item = Result<SimpleTest, Box<dyn Error>>>,
                    > = if let Some(expected_output_tests) = expected_output_tests {
                        Box::new(
                            merge_test_file(input_tests, expected_output_tests)
                                .expect("Failed to merged test file."),
                        )
                    } else {
                        Box::new(input_tests)
                    };

                    #[allow(clippy::explicit_counter_loop)]
                    for (inner_index, test) in test_iterator.enumerate() {
                        let Ok(SimpleTest {
                            input,
                            expected_output,
                        }) = test
                        else {
                            break;
                        };

                        show_contents += &format!(" Testcase-{inner_index}\n");
                        if !input.contains('\n') && !expected_output.contains('\n') {
                            show_contents += &format!(
                                "  Input : {}  Output: {}",
                                limited_string(&input, 50, 1),
                                limited_string(&expected_output, 50, 1)
                            );
                        } else {
                            show_contents += &format!(
                                "  Input :   \n{}\n  Output:\n   {}\n",
                                limited_string(&input, 50, 20)
                                    .lines()
                                    .map(|line| format!("  {line}"))
                                    .join("\n"),
                                limited_string(&expected_output, 50, 20)
                                    .lines()
                                    .map(|line| format!("  {line}"))
                                    .join("\n"),
                            );
                        }
                    }
                }
            }
        }

        show_core(show_contents.lines().collect_vec(), filename)?;

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
        only_run_at: Option<usize>,
    ) -> io::Result<RunResult> {
        if !input.exists()
            || (expected_output.is_some() && !expected_output.as_ref().unwrap().exists())
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
            if expected_output_tests.is_some() {
                Box::new(
                    merge_test_file(input_tests, expected_output_tests.unwrap())
                        .expect("Failed to merged test file."),
                )
            } else {
                Box::new(input_tests)
            };

        let mut only_run_at_ran: bool = false;

        #[allow(clippy::explicit_counter_loop)]
        for (inner_index, test) in test_iterator.enumerate() {
            if !(only_run_at.is_some() && only_run_at.unwrap() == inner_index) {
                continue;
            }

            let Ok(SimpleTest {
                input,
                expected_output,
            }) = test
            else {
                only_run_at_ran = true;
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
                        "-".repeat((size().unwrap_or((20u16, 1u16)).0 as usize) / 2),
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
                not_success: total_inner_tests,
                total: total_inner_tests,
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
            "🚧 cache for {path:?} not found, start Re-compiling..".yellow()
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
            "🚧 cache for {path:?} not found, start Re-compiling..".yellow()
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
}

fn sha256_digest<R: io::Read>(mut reader: R) -> io::Result<digest::Digest> {
    let mut context = digest::Context::new(&digest::SHA256);
    let mut buffer = [0; 1024];

    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        context.update(&buffer[..count]);
    }

    Ok(context.finish())
}

enum ExecutionInput {
    InheritFromTerminal,
    CustomInput(String),
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
        drop(child_stdin);
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

pub fn run(path: &Path) -> io::Result<()> {
    assert!(path.exists());

    let filename = path.file_name().unwrap().to_str().unwrap();

    let target_file = fs::File::open(path)?;
    let target_reader = io::BufReader::new(target_file);
    let target_hashed = HEXUPPER.encode(sha256_digest(target_reader)?.as_ref());

    let config = get_config()?;

    if let Ok(Some(file_cache)) = get_file(filename) {
        if target_hashed == file_cache.source_hash {
            println!("ℹ️ cache for {path:?} is matched, skip re-compiling..");

            if excute_binary(
                &config.binary_dir_path,
                filename,
                ExecutionInput::InheritFromTerminal,
                false,
            )?
            .is_some()
            {
                return Ok(());
            }
        }
    }

    println!("🚧 Re-compiling binary...");

    recompile_binary(path)?;

    let file_cache = FileCache {
        source_hash: target_hashed,
        tests: Vec::new(),
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

fn limited_print(content: &str, cols: usize, rows: usize) {
    assert_ne!(rows, 0);
    for row in content.lines().take(rows) {
        for col in row.chars().take(cols) {
            print!("{}", col);
        }
        print!("\n");
    }

    io::stdout().flush().unwrap();
}

fn limited_string(content: &str, cols: usize, rows: usize) -> String {
    assert_ne!(rows, 0);

    let mut buffer = String::new();
    for row in content.lines().take(rows) {
        for col in row.chars().take(cols) {
            buffer.push(col);
        }
        buffer.push('\n');
    }

    buffer
}

fn append_extension(extension: &str, path: PathBuf) -> PathBuf {
    let mut os_str = path.into_os_string();
    os_str.push(".");
    os_str.push(extension);
    os_str.into()
}
