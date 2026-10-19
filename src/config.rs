use crate::error::{MorphError, MorphResult};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct MorphConfig {
    pub path: Option<PathBuf>,
    pub backend: Option<String>,
    pub c: CConfig,
    pub python: PythonConfig,
}

#[derive(Debug, Clone, Default)]
pub struct CConfig {
    pub plugin_source: Option<String>,
    pub plugin_output: Option<String>,
    pub build: Vec<String>,
    pub host_command: Vec<String>,
    pub control_file: Option<String>,
    pub status_file: Option<String>,
    pub watch: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct PythonConfig {
    pub command: Vec<String>,
    pub modules: Vec<String>,
    pub imports: Vec<String>,
    pub control_file: Option<String>,
    pub status_file: Option<String>,
    pub watch: Vec<String>,
}

impl MorphConfig {
    pub fn load(root: &Path) -> MorphResult<Self> {
        let path = root.join("morph.toml");
        if !path.is_file() {
            return Ok(Self::default());
        }

        let source = fs::read_to_string(&path).map_err(|error| {
            MorphError::Config(format!("cannot read {}: {error}", path.display()))
        })?;
        let values = parse_toml_subset(&source)?;
        let mut config = Self {
            path: Some(path),
            ..Self::default()
        };
        config.backend = values
            .get("morph.backend")
            .and_then(|value| value.first().cloned());
        config.c.plugin_source = values
            .get("c.plugin_source")
            .and_then(|value| value.first().cloned());
        config.c.plugin_output = values
            .get("c.plugin_output")
            .and_then(|value| value.first().cloned());
        config.c.build = values.get("c.build").cloned().unwrap_or_default();
        config.c.host_command = values.get("c.host_command").cloned().unwrap_or_default();
        config.c.control_file = values
            .get("c.control_file")
            .and_then(|value| value.first().cloned());
        config.c.status_file = values
            .get("c.status_file")
            .and_then(|value| value.first().cloned());
        config.c.watch = values.get("c.watch").cloned().unwrap_or_default();
        config.python.command = values.get("python.command").cloned().unwrap_or_default();
        config.python.modules = values.get("python.modules").cloned().unwrap_or_default();
        config.python.imports = values.get("python.imports").cloned().unwrap_or_default();
        config.python.control_file = values
            .get("python.control_file")
            .and_then(|value| value.first().cloned());
        config.python.status_file = values
            .get("python.status_file")
            .and_then(|value| value.first().cloned());
        config.python.watch = values.get("python.watch").cloned().unwrap_or_default();
        Ok(config)
    }

    pub fn has_c_integration(&self) -> bool {
        self.c.plugin_source.is_some() && !self.c.host_command.is_empty()
    }

    pub fn has_python_integration(&self) -> bool {
        !self.python.command.is_empty() && !self.python.modules.is_empty()
    }
}

fn parse_toml_subset(source: &str) -> MorphResult<BTreeMap<String, Vec<String>>> {
    let mut section = String::new();
    let mut values = BTreeMap::new();
    for (line_number, original) in source.lines().enumerate() {
        let line = strip_comment(original).trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].trim().to_string();
            if section.is_empty() {
                return Err(MorphError::Config(format!(
                    "empty section at line {}",
                    line_number + 1
                )));
            }
            continue;
        }
        let Some((key, raw_value)) = line.split_once('=') else {
            return Err(MorphError::Config(format!(
                "expected key = value at line {}",
                line_number + 1
            )));
        };
        let key = key.trim();
        if key.is_empty() {
            return Err(MorphError::Config(format!(
                "empty key at line {}",
                line_number + 1
            )));
        }
        let full_key = if section.is_empty() {
            key.to_string()
        } else {
            format!("{section}.{key}")
        };
        values.insert(
            full_key,
            parse_value(raw_value.trim()).map_err(|message| {
                MorphError::Config(format!("line {}: {message}", line_number + 1))
            })?,
        );
    }
    Ok(values)
}

fn strip_comment(line: &str) -> &str {
    let mut quoted = false;
    for (index, character) in line.char_indices() {
        if character == '"' {
            quoted = !quoted;
        } else if character == '#' && !quoted {
            return &line[..index];
        }
    }
    line
}

fn parse_value(raw: &str) -> Result<Vec<String>, String> {
    if raw.starts_with('[') {
        if !raw.ends_with(']') {
            return Err("array is missing a closing ]".to_string());
        }
        let inner = &raw[1..raw.len() - 1];
        return split_csv(inner)
            .into_iter()
            .map(parse_scalar)
            .collect::<Result<Vec<_>, _>>();
    }
    Ok(vec![parse_scalar(raw)?])
}

fn split_csv(raw: &str) -> Vec<&str> {
    let mut values = Vec::new();
    let mut start = 0;
    let mut quoted = false;
    for (index, character) in raw.char_indices() {
        if character == '"' {
            quoted = !quoted;
        } else if character == ',' && !quoted {
            values.push(raw[start..index].trim());
            start = index + character.len_utf8();
        }
    }
    if !raw[start..].trim().is_empty() {
        values.push(raw[start..].trim());
    }
    values
}

fn parse_scalar(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        let value = &trimmed[1..trimmed.len() - 1];
        return Ok(value.replace("\\\"", "\"").replace("\\\\", "\\"));
    }
    if trimmed == "true" || trimmed == "false" || trimmed.parse::<f64>().is_ok() {
        return Ok(trimmed.to_string());
    }
    Err(format!(
        "expected a quoted string, boolean, or number; got {trimmed:?}"
    ))
}
