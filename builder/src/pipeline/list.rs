use anyhow::Result;
use serde::Serialize;

use crate::build_json::{AppDir, Source, SystemRecord};
use crate::manifest::{Manifest, SourceKind};

use super::{Ctx, discover_manifests};

#[derive(Debug, Serialize)]
pub struct Job {
    pub app: String,
    pub system: String,
}

pub fn run_list(ctx: &Ctx, only_outdated: bool) -> Result<()> {
    let manifests = discover_manifests(&ctx.registry_dir)?;
    let mut jobs = Vec::new();
    for manifest_path in &manifests {
        let manifest = Manifest::load(&manifest_path.path)?;
        for system in &manifest.systems {
            if only_outdated && !needs_build(ctx, &manifest_path.component_id, system, &manifest) {
                continue;
            }
            jobs.push(Job {
                app: manifest_path.component_id.clone(),
                system: system.clone(),
            });
        }
    }
    println!("{}", serde_json::to_string(&jobs)?);
    Ok(())
}

fn needs_build(ctx: &Ctx, app_id: &str, system: &str, manifest: &Manifest) -> bool {
    let record_path = AppDir::new(&ctx.builds_dir, app_id).system_json(system);
    let raw = match std::fs::read_to_string(&record_path) {
        Ok(s) => s,
        Err(_) => return true,
    };
    let record: SystemRecord = match serde_json::from_str(&raw) {
        Ok(r) => r,
        Err(_) => return true,
    };

    if record.attr != manifest.attr_for(system) {
        return true;
    }
    if record.main_program != manifest.main_program {
        return true;
    }
    if let Some(v) = manifest.version.as_deref()
        && record.version != v
    {
        return true;
    }
    if !source_matches(&record.source, manifest) {
        return true;
    }
    manifest.wrappers.is_empty() == record.upstream.is_some()
}

fn source_matches(record: &Source, manifest: &Manifest) -> bool {
    match (manifest.kind(), record) {
        (SourceKind::Flake, Source::Flake { locked_rev, .. }) => {
            manifest.flake.as_ref().map(|f| &f.rev) == Some(locked_rev)
        }
        (SourceKind::Nilla, Source::Nilla { url, hash, .. }) => manifest
            .nilla
            .as_ref()
            .map(|n| (&n.url, &n.hash) == (url, hash))
            .unwrap_or(false),
        (SourceKind::Legacy, Source::Legacy { url, hash, .. }) => manifest
            .legacy
            .as_ref()
            .map(|l| (&l.url, &l.hash) == (url, hash))
            .unwrap_or(false),
        _ => false,
    }
}
