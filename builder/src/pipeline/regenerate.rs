use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use tracing::{info, info_span, warn};

use crate::appstream;
use crate::build_json::{AppDir, AppMetadata, SystemRecord};
use crate::manifest::ManifestPath;

use super::{Ctx, discover_manifests};

/// Regenerate AppStream slices for one or all `(app, system)` pairs from
/// already-committed build records.
pub fn run_regenerate(ctx: &Ctx, app: Option<&str>, system: Option<&str>) -> Result<()> {
    let manifests = discover_manifests(&ctx.registry_dir)?;
    let mut total = 0u32;
    let mut failed: Vec<(String, String)> = Vec::new();
    for manifest_path in &manifests {
        if let Some(filter) = app
            && manifest_path.component_id != filter
        {
            continue;
        }
        let dir = AppDir::new(&ctx.builds_dir, &manifest_path.component_id);
        for entry in walk_records(&dir)? {
            if let Some(filter) = system
                && entry.system != filter
            {
                continue;
            }
            total += 1;
            if let Err(err) = regenerate_one(ctx, manifest_path, &entry.record) {
                warn!(
                    app = %entry.record.app_id,
                    system = %entry.record.system,
                    error = %format!("{err:#}"),
                    "regenerate failed; continuing"
                );
                failed.push((entry.record.app_id.clone(), entry.record.system.clone()));
            }
        }
    }
    if total == 0 {
        bail!(
            "no build records matched (app={:?}, system={:?})",
            app,
            system
        );
    }
    let ok = total - failed.len() as u32;
    info!(succeeded = ok, failed = failed.len(), "regenerate done");
    if failed.is_empty() {
        Ok(())
    } else {
        let summary = failed
            .iter()
            .map(|(a, s)| format!("{a}/{s}"))
            .collect::<Vec<_>>()
            .join(", ");
        Err(anyhow!("{} pair(s) failed: {summary}", failed.len()))
    }
}

struct WalkedRecord {
    system: String,
    record: SystemRecord,
}

fn walk_records(dir: &AppDir<'_>) -> Result<Vec<WalkedRecord>> {
    let path = dir.dir();
    if !path.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&path).with_context(|| format!("reading {}", path.display()))? {
        let entry = entry?;
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        let name = match p.file_name().and_then(|s| s.to_str()) {
            Some(n) => n,
            None => continue,
        };
        let system = match name.strip_suffix(".json") {
            Some(s) => s.to_owned(),
            None => continue,
        };
        let raw =
            std::fs::read_to_string(&p).with_context(|| format!("reading {}", p.display()))?;
        let record: SystemRecord =
            serde_json::from_str(&raw).with_context(|| format!("parsing {}", p.display()))?;
        out.push(WalkedRecord { system, record });
    }
    out.sort_by(|a, b| a.system.cmp(&b.system));
    Ok(out)
}

fn regenerate_one(ctx: &Ctx, manifest_path: &ManifestPath, record: &SystemRecord) -> Result<()> {
    let _span = info_span!("regenerate", app = %record.app_id, system = %record.system).entered();
    info!(store = %record.store_path, "ensuring closure is realised");
    realise_closure(&record.store_path)?;

    let metadata = recompose_appstream(ctx, manifest_path, &record.store_path, &record.system)?;
    if metadata != record.metadata {
        warn!("regenerated metadata differs from committed record (record stays as-is)");
    }
    info!("slice ready");
    Ok(())
}

fn recompose_appstream(
    ctx: &Ctx,
    manifest_path: &ManifestPath,
    store_path: &str,
    system: &str,
) -> Result<AppMetadata> {
    let app_id = &manifest_path.component_id;
    let closure = PathBuf::from(store_path);

    let metainfo = appstream::resolve(app_id, &closure, &manifest_path.component_dir)?;
    let arch_staging = ctx.staging_dir.join(system);
    let arch_publish = ctx.publish_dir.join(system);
    let app_stage = arch_staging.join(app_id);
    if app_stage.exists() {
        std::fs::remove_dir_all(&app_stage)
            .with_context(|| format!("clearing {}", app_stage.display()))?;
    }
    std::fs::create_dir_all(&arch_publish)
        .with_context(|| format!("creating {}", arch_publish.display()))?;

    let staged = appstream::stage_app(app_id, &closure, &metainfo, &arch_staging)?;
    appstream::compose(app_id, &staged, &arch_publish)?;

    Ok(AppMetadata {
        component_type: metainfo.extracted.kind,
        summary: metainfo.extracted.summary.clone(),
        long_description: metainfo.extracted.description_text.clone(),
        homepage: metainfo.extracted.homepage.clone(),
        license: metainfo.extracted.project_license.clone(),
    })
}

fn realise_closure(store_path: &str) -> Result<()> {
    let status = Command::new("nix-store")
        .arg("--realise")
        .arg(store_path)
        .status()
        .context("spawning `nix-store --realise`")?;
    if !status.success() {
        bail!("nix-store --realise failed for {store_path}: {status}");
    }
    Ok(())
}
