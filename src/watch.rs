use crate::error::MorphResult;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const IGNORED_DIRS: &[&str] = &[
    ".git",
    ".morph",
    "target",
    "build",
    "dist",
    "node_modules",
    "__pycache__",
    ".venv",
];

#[derive(Debug, Clone)]
struct FileStamp {
    modified_nanos: u128,
    size: u64,
}

type Snapshot = BTreeMap<PathBuf, FileStamp>;

pub struct FileWatcher {
    paths: Vec<PathBuf>,
    snapshot: Snapshot,
    debounce: Duration,
    interval: Duration,
}

impl FileWatcher {
    pub fn new(paths: Vec<PathBuf>, debounce: Duration) -> MorphResult<Self> {
        let paths = normalise_paths(paths);
        let snapshot = snapshot(&paths)?;
        Ok(Self {
            paths,
            snapshot,
            debounce,
            interval: Duration::from_millis(100),
        })
    }

    pub fn wait_for_change(&mut self) -> MorphResult<Vec<PathBuf>> {
        let mut changed = BTreeSet::new();
        let mut first_change = None;
        loop {
            thread::sleep(self.interval);
            let current = snapshot(&self.paths)?;
            for path in self.snapshot.keys().chain(current.keys()) {
                let before = self.snapshot.get(path);
                let after = current.get(path);
                if !same_stamp(before, after) {
                    changed.insert(path.clone());
                }
            }
            if !changed.is_empty() {
                self.snapshot = current;
                first_change.get_or_insert_with(SystemTime::now);
            }
            if let Some(started) = first_change {
                if started.elapsed().unwrap_or_default() >= self.debounce {
                    return Ok(changed.into_iter().collect());
                }
            }
        }
    }

    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }
}

fn normalise_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut output = Vec::new();
    for path in paths {
        let path = if has_symlink_ancestor(&path) {
            path
        } else {
            path.canonicalize().unwrap_or(path)
        };
        if !output.contains(&path) {
            output.push(path);
        }
    }
    output
}

fn snapshot(paths: &[PathBuf]) -> MorphResult<Snapshot> {
    let mut files = BTreeSet::new();
    for path in paths {
        collect_watch_files(path, &mut files)?;
    }
    let mut result = Snapshot::new();
    for file in files {
        let metadata = match fs::metadata(&file) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        let modified_nanos = metadata
            .modified()
            .unwrap_or(UNIX_EPOCH)
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        result.insert(
            file,
            FileStamp {
                modified_nanos,
                size: metadata.len(),
            },
        );
    }
    Ok(result)
}

fn collect_watch_files(path: &Path, output: &mut BTreeSet<PathBuf>) -> MorphResult<()> {
    if has_symlink_ancestor(path) {
        return Ok(());
    }
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink() {
        return Ok(());
    }
    if metadata.is_file() {
        output.insert(path.to_path_buf());
        return Ok(());
    }
    if !metadata.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let entry_path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !IGNORED_DIRS.contains(&name.as_str()) {
                collect_watch_files(&entry_path, output)?;
            }
        } else if metadata.is_file() {
            output.insert(entry_path);
        }
    }
    Ok(())
}

fn has_symlink_ancestor(path: &Path) -> bool {
    path.ancestors().any(|ancestor| {
        fs::symlink_metadata(ancestor)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
    })
}

fn same_stamp(before: Option<&FileStamp>, after: Option<&FileStamp>) -> bool {
    match (before, after) {
        (Some(before), Some(after)) => {
            before.modified_nanos == after.modified_nanos && before.size == after.size
        }
        (None, None) => true,
        _ => false,
    }
}

pub fn configured_watch_paths(root: &Path, paths: &[String]) -> Vec<PathBuf> {
    if paths.is_empty() {
        return vec![root.to_path_buf()];
    }
    paths.iter().map(|path| root.join(path)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn generated_directories_are_filtered() {
        let root = std::env::temp_dir().join(format!(
            "morph-watch-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("target")).unwrap();
        fs::write(root.join("source.py"), "print(1)\n").unwrap();
        fs::write(root.join("target/generated.py"), "print(2)\n").unwrap();
        let files = snapshot(&[root.clone()]).unwrap();
        assert!(files.contains_key(&root.join("source.py")));
        assert!(!files.contains_key(&root.join("target/generated.py")));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn snapshot_detects_size_and_timestamp_changes() {
        let root = std::env::temp_dir().join(format!(
            "morph-watch-stamp-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let file = root.join("source.c");
        let mut handle = fs::File::create(&file).unwrap();
        handle.write_all(b"one").unwrap();
        let before = snapshot(std::slice::from_ref(&root)).unwrap();
        std::thread::sleep(Duration::from_millis(5));
        fs::write(&file, b"two-two").unwrap();
        let after = snapshot(std::slice::from_ref(&root)).unwrap();
        assert!(!same_stamp(before.get(&file), after.get(&file)));
        fs::remove_dir_all(root).unwrap();
    }
}
