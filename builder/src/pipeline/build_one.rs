use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use tracing::{info, info_span, warn};

use crate::appstream;
use crate::build_json::{AppDir, AppMetadata, BUILD_JSON_SCHEMA_VERSION, SystemRecord};
use crate::manifest::{Manifest, ManifestPath};
use crate::nix;
use crate::source;

use super::{Ctx, discover_manifests};

/// Build every `(app, system)` pair in the registry.
pub fn run_build_all(ctx: &Ctx, only_system: Option<&str>) -> Result<()> {
    let manifests = discover_manifests(&ctx.registry_dir)?;
    let mut total = 0u32;
    let mut failed: Vec<(String, String)> = Vec::new();
    for manifest_path in &manifests {
        let manifest = Manifest::load(&manifest_path.path)?;
        for system in &manifest.systems {
            if let Some(filter) = only_system
                && system != filter
            {
                continue;
            }
            total += 1;
            if let Err(err) = run_build(ctx, &manifest_path.component_id, system) {
                warn!(
                    app = %manifest_path.component_id,
                    system = %system,
                    error = %format!("{err:#}"),
                    "build failed; continuing"
                );
                failed.push((manifest_path.component_id.clone(), system.clone()));
            }
        }
    }
    if total == 0 {
        match only_system {
            Some(s) => bail!("no manifests declare system {s:?}"),
            None => bail!("no manifests to build"),
        }
    }
    let ok = total - failed.len() as u32;
    info!(succeeded = ok, failed = failed.len(), "build --all done");
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

/// Build one `(app, system)` pair.
pub fn run_build(ctx: &Ctx, app_id: &str, system: &str) -> Result<()> {
    let manifest_path = find_manifest(ctx, app_id)?;
    let manifest = Manifest::load(&manifest_path.path)?;
    if !manifest.systems.iter().any(|s| s == system) {
        bail!(
            "{}: system {:?} is not declared in manifest (declared: {})",
            app_id,
            system,
            manifest.systems.join(", ")
        );
    }

    let _span = info_span!("build", app = %app_id, system = %system).entered();
    info!(
        kind = manifest.kind().as_str(),
        attr = %manifest.attr,
        "building"
    );

    let driver = source::make_driver(&manifest, &manifest_path);
    let resolved = driver
        .resolve()
        .with_context(|| format!("resolving source for {app_id}"))?;
    info!("resolved source");

    let attr = manifest.attr_for(system);
    let output = driver.build(system, &attr)?;
    info!(store = %output.store_path, "built");

    let bin_path = Path::new(&output.store_path)
        .join("bin")
        .join(&manifest.main_program);
    if !bin_path.exists() {
        warn!(
            main_program = %manifest.main_program,
            path = %bin_path.display(),
            "main program not found in closure (continuing; fix manifest if unintended)"
        );
    }

    let info = nix::path_info(&output.store_path)?;
    let version = manifest
        .version
        .clone()
        .or_else(|| output.version.clone())
        .unwrap_or_else(|| "unknown".to_owned());

    let metadata = run_appstream(ctx, &manifest_path, &output.store_path, system)?;

    let shard = SystemRecord {
        schema_version: BUILD_JSON_SCHEMA_VERSION,
        app_id: app_id.to_owned(),
        main_program: manifest.main_program.clone(),
        system: system.to_owned(),
        source: resolved,
        metadata,
        attr: output.attr,
        version,
        store_path: output.store_path,
        nar_hash: info.nar_hash,
        closure_size: info.closure_size,
        unfree: output.unfree,
        generated: now_iso8601(),
    };
    shard.write_to(&ctx.builds_dir)?;
    info!(
        path = %AppDir::new(&ctx.builds_dir, app_id).system_json(system).display(),
        "wrote system shard"
    );
    Ok(())
}

fn run_appstream(
    ctx: &Ctx,
    manifest_path: &ManifestPath,
    store_path: &str,
    system: &str,
) -> Result<AppMetadata> {
    let app_id = &manifest_path.component_id;
    let closure = PathBuf::from(store_path);

    let metainfo = appstream::resolve(app_id, &closure, &manifest_path.component_dir)?;
    info!(
        source = ?metainfo.source,
        path = %metainfo.path.display(),
        "resolved metainfo"
    );

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

fn find_manifest(ctx: &Ctx, app_id: &str) -> Result<ManifestPath> {
    discover_manifests(&ctx.registry_dir)?
        .into_iter()
        .find(|m| m.component_id == app_id)
        .ok_or_else(|| anyhow::anyhow!("no registry entry for app id {app_id:?}"))
}

fn now_iso8601() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}
