use std::path::{Path, PathBuf};

use anyhow::Result;
use walkdir::WalkDir;

use crate::manifest::ManifestPath;

pub mod aggregate;
pub mod build_one;
pub mod list;
pub mod regenerate;

pub use aggregate::run_aggregate;
pub use build_one::{run_build, run_build_all};
pub use list::run_list;
pub use regenerate::run_regenerate;

pub struct Ctx {
    pub repo_root: PathBuf,
    pub registry_dir: PathBuf,
    pub builds_dir: PathBuf,
    pub publish_dir: PathBuf,
    pub staging_dir: PathBuf,
}

impl Ctx {
    pub fn new(repo_root: PathBuf, publish_dir: Option<PathBuf>) -> Self {
        let var_dir = repo_root.join("var");
        let registry_dir = repo_root.join("registry");
        let builds_dir = repo_root.join("builds");
        let publish_dir = publish_dir.unwrap_or_else(|| var_dir.join("appstream"));
        let staging_dir = var_dir.join("staging").join("appstream");
        Self {
            repo_root,
            registry_dir,
            builds_dir,
            publish_dir,
            staging_dir,
        }
    }
}

pub fn discover_manifests(registry_dir: &Path) -> Result<Vec<ManifestPath>> {
    if !registry_dir.is_dir() {
        anyhow::bail!("registry directory not found at {}", registry_dir.display());
    }
    let mut manifests: Vec<ManifestPath> = WalkDir::new(registry_dir)
        .min_depth(2)
        .max_depth(2)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file() && e.file_name() == "manifest.json")
        .map(|e| ManifestPath::new(e.into_path()))
        .collect::<Result<_>>()?;
    manifests.sort_by(|a, b| a.path.cmp(&b.path));
    if manifests.is_empty() {
        anyhow::bail!(
            "no manifests found under {}/<componentId>/manifest.json",
            registry_dir.display()
        );
    }
    Ok(manifests)
}
