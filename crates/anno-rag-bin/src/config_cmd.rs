//! `anno-rag config` subcommand: init / show / validate.

use anno_rag::config::AnnoRagConfig;
use std::path::{Path, PathBuf};

pub fn config_init(config_path: &Path) -> anyhow::Result<()> {
    if config_path.exists() {
        println!("Config already exists at {}", config_path.display());
        return Ok(());
    }
    let example = include_str!("../../anno-rag/config.toml.example");
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(config_path, example)?;
    println!("Created {}", config_path.display());
    Ok(())
}

pub fn config_show(config_path: Option<&Path>) -> anyhow::Result<()> {
    let path = config_path
        .map(|p| p.to_path_buf())
        .or_else(AnnoRagConfig::default_config_path);

    let file_exists = path.as_ref().map(|p| p.exists()).unwrap_or(false);

    let cfg = AnnoRagConfig::load(None).unwrap_or_else(|e| {
        eprintln!("Warning: {e}; showing defaults");
        AnnoRagConfig::default()
    });

    let as_json: serde_json::Value = serde_json::to_value(&cfg)?;

    let file_cfg_json: Option<serde_json::Value> = if file_exists {
        let contents = std::fs::read_to_string(path.as_ref().unwrap())?;
        let file_cfg: AnnoRagConfig = toml::from_str(&contents)
            .map_err(|e| anyhow::anyhow!("parse error: {e}"))?;
        Some(serde_json::to_value(file_cfg)?)
    } else {
        None
    };

    let defaults_json: serde_json::Value = serde_json::to_value(AnnoRagConfig::default())?;

    let schema = AnnoRagConfig::config_schema();

    for field in schema {
        let effective = as_json
            .get(field.name)
            .map(|v| v.to_string())
            .unwrap_or_default();
        let default = defaults_json
            .get(field.name)
            .map(|v| v.to_string())
            .unwrap_or_default();

        let env_var_set = std::env::var(field.env_var).is_ok();
        let from_file = file_cfg_json
            .as_ref()
            .and_then(|f| f.get(field.name))
            .map(|v| v.to_string())
            .as_deref()
            != Some(&default);

        let source = if env_var_set {
            format!("[env: {}]", field.env_var)
        } else if from_file {
            format!(
                "[file: {}]",
                path.as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default()
            )
        } else {
            "[default]".to_string()
        };

        println!("{:<35} = {:<40} {}", field.name, effective, source);
    }
    Ok(())
}

pub fn config_validate(config_path: &Path) -> anyhow::Result<()> {
    if !config_path.exists() {
        anyhow::bail!("config file not found: {}", config_path.display());
    }
    let contents = std::fs::read_to_string(config_path)?;
    let _cfg: AnnoRagConfig =
        toml::from_str(&contents).map_err(|e| anyhow::anyhow!("invalid config: {e}"))?;
    println!("Config valid: {}", config_path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn config_init_creates_file() {
        let dir = tempdir().expect("tmpdir");
        let path = dir.path().join("config.toml");
        assert!(!path.exists());
        config_init(&path).expect("init");
        assert!(path.exists());
        let contents = std::fs::read_to_string(&path).expect("read");
        assert!(
            contents.contains("anno-rag configuration"),
            "file must contain the example template header"
        );
    }

    #[test]
    fn config_init_does_not_overwrite_existing() {
        let dir = tempdir().expect("tmpdir");
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "custom = true").expect("write");
        config_init(&path).expect("init");
        let contents = std::fs::read_to_string(&path).expect("read");
        assert_eq!(contents, "custom = true");
    }

    #[test]
    fn config_validate_accepts_valid_toml() {
        let dir = tempdir().expect("tmpdir");
        let path = dir.path().join("config.toml");
        std::fs::write(&path, r#"default_top_k = 20"#).expect("write");
        config_validate(&path).expect("should be valid");
    }

    #[test]
    fn config_validate_rejects_missing_file() {
        let dir = tempdir().expect("tmpdir");
        let path = dir.path().join("no-such-file.toml");
        let result = config_validate(&path);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("not found"), "expected 'not found' in: {msg}");
    }
}
