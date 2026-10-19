use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Write a small protocol file without exposing a partially written payload.
///
/// The temporary file stays beside the destination so the final replacement
/// is on one filesystem. Windows does not let `std::fs::rename` replace an
/// existing file, so use the native replace operation there.
pub fn write_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, contents)?;
    replace_file(&temporary, path)
}

/// Return a path form that native Windows command-line tools understand.
///
/// `Path::canonicalize` commonly returns the extended path prefix
/// (`\\?\\UNC\\...`) for WSL shares. Rust can use that form, but MinGW tools
/// often misparse it as a relative path. Strip only the extended prefix when
/// handing paths to external Windows programs.
#[cfg(windows)]
pub fn external_path(path: &Path) -> PathBuf {
    let value = path.to_string_lossy();
    if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = value.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path.to_path_buf()
    }
}

#[cfg(not(windows))]
pub fn external_path(path: &Path) -> PathBuf {
    path.to_path_buf()
}

#[cfg(not(windows))]
fn replace_file(from: &Path, to: &Path) -> io::Result<()> {
    fs::rename(from, to)
}

#[cfg(windows)]
fn replace_file(from: &Path, to: &Path) -> io::Result<()> {
    use std::thread;
    use std::time::Duration;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x0000_0001;
    const MOVEFILE_COPY_ALLOWED: u32 = 0x0000_0002;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x0000_0008;

    let from = wide_path(from);
    let to = wide_path(to);
    let flags = MOVEFILE_REPLACE_EXISTING | MOVEFILE_COPY_ALLOWED | MOVEFILE_WRITE_THROUGH;
    let mut last_error = None;
    // WSL's Windows filesystem bridge and antivirus scanners can briefly keep
    // the previous protocol file open. Retry before reporting an indeterminate
    // reload to the caller.
    for _ in 0..40 {
        let replaced = unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), flags) };
        if replaced != 0 {
            return Ok(());
        }
        last_error = Some(io::Error::last_os_error());
        thread::sleep(Duration::from_millis(25));
    }
    Err(last_error.unwrap_or_else(|| io::Error::other("Windows file replacement failed")))
}

#[cfg(windows)]
fn wide_path(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;

    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn MoveFileExW(existing_file_name: *const u16, new_file_name: *const u16, flags: u32) -> i32;
}

#[cfg(test)]
mod tests {
    use super::write_atomic;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn atomic_write_replaces_an_existing_protocol_file() {
        let directory = std::env::temp_dir().join(format!(
            "morph-file-ops-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("status.json");
        write_atomic(&path, b"first").unwrap();
        write_atomic(&path, b"second").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "second");
        assert!(!directory.join("status.tmp").exists());
        fs::remove_dir_all(directory).unwrap();
    }
}
