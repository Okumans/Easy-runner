use easy_runner::{cache_file::initialize, execute, test_file};

use clap::{Parser, Subcommand};
use std::{fs, path::PathBuf};

#[derive(Debug, Subcommand)]
enum CommandTest {
    Add { input: String, output: String },
    AddStandaloneLink { input: PathBuf, output: PathBuf },
    AddLink { tests: PathBuf },
    RunAt { index: usize },
    Config,
    Run,
    Show,
}

#[derive(Debug, Subcommand)]
enum Command {
    Test {
        path: PathBuf,
        #[command(subcommand)]
        command: CommandTest,
    },

    Run {
        path: PathBuf,
    },

    Cache,
}

#[derive(Debug, Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

fn main() {
    let current_dir = std::env::current_dir().expect("Cannot find current directory.");
    initialize(current_dir).expect("Failed to initialize easy-runner.");

    let args = Cli::parse();

    match args.command {
        Command::Test { path, command } => {
            if !path.exists() {
                println!("❌ File {path:?} not found. Please make sure it is a valid filename.");
                return;
            }

            let path = fs::canonicalize(path).expect("Unable to canonicalize path");

            match command {
                CommandTest::Add { input, output } => {
                    execute::test::add(&path, &input, &output).expect("Failed to add test.");
                }

                CommandTest::AddStandaloneLink { input, output } => {
                    if !input.exists() || !output.exists() {
                        println!(
                            "❌ Input content file is {}, {} output content file {}, Please make sure both of it is a valid filename.",
                            if input.exists() {"exists"} else {"not exists"},
                            if !input.exists() && !output.exists() {"And"} else {"But"},
                            if output.exists() {"exists"} else {"not exists"},
                        );
                        return;
                    }

                    let input = fs::canonicalize(input).expect("Unable to canonicalize path");
                    let output = fs::canonicalize(output).expect("Unable to canonicalize path");
                    execute::test::add_standalone_file_link(&path, &input, &output)
                        .expect("Failed to add Linked test.");
                }

                CommandTest::AddLink { tests } => {
                    if !tests.exists() {
                        println!("❌ test file {:?} doesn't exists.", tests);
                        return;
                    }

                    let tests = fs::canonicalize(tests).expect("Unable to canonicalize path");
                    execute::test::add_file_link(&path, &tests)
                        .expect("Failed to add Linked test.");
                }

                CommandTest::RunAt { index } => {
                    execute::test::run_at(&path, index).expect("Failed to run test-at index.");
                }
                CommandTest::Run => {
                    execute::test::run(&path).expect("Failed to run executable.");
                }
                CommandTest::Show => execute::test::show(&path).expect("Failed to show tests"),
                CommandTest::Config => {
                    let tests = test_file::read_test_file(&path);

                    match tests {
                        Ok(tests) => {
                            for test in tests {
                                println!("{}", test.expect("failed to unwrap test").to_test());
                            }
                        }
                        Err(err) => {
                            println!("failed to {}", err);
                        }
                    }
                }
            }
        }

        Command::Run { path } => {
            if !path.exists() {
                println!("❌ File {path:?} not found. Please make sure it is a valid filename.");
                return;
            }

            let path = fs::canonicalize(path).expect("Unable to canonicalize path");
            execute::run(&path).expect("Failed to the run file.");
        }
        Command::Cache => println!("Config"),
    }
}
