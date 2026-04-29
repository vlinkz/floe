use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use libappstream::ComponentKind;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub const BUILD_JSON_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemRecord {
    pub schema_version: u32,
    pub app_id: String,
    pub main_program: String,
    pub system: String,
    pub source: Source,
    #[serde(default, skip_serializing_if = "AppMetadata::is_empty")]
    pub metadata: AppMetadata,
    pub attr: String,
    pub version: String,
    pub store_path: String,
    pub nar_hash: String,
    pub closure_size: u64,
    #[serde(default)]
    pub unfree: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream: Option<UpstreamRecord>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub wrappers: BTreeMap<String, Vec<String>>,
    pub generated: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamRecord {
    pub store_path: String,
    pub nar_hash: String,
    pub closure_size: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppMetadata {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "serialize_component_kind",
        deserialize_with = "deserialize_component_kind"
    )]
    pub component_type: Option<ComponentKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub long_description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
}

impl AppMetadata {
    pub fn is_empty(&self) -> bool {
        self.component_type.is_none()
            && self.summary.is_none()
            && self.long_description.is_none()
            && self.homepage.is_none()
            && self.license.is_none()
    }
}

fn serialize_component_kind<S: Serializer>(
    kind: &Option<ComponentKind>,
    s: S,
) -> Result<S::Ok, S::Error> {
    match kind {
        None => s.serialize_none(),
        Some(k) => match k.to_str() {
            Some(name) => s.serialize_some(name.as_str()),
            None => s.serialize_none(),
        },
    }
}

fn deserialize_component_kind<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Option<ComponentKind>, D::Error> {
    let raw = Option::<String>::deserialize(d)?;
    Ok(raw.map(|s| ComponentKind::from_string(&s)))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum Source {
    #[serde(rename = "flake", rename_all = "camelCase")]
    Flake {
        url: String,
        locked_url: String,
        locked_rev: String,
    },
    #[serde(rename = "nilla", rename_all = "camelCase")]
    Nilla {
        url: String,
        hash: String,
        project_file: String,
    },
    #[serde(rename = "legacy", rename_all = "camelCase")]
    Legacy {
        url: String,
        hash: String,
        entry: String,
    },
}

pub struct AppDir<'a> {
    pub root: &'a Path,
    pub app_id: &'a str,
}

impl<'a> AppDir<'a> {
    pub fn new(root: &'a Path, app_id: &'a str) -> Self {
        Self { root, app_id }
    }

    pub fn dir(&self) -> PathBuf {
        self.root.join(self.app_id)
    }

    pub fn system_json(&self, system: &str) -> PathBuf {
        self.dir().join(format!("{system}.json"))
    }
}

impl SystemRecord {
    pub fn write_to(&self, builds_dir: &Path) -> Result<()> {
        let dir = AppDir::new(builds_dir, &self.app_id);
        std::fs::create_dir_all(dir.dir())
            .with_context(|| format!("creating {}", dir.dir().display()))?;
        let path = dir.system_json(&self.system);
        let json = serde_json::to_string_pretty(self).context("serializing SystemRecord")?;
        std::fs::write(&path, format!("{json}\n"))
            .with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }
}
