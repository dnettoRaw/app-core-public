// =============================================================================
//        #######
//     ###       ###     F: filesystem.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/02 13:24:05 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 13:24:05 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::path::Path;

pub(crate) fn read_regular_file_bounded(path: &Path, max_bytes: usize) -> io::Result<Vec<u8>> {
    reject_symlink(path)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options.open(path)?;
    reject_non_regular(&file)?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take((max_bytes as u64).saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() > max_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "regular file exceeds configured read limit",
        ));
    }
    Ok(bytes)
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
