# Easy Runner

Easy Runner is a command-line tool for compiling, running, and testing code with a focus on simplicity and speed. It uses a caching system to avoid unnecessary recompilations, making your development workflow more efficient.

## Features

- **Smart Compilation**: Automatically recompiles files only when they have changed.
- **Test Management**: Add and run tests for your code, either as simple strings or from external files.
- **Language Support**: Easily configurable for different programming languages. Default configurations are provided for C, C++, Python, and Rust.
- **Cache Control**: Manage the cache with commands to clean, purge, or recompile all tracked files.
- **Status Overview**: Get a clear overview of all your tracked files, their status, and associated tests.

## Getting Started

### Initialization

To start using Easy Runner in your project, you first need to initialize it. This will create a `erunner_cache.json` file to store configuration and a `binary` directory for the compiled files.

```bash
erunner init
```

### Running a File

To compile and run a file, use the `run` command:

```bash
erunner run <path/to/your/file>
```

For example:

```bash
erunner run src/main.rs
```

Easy Runner will compile the file if it's the first time or if the file has been modified, and then run the compiled binary.

### Testing

Easy Runner provides a simple yet powerful testing framework.

#### Adding a Test

You can add a test case with a specific input and expected output:

```bash
erunner test <path/to/your/file> add "your input" "your expected output"
```

#### Adding a Test from a File

You can also link a test file:

```bash
erunner test <path/to/your/file> add-link <path/to/your/test_file>
```

#### Running Tests

To run all the tests for a file:

```bash
erunner test <path/to/your/file> run
```

To run a specific test or a range of tests, use the `run-at` command with an expression:

```bash
# Run the first test
erunner test <path/to/your/file> run-at 1

# Run the second sub-test of the first test
erunner test <path/to/your/file> run-at 1.2

# Run sub-tests 2 to 5 of the first test
erunner test <path/to/your/file> run-at 1.2-1.5
```

## Commands

Here is a list of all available commands:

- `erunner init`: Initializes Easy Runner in the current directory.
- `erunner run <file>`: Compiles and runs a file.
- `erunner test <file> add <input> <output>`: Adds a string-based test.
- `erunner test <file> add-link <test_file>`: Adds a test from a file.
- `erunner test <file> run`: Runs all tests for a file.
- `erunner test <file> run-at <expression>`: Runs specific tests.
- `erunner status`: Shows the status of all tracked files.
- `erunner cache clean`: Removes non-existent files from the cache.
- `erunner cache purge`: Clears all data from the cache.
- `erunner cache recompile`: Recompiles all tracked files.

## Configuration

Easy Runner uses a `erunner_cache.json` file to manage its state. This file contains information about tracked files, their hashes, tests, and language configurations. You can customize the compilation commands for each language in this file.

### Available Macros

When defining compilation commands, you can use the following macros:

- `$(FILE)`: The full path to the source file.
- `$(FILENAME)`: The name of the source file with its extension.
- `$(DIR)`: The directory containing the source file.
- `$(DIRNAME)`: The name of the directory containing the source file.
- `$(BIN_DIR)`: The path to the directory where binaries are stored.
- `$(EXE_EXT)`: The appropriate executable extension for the operating system (`.exe` for Windows, `.out` for others).