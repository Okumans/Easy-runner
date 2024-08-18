use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::hash_map::HashMap;
use std::fmt::Display;
use std::fs;
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::vec::Vec;

pub const DEFUALT_BIN_DIR: &str = "binary";
pub const DEFAULT_CACHE_FILE: &str = "erunner_cache.json";

// Templates config for langueges compilation
const TEMPLATE_CONFIG_BINARY_DIR: &str = "$(BIN_DIR)";
const TEMPLATE_CONFIG_FILE: &str = "$(FILE)";
const TEMPLATE_CONFIG_FILENAME: &str = "$(FILENAME)";
const TEMPLATE_CONFIG_DIRNAME: &str = "$(DIR)";
const TEMPLATE_CONFIG_DIR: &str = "$(DIRNAME)";
const TEMPLATE_CONFIG_EXE_EXTENSION: &str = "$(EXE_EXT)";

const README_CONTENT: &str = r#"
# Compiled Binary File Instructions

Compiled binary file needs to be named `$(FILENAME).$(EXE_EXT)` and placed in the `$(BIN_DIR)` directory.

## Available Macros

- `$(FILE)`: file path
- `$(FILENAME)`: filename with extension
- `$(DIR)`: file directory path
- `$(DIRNAME)`: file directory name
- `$(BIN_DIR)`: binary directory path
- `$(EXE_EXT)`: binary extension based on OS
"#;

#[derive(Serialize, Deserialize, Clone)]
pub struct Files {
    pub binary_dir_path: PathBuf,
    pub files: HashMap<String, FileCache>,
    pub languages_config: HashMap<String, String>,
    pub no_readme: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct FileCache {
    pub source_hash: String,
    pub tests: Vec<Test>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Test {
    StringTest {
        input: String,
        expected_output: String,
    },
    RefTest {
        input: PathBuf,
        expected_output: Option<PathBuf>,
    },
}

impl Display for Test {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Test::StringTest {
                input,
                expected_output,
            } => {
                write!(f, "Test: ({}), Expected: ({})", input, expected_output)
            }
            Test::RefTest {
                input,
                expected_output,
            } => {
                write!(
                    f,
                    "Test from file: ({:?}), Expected from file: ({:?})",
                    input, expected_output
                )
            }
        }
    }
}

pub fn template_config_replacement(
    config: &mut String,
    binary_dir_path: &Path,
    source_path: &Path,
) -> Result<(), &'static str> {
    // Replace TEMPLATE_CONFIG_BINARY_DIR with binary directory path
    *config = config.replace(
        TEMPLATE_CONFIG_BINARY_DIR,
        binary_dir_path
            .to_str()
            .ok_or("Invalid binary directory path")?,
    );

    // Replace TEMPLATE_CONFIG_FILENAME with the file stem of the source path
    *config = config.replace(
        TEMPLATE_CONFIG_FILENAME,
        source_path
            .file_name()
            .ok_or("Invalid source file stem")?
            .to_str()
            .ok_or("Invalid source file stem")?,
    );

    // Replace TEMPLATE_CONFIG_FILE with the full source file path
    *config = config.replace(
        TEMPLATE_CONFIG_FILE,
        source_path.to_str().ok_or("Invalid source file path")?,
    );

    // Replace TEMPLATE_CONFIG_DIRNAME with the parent directory name of the source path
    *config = config.replace(
        TEMPLATE_CONFIG_DIRNAME,
        source_path
            .parent()
            .ok_or("Invalid source file parent directory")?
            .file_name()
            .ok_or("Invalid source file parent directory name")?
            .to_str()
            .ok_or("Invalid source file parent directory name")?,
    );

    // Replace TEMPLATE_CONFIG_DIR with the parent directory path of the source path
    *config = config.replace(
        TEMPLATE_CONFIG_DIR,
        source_path
            .parent()
            .ok_or("Invalid source file parent directory")?
            .to_str()
            .ok_or("Invalid source file parent directory")?,
    );

    // Replace TEMPLATE_CONFIG_EXE_EXTENSION with target execuatable in current os
    *config = config.replace(
        TEMPLATE_CONFIG_EXE_EXTENSION,
        if cfg!(windows) { "exe" } else { "out" },
    );

    Ok(())
}

pub fn initialize(current_path: PathBuf) -> io::Result<()> {
    if !current_path.join(DEFAULT_CACHE_FILE).is_file() {
        println!("❌ Cannot locate {DEFAULT_CACHE_FILE}, creating..");

        let mut binary_dir_path = current_path.clone();
        let mut no_readme = false;

        if !Path::new(DEFUALT_BIN_DIR).is_dir() {
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

            dbg!(&binary_dir_path);

            if !binary_dir_path.is_dir() {
                fs::create_dir(&binary_dir_path)?;
            }
        }

        if !current_path.join("README.md").exists() {
            print!("❓ README.md not found, Would you like to create one? [y/n]? (\"NE\" to no show this again.) ");
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin()
                .read_line(&mut input)
                .expect("Failed to read line.");

            no_readme = match input.trim() {
                "y" | "Y" => {
                    let mut read_me_file = fs::File::create(current_path.join("README.md"))?;
                    read_me_file.write_all(README_CONTENT.as_bytes())?;
                    println!("✅ README.md created successfuly.");
                    false
                }
                "NE" => true,
                _ => false,
            };
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

        let files: Files = Files {
            binary_dir_path: binary_dir_path.clone(),
            files: HashMap::new(),
            languages_config,
            no_readme,
        };

        let file = fs::File::create(current_path.join(DEFAULT_CACHE_FILE))?;
        let mut writer = io::BufWriter::new(file);

        serde_json::to_writer_pretty(&mut writer, &files)?;
        writer.flush()?;
    }

    Ok(())
}

pub fn get_file(filename: &str) -> io::Result<Option<FileCache>> {
    let cache_file_path = Path::new(".").join(DEFAULT_CACHE_FILE);

    if !cache_file_path.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Cache file not found",
        ));
    }

    let file = fs::File::open(&cache_file_path)?;
    let reader = io::BufReader::new(file);

    let mut files: Files = serde_json::from_reader(reader)?;
    Ok(files.files.remove(filename))
}

pub fn put_file(filename: &str, file_cache: FileCache) -> io::Result<()> {
    let cache_file_path = Path::new(".").join(DEFAULT_CACHE_FILE);

    if !cache_file_path.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Cache file not found",
        ));
    }

    let file = fs::File::open(&cache_file_path)?;
    let reader = io::BufReader::new(file);
    let mut files: Files = serde_json::from_reader(reader)?;

    files.files.insert(filename.to_string(), file_cache);

    let file = fs::File::create(&cache_file_path)?;
    let mut writer = io::BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, &files)?;

    Ok(())
}

pub fn get_config() -> io::Result<Files> {
    let cache_file_path = Path::new(".").join(DEFAULT_CACHE_FILE);

    if !cache_file_path.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Cache file not found",
        ));
    }

    let file = fs::File::open(&cache_file_path)?;
    let reader = io::BufReader::new(file);

    let files: Files = serde_json::from_reader(reader)?;
    Ok(files)
}
