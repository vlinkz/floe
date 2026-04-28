use anyhow::Result;
use serde::Serialize;

use crate::manifest::Manifest;

use super::{Ctx, discover_manifests};

#[derive(Debug, Serialize)]
pub struct Job {
    pub app: String,
    pub system: String,
}

pub fn run_list(ctx: &Ctx) -> Result<()> {
    let manifests = discover_manifests(&ctx.registry_dir)?;
    let mut jobs = Vec::new();
    for manifest_path in &manifests {
        let manifest = Manifest::load(&manifest_path.path)?;
        for system in &manifest.systems {
            jobs.push(Job {
                app: manifest_path.component_id.clone(),
                system: system.clone(),
            });
        }
    }
    println!("{}", serde_json::to_string(&jobs)?);
    Ok(())
}
