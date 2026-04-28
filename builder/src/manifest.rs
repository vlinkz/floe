use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

pub const SUPPORTED_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flake: Option<FlakeSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nilla: Option<NillaSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy: Option<LegacySource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub systems: Vec<String>,
    pub attr: String,
    pub main_program: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FlakeSource {
    pub url: String,
    pub rev: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NillaSource {
    pub url: String,
    pub hash: String,
    #[serde(default = "default_nilla_project_file")]
    pub project_file: String,
}

fn default_nilla_project_file() -> String {
    "nilla.nix".to_owned()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LegacySource {
    pub url: String,
    pub hash: String,
    #[serde(default = "default_legacy_entry")]
    pub entry: String,
}

fn default_legacy_entry() -> String {
    "default.nix".to_owned()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Flake,
    Nilla,
    Legacy,
}

impl SourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Flake => "flake",
            Self::Nilla => "nilla",
            Self::Legacy => "legacy",
        }
    }
}

impl Manifest {
    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading manifest {}", path.display()))?;
        let manifest: Self = serde_json::from_str(&raw)
            .with_context(|| format!("parsing manifest {}", path.display()))?;
        manifest.validate(path)?;
        Ok(manifest)
    }

    fn validate(&self, path: &Path) -> Result<()> {
        let p = || path.display();
        if self.schema_version != SUPPORTED_SCHEMA_VERSION {
            bail!(
                "{}: unsupported schemaVersion {} (expected {})",
                p(),
                self.schema_version,
                SUPPORTED_SCHEMA_VERSION,
            );
        }
        if self.systems.is_empty() {
            bail!("{}: 'systems' must be a non-empty array", p());
        }
        if self.attr.trim().is_empty() {
            bail!("{}: 'attr' must not be empty", p());
        }
        if self.version.as_deref().is_some_and(|v| v.trim().is_empty()) {
            bail!("{}: 'version' must not be empty", p());
        }

        let kinds: Vec<&'static str> = [
            self.flake.as_ref().map(|_| "flake"),
            self.nilla.as_ref().map(|_| "nilla"),
            self.legacy.as_ref().map(|_| "legacy"),
        ]
        .into_iter()
        .flatten()
        .collect();
        match kinds.as_slice() {
            [] => bail!(
                "{}: exactly one of 'flake', 'nilla', or 'legacy' must be set",
                p()
            ),
            [_] => {}
            many => bail!(
                "{}: only one of 'flake', 'nilla', or 'legacy' may be set (got: {})",
                p(),
                many.join(", "),
            ),
        }

        let needs_placeholder =
            matches!(self.kind(), SourceKind::Flake | SourceKind::Nilla) && self.systems.len() > 1;
        if needs_placeholder && !self.attr.contains("{system}") {
            bail!(
                "{}: 'attr' must contain '{{system}}' when more than one system is listed (got: {})",
                p(),
                self.attr,
            );
        }

        Ok(())
    }

    pub fn kind(&self) -> SourceKind {
        match (
            self.flake.is_some(),
            self.nilla.is_some(),
            self.legacy.is_some(),
        ) {
            (true, _, _) => SourceKind::Flake,
            (_, true, _) => SourceKind::Nilla,
            (_, _, true) => SourceKind::Legacy,
            _ => unreachable!("manifest with no source kind passed validation"),
        }
    }

    pub fn attr_for(&self, system: &str) -> String {
        self.attr.replace("{system}", system)
    }
}

#[derive(Debug, Clone)]
pub struct ManifestPath {
    pub path: PathBuf,
    pub component_dir: PathBuf,
    pub component_id: String,
}

impl ManifestPath {
    pub fn new(path: PathBuf) -> Result<Self> {
        let component_dir = path
            .parent()
            .ok_or_else(|| anyhow!("{}: manifest has no parent dir", path.display()))?
            .to_path_buf();
        let component_id = component_dir
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow!("{}: cannot derive app id from path", path.display()))?
            .to_owned();
        Ok(Self {
            path,
            component_dir,
            component_id,
        })
    }
}
