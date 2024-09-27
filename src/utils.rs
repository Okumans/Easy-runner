use ring::digest;
use std::io::Read;
use std::io::{self};
use std::path::PathBuf;

// Define the macro in a module
pub mod logging {
    #[allow(unused_imports)]
    use colored::Colorize;

    #[macro_export]
    macro_rules! log {
        (info, $($arg:tt)*) => {
            println!("{}: {}", "[Info]".blue().bold(), format!($($arg)*).bright_blue());
        };
        (warn, $($arg:tt)*) => {
            println!("🚧: {}", format!($($arg)*).yellow());
        };
        (error, $($arg:tt)*) => {
            println!("❌: {}", format!($($arg)*).red());
        };
        (question, $($arg:tt)*) => {
            print!("❓: {}", format!($($arg)*).bright_blue());
        };
        (success, $($arg:tt)*) => {
            println!("✅: {}", format!($($arg)*).green());
        };
        ($($arg:tt)*) => {
            println!("{}: {}", "[Info]".bold().blue(), format!($($arg)*).blue());
        };
    }
}

pub fn sha256_digest<R: Read>(mut reader: R) -> io::Result<digest::Digest> {
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

// pub fn limited_print(content: &str, cols: usize, rows: usize) {
//     assert_ne!(rows, 0);
//     for row in content.lines().take(rows) {
//         for col in row.chars().take(cols) {
//             print!("{}", col);
//         }
//         println!();
//     }
//
//     io::stdout().flush().expect("Unable to flush stdout.");
// }

pub fn limited_string(content: &str, cols: usize, rows: usize, truncated: bool) -> String {
    let mut limited_content = String::new();
    let mut current_rows = 0;

    if truncated {
        let mut current_row_len = 0;

        for ch in content.chars() {
            if current_row_len >= cols {
                limited_content.push('\n');
                current_row_len = 0;
                current_rows += 1;

                if current_rows >= rows {
                    limited_content.push_str("...");
                    break;
                }
            }

            limited_content.push(ch);
            current_row_len += 1;
        }

        limited_content
    } else {
        #[allow(clippy::explicit_counter_loop)]
        for line in content.lines() {
            if current_rows >= rows {
                limited_content.push_str("...");
                break;
            }

            let limited_line = if line.chars().count() > cols {
                line.chars().take(cols - 3).collect::<String>() + "..."
            } else {
                line.to_string()
            };

            limited_content.push_str(&limited_line);
            limited_content.push('\n');
            current_rows += 1;
        }

        limited_content.trim_end().to_string()
    }
}

pub fn append_extension(extension: &str, path: PathBuf) -> PathBuf {
    let mut os_str = path.into_os_string();
    os_str.push(".");
    os_str.push(extension);
    os_str.into()
}

pub fn padded_string(content: &str, cols: usize, rows: usize, single_line: bool) -> String {
    limited_string(content, cols - 2, rows, single_line)
        .lines()
        .map(|line| format!("  {}", line)) // Add 2 spaces padding to each line
        .collect::<Vec<String>>()
        .join("\n")
}
