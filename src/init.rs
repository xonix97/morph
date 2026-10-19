use crate::error::{MorphError, MorphResult};
use std::fs;
use std::path::{Path, PathBuf};

const TEMPLATE_FILES: &[&str] = &[
    "app.py",
    "app_logic.py",
    "morph.toml",
    ".python-version",
    "README.md",
];

pub fn run(args: &[String]) -> MorphResult<()> {
    let (destination, force) = parse_args(args)?;
    scaffold_python_template(&destination, force)
}

fn parse_args(args: &[String]) -> MorphResult<(PathBuf, bool)> {
    let mut destination = None;
    let mut force = false;
    for argument in args {
        match argument.as_str() {
            "--force" | "-f" => force = true,
            value if value.starts_with('-') => {
                return Err(MorphError::InvalidArguments(format!(
                    "unknown init option {value:?}"
                )))
            }
            value if destination.is_none() => destination = Some(PathBuf::from(value)),
            value => {
                return Err(MorphError::InvalidArguments(format!(
                    "unexpected init path argument {value:?}"
                )))
            }
        }
    }
    Ok((destination.unwrap_or_else(|| PathBuf::from(".")), force))
}

fn scaffold_python_template(destination: &Path, force: bool) -> MorphResult<()> {
    let destination = if destination.exists() {
        destination.canonicalize()?
    } else {
        std::env::current_dir()?.join(destination)
    };
    fs::create_dir_all(&destination)?;

    let conflicts = TEMPLATE_FILES
        .iter()
        .map(|file| destination.join(file))
        .filter(|path| path.exists())
        .collect::<Vec<_>>();
    if !force && !conflicts.is_empty() {
        let names = conflicts
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(MorphError::InvalidArguments(format!(
            "init would overwrite existing files: {names}; choose an empty directory or pass --force"
        )));
    }

    for file in TEMPLATE_FILES {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("templates")
            .join("python-controlled")
            .join(file);
        let target = destination.join(file);
        fs::write(&target, fs::read(&source)?)?;
    }
    println!(
        "Created Morph Python template in {}. Run `morph dev {}` to start it.",
        destination.display(),
        destination.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_directory() -> PathBuf {
        std::env::temp_dir().join(format!(
            "morph-init-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn creates_python_template_files() {
        let directory = temporary_directory();
        scaffold_python_template(&directory, false).unwrap();
        for file in TEMPLATE_FILES {
            assert!(directory.join(file).is_file(), "missing {file}");
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn refuses_to_overwrite_without_force() {
        let directory = temporary_directory();
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("app.py"), "keep me\n").unwrap();
        let result = scaffold_python_template(&directory, false);
        assert!(matches!(result, Err(MorphError::InvalidArguments(_))));
        assert_eq!(
            fs::read_to_string(directory.join("app.py")).unwrap(),
            "keep me\n"
        );
        fs::remove_dir_all(directory).unwrap();
    }
}
