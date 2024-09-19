use ring::digest;
use std::io::Read;
use std::io::{self, Write};
use std::path::PathBuf;

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

pub fn limited_print(content: &str, cols: usize, rows: usize) {
    assert_ne!(rows, 0);
    for row in content.lines().take(rows) {
        for col in row.chars().take(cols) {
            print!("{}", col);
        }
        println!();
    }

    io::stdout().flush().expect("Unable to flush stdout.");
}

pub fn limited_string(content: &str, cols: usize, rows: usize) -> String {
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

pub fn append_extension(extension: &str, path: PathBuf) -> PathBuf {
    let mut os_str = path.into_os_string();
    os_str.push(".");
    os_str.push(extension);
    os_str.into()
}
