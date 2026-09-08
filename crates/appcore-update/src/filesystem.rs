// =============================================================================
//        #######
//     ###       ###     F: filesystem.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/02 13:24:05 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 13:24:05 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Safe bounded reads for update metadata, indexes and opaque artifacts.
//!
//! Declared size is checked before allocation and a fixed scratch buffer reads
//! one non-retained sentinel byte so concurrent growth fails closed.

use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::path::Path;

const READ_SCRATCH_BYTES: usize = 16 * 1024;

pub(crate) fn read_regular_file_bounded(path: &Path, max_bytes: usize) -> io::Result<Vec<u8>> {
    let file = open_regular_file(path)?;
    let declared_length = file.metadata()?.len();
    read_bounded(file, declared_length, max_bytes)
}

fn read_bounded(reader: impl Read, declared_length: u64, max_bytes: usize) -> io::Result<Vec<u8>> {
    let max_bytes_u64 = u64::try_from(max_bytes).map_err(|_| invalid_read_limit())?;
    if declared_length > max_bytes_u64 {
        return Err(size_limit_error());
    }
    let read_limit = max_bytes_u64
        .checked_add(1)
        .ok_or_else(invalid_read_limit)?;
    let capacity = usize::try_from(declared_length).map_err(|_| invalid_read_limit())?;
    let mut bytes = Vec::with_capacity(capacity);
    let mut reader = reader.take(read_limit);
    let mut scratch = [0u8; READ_SCRATCH_BYTES];
    loop {
        let read = reader.read(&mut scratch)?;
        if read == 0 {
            return Ok(bytes);
        }
        let Some(new_length) = bytes.len().checked_add(read) else {
            return Err(size_limit_error());
        };
        if new_length > max_bytes {
            return Err(size_limit_error());
        }
        bytes.reserve_exact(read);
        bytes.extend_from_slice(&scratch[..read]);
    }
}

fn size_limit_error() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "regular file exceeds configured read limit",
    )
}

fn invalid_read_limit() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "configured read limit exceeds platform range",
    )
}

pub(crate) fn open_regular_file(path: &Path) -> io::Result<File> {
    reject_symlink(path)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path)?;
    reject_non_regular(&file)?;
    Ok(file)
}

fn reject_symlink(path: &Path) -> io::Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path must not be a symlink",
        ));
    }
    Ok(())
}

fn reject_non_regular(file: &File) -> io::Result<()> {
    if file.metadata()?.is_file() {
        return Ok(());
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "path is not a regular file",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn bounded_reader_accepts_exact_limit_and_detects_growth() {
        assert_eq!(read_bounded(Cursor::new(b"test"), 4, 4).unwrap(), b"test");
        assert_eq!(
            read_bounded(Cursor::new(b"tests"), 4, 4)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn declared_oversize_is_rejected_before_reader_is_polled() {
        struct PanicReader;

        impl Read for PanicReader {
            fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
                panic!("oversized update reader must not be polled");
            }
        }

        assert_eq!(
            read_bounded(PanicReader, 5, 4).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn sparse_oversize_file_is_rejected_by_metadata() {
        let path =
            std::env::temp_dir().join(format!("appcore-update-read-limit-{}", std::process::id()));
        let _ = std::fs::remove_file(&path);
        File::create(&path).unwrap().set_len(5).unwrap();

        assert_eq!(
            read_regular_file_bounded(&path, 4).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 5);
        std::fs::remove_file(path).unwrap();
    }
}
