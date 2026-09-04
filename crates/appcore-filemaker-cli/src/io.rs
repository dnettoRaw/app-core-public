// =============================================================================
//        #######
//     ###       ###     F: io.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded io contracts and behavior for this crate.

use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::failure::{CliFailure, CliResult, EXIT_CANTCREATE, EXIT_IO, EXIT_NOINPUT};

pub(crate) fn read_bounded(path: &Path, limit: usize, json: bool) -> CliResult<Vec<u8>> {
    let mut file = OpenOptions::new().read(true).open(path).map_err(|error| {
        CliFailure::io(
            EXIT_NOINPUT,
            "FM-CLI-NOINPUT",
            format!("cannot open `{}`: {error}", path.display()),
            json,
        )
    })?;
    let metadata = file.metadata().map_err(|error| {
        CliFailure::io(
            EXIT_NOINPUT,
            "FM-CLI-NOINPUT",
            format!("cannot inspect `{}`: {error}", path.display()),
            json,
        )
    })?;
    let length = usize::try_from(metadata.len()).map_err(|_| {
        CliFailure::io(
            EXIT_NOINPUT,
            "FM-CLI-NOINPUT",
            "input length exceeds platform range",
            json,
        )
    })?;
    if !metadata.is_file() || length > limit {
        return Err(CliFailure::io(
            EXIT_NOINPUT,
            "FM-CLI-NOINPUT",
            format!("`{}` is not a bounded regular file", path.display()),
            json,
        ));
    }
    let mut bytes = Vec::new();
    let read_limit = u64::try_from(limit)
        .ok()
        .and_then(|value| value.checked_add(1))
        .ok_or_else(|| {
            CliFailure::io(
                EXIT_NOINPUT,
                "FM-CLI-LIMIT",
                format!("input limit is too large for `{}`", path.display()),
                json,
            )
        })?;
    Read::by_ref(&mut file)
        .take(read_limit)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            CliFailure::io(
                EXIT_IO,
                "FM-CLI-IO",
                format!("cannot read `{}`: {error}", path.display()),
                json,
            )
        })?;
    if bytes.len() > limit {
        return Err(CliFailure::io(
            EXIT_NOINPUT,
            "FM-CLI-NOINPUT",
            "input changed beyond its byte limit while reading",
            json,
        ));
    }
    Ok(bytes)
}

pub(crate) fn atomic_write(path: &Path, bytes: &[u8], json: bool) -> CliResult<()> {
    let parent = path
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = fs::canonicalize(parent).map_err(|error| {
        CliFailure::io(
            EXIT_CANTCREATE,
            "FM-CLI-CANTCREATE",
            format!("cannot resolve output parent: {error}"),
            json,
        )
    })?;
    if !parent.is_dir() {
        return Err(CliFailure::io(
            EXIT_CANTCREATE,
            "FM-CLI-CANTCREATE",
            "output parent is not a directory",
            json,
        ));
    }
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            CliFailure::io(
                EXIT_CANTCREATE,
                "FM-CLI-CANTCREATE",
                "output filename is missing or non-UTF-8",
                json,
            )
        })?;
    let target = parent.join(file_name);
    let (temporary, mut file) = create_temporary(&parent, file_name, json)?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, &target)?;
        if let Ok(directory) = OpenOptions::new().read(true).open(&parent) {
            let _ = directory.sync_all();
        }
        Ok::<(), std::io::Error>(())
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&temporary);
        return Err(CliFailure::io(
            EXIT_IO,
            "FM-CLI-IO",
            format!("cannot atomically write `{}`: {error}", target.display()),
            json,
        ));
    }
    Ok(())
}

pub(crate) fn ensure_distinct_output(input: &Path, output: &Path, json: bool) -> CliResult<()> {
    let input = fs::canonicalize(input).map_err(|error| {
        CliFailure::io(
            EXIT_NOINPUT,
            "FM-CLI-NOINPUT",
            format!("cannot resolve input: {error}"),
            json,
        )
    })?;
    let output = match fs::canonicalize(output) {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let parent = output
                .parent()
                .filter(|value| !value.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            let parent = fs::canonicalize(parent).map_err(|parent_error| {
                CliFailure::io(
                    EXIT_CANTCREATE,
                    "FM-CLI-CANTCREATE",
                    format!("cannot resolve output parent: {parent_error}"),
                    json,
                )
            })?;
            let name = output.file_name().ok_or_else(|| {
                CliFailure::io(
                    EXIT_CANTCREATE,
                    "FM-CLI-CANTCREATE",
                    "output filename is missing",
                    json,
                )
            })?;
            parent.join(name)
        }
        Err(error) => {
            return Err(CliFailure::io(
                EXIT_CANTCREATE,
                "FM-CLI-CANTCREATE",
                format!("cannot resolve output: {error}"),
                json,
            ))
        }
    };
    if input == output {
        return Err(CliFailure::usage(
            "output must not replace the input template",
            json,
        ));
    }
    Ok(())
}

fn create_temporary(parent: &Path, file_name: &str, json: bool) -> CliResult<(PathBuf, fs::File)> {
    for sequence in 0..32_u8 {
        let candidate = parent.join(format!(
            ".{file_name}.{}.{}.tmp",
            std::process::id(),
            sequence
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => return Ok((candidate, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(CliFailure::io(
                    EXIT_CANTCREATE,
                    "FM-CLI-CANTCREATE",
                    format!("cannot create output temporary file: {error}"),
                    json,
                ))
            }
        }
    }
    Err(CliFailure::io(
        EXIT_CANTCREATE,
        "FM-CLI-CANTCREATE",
        "cannot reserve a unique output temporary file",
        json,
    ))
}
