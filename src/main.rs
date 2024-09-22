use colored::Colorize;
use easy_runner::{execute, log, utils::logging::*};

use clap::{Parser, Subcommand};
use std::{fs, path::PathBuf};

#[derive(Debug, Subcommand)]
enum CommandTest {
    Add {
        input: String,
        output: String,
    },
    AddLink {
        #[arg(help = "Path to the tests")]
        tests: PathBuf,

        #[arg(long, short, help = "Add link as a standalone")]
        standalone: bool,

        #[arg(help = "Output path, required when --standalone is used")]
        output: Option<PathBuf>,
    },
    RunAt {
        expression: String,
    },
    Run,
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
        #[arg(long, short, help = "Force recompilation of the project")]
        force_recompile: bool,
    },

    Status,
    Init,
}

#[derive(Debug, Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

fn main() {
    let current_dir = std::env::current_dir().expect("Cannot find current directory.");
    let args = Cli::parse();

    if !execute::check_initialized(&current_dir) && !matches!(args.command, Command::Init) {
        println!(
            "{} {} {} {}",
            "Error:".red().bold(),
            "Project is not initialized. Please run".white(),
            "\"erunner init\"".yellow().bold(),
            "to initialize the project.".white()
        );

        return;
    }

    match args.command {
        Command::Test { path, command } => {
            if !path.exists() {
                log!(
                    error,
                    "File {path:?} not found. Please make sure it is a valid filename."
                );
                return;
            }

            let path = fs::canonicalize(path).expect("Unable to canonicalize path");

            match command {
                CommandTest::Add { input, output } => {
                    execute::test::add(&path, &input, &output).expect("Failed to add test.");
                }

                CommandTest::AddLink {
                    tests,
                    standalone,
                    output,
                } => {
                    if standalone {
                        // Handle standalone case where both input and output paths are needed
                        if let Some(output) = output {
                            // Check if both input and output files exist
                            if !tests.exists() || !output.exists() {
                                log!(
                                    error,
                                    "Input content file is {}, {} output content file {}, Please make sure both of them are valid filenames.",
                                    if tests.exists() { "exists" } else { "does not exist" },
                                    if !tests.exists() && !output.exists() { "And" } else { "But" },
                                    if output.exists() { "exists" } else { "does not exist" },
                                );
                                return;
                            }

                            let input =
                                fs::canonicalize(tests).expect("Unable to canonicalize input path");
                            let output = fs::canonicalize(output)
                                .expect("Unable to canonicalize output path");

                            // Execute standalone linking logic
                            execute::test::add_standalone_file_link(&path, &input, &output)
                                .expect("Failed to add Linked test.");
                        } else {
                            // If `--standalone` is used but output is missing, show an error
                            eprintln!("Error: --standalone requires an output path.");
                        }
                    } else {
                        // Regular AddLink case where only the `tests` path is needed
                        if !tests.exists() {
                            log!(error, "Test file {:?} doesn't exist.", tests);
                            return;
                        }

                        let tests =
                            fs::canonicalize(tests).expect("Unable to canonicalize tests path");

                        // Execute regular linking logic
                        execute::test::add_file_link(&path, &tests)
                            .expect("Failed to add Linked test.");
                    }
                }

                CommandTest::RunAt { expression } => {
                    execute::test::run_at(&path, &expression)
                        .expect("Failed to run test-at index.");
                }
                CommandTest::Run => {
                    execute::test::run(&path).expect("Failed to run executable.");
                }
            }
        }

        Command::Run {
            path,
            force_recompile,
        } => {
            if !path.exists() {
                log!(
                    error,
                    "File {path:?} not found. Please make sure it is a valid filename."
                );
                return;
            }

            let path = fs::canonicalize(path).expect("Unable to canonicalize path");
            execute::run(&path).expect("Failed to the run file.");
        }

        Command::Status => execute::status().expect("Failed to show status."),
        Command::Init => {
            execute::initialize(&current_dir).expect("Failed to initialize erunner cache.")
        }
    }
}
