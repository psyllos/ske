use serde::{Deserialize, Serialize};
use std::path::{Component, Path};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Profile {
    pub profile_version: u32,
    pub language: String,
    pub target: Option<String>,
    pub features: Vec<String>,
    pub cfg: Vec<String>,
    pub entry: String,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            profile_version: 1,
            language: "rust".into(),
            target: None,
            features: vec![],
            cfg: vec![],
            entry: "src/lib.rs".into(),
        }
    }
}

impl Profile {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut p: Self = if text.trim().is_empty() {
            Self::default()
        } else {
            serde_json::from_str(text).map_err(|e| e.to_string())?
        };
        if p.profile_version != 1 || p.language != "rust" {
            return Err("profile_version must be 1 and language must be rust".into());
        }
        if p.entry.is_empty()
            || !Path::new(&p.entry)
                .components()
                .all(|c| matches!(c, Component::Normal(_)))
            || !p.entry.ends_with(".rs")
        {
            return Err("entry must be a relative in-project .rs path".into());
        }
        for flag in p.features.iter().chain(&p.cfg).chain(p.target.iter()) {
            if flag.is_empty() || flag.starts_with('-') || flag.contains(['\0', '\r', '\n']) {
                return Err("invalid cfg, feature, or target".into());
            }
        }
        p.features.sort();
        p.features.dedup();
        p.cfg.sort();
        p.cfg.dedup();
        Ok(p)
    }
}
