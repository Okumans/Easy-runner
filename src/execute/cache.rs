use super::{put_config, sha256_digest, Files};
use crate::{
    cache_file::{get_config, FileCache},
    execute::core::recompile_binary,
    log,
};

use colored::Colorize;
use data_encoding::HEXUPPER;
use std::fs;
use std::{collections::HashMap, io, path::Path};

pub fn purge() -> io::Result<()> {
    log!(warn, "This operation will permanently remove all registered tests. This action cannot be undone.");
    log!(question, "Are you sure you want to proceed? (Y/n): ");

    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;

    if answer.trim() != "Y" {
        return Ok(());
    }

    let config = get_config()?;
    let files_numbers = config.files.len();

    put_config(Files {
        files: HashMap::new(),
        ..config
    })?;

    log!(
        info,
        "Purged all {} file{} from the configuration.",
        files_numbers,
        if files_numbers > 1 { "s" } else { "" }
    );
    Ok(())
}

pub fn clean() -> io::Result<()> {
    let config = get_config()?;

    let original_files_length = config.files.len();
    let mut cleaned_files: HashMap<String, FileCache> = HashMap::new();

    for (filename, file_cache) in config.files {
        if Path::new(&filename).exists() {
            cleaned_files.insert(filename, file_cache);
        }
    }

    let cleaned_files_length = original_files_length - cleaned_files.len();

    put_config(Files {
        files: cleaned_files,
        ..config
    })?;

    log!(
        info,
        "Removed {} out-of-date file{} from the configuration.",
        cleaned_files_length,
        if cleaned_files_length > 1 { "s" } else { "" }
    );

    Ok(())
}

pub fn recompile(all: bool) -> io::Result<()> {
    let mut config = get_config()?;
    let mut recompiled_numbers = 0u32;

    for (filename, file_cache) in &mut config.files {
        if Path::new(filename).exists() {
            let target_hashed = HEXUPPER
                .encode(sha256_digest(io::BufReader::new(fs::File::open(filename)?))?.as_ref());

            if all || target_hashed != file_cache.source_hash {
                if let Err(recompile_error) =
                    recompile_binary(&fs::canonicalize(Path::new(&filename))?).map_err(|err| {
                        io::Error::new(io::ErrorKind::Other, format!("Recompilation error: {err}"))
                    })
                {
                    log!(error, "Recompilation failed for file '{filename}'. Error details: {recompile_error}");
                    continue;
                }

                file_cache.source_hash = target_hashed;
                recompiled_numbers += 1;
            }
        }
    }

    put_config(config)?;

    log!(
        info,
        "Successfully recompiled {} file{}.",
        recompiled_numbers,
        if recompiled_numbers > 1 { "s" } else { "" }
    );

    Ok(())
}
