use std::path::PathBuf;

use anyhow::{Result, bail};
use tracing::info;

use crate::aggregate;

/// Aggregate per-app AppStream slices for one system into `out_dir`.
pub fn run_aggregate(slices_dir: PathBuf, out_dir: PathBuf, system: &str) -> Result<()> {
    if !slices_dir.is_dir() {
        bail!("slices dir {} does not exist", slices_dir.display());
    }
    let report = aggregate::run(&slices_dir, &out_dir, system)?;
    info!(
        system = %system,
        apps = report.apps.len(),
        icons = report.icons,
        "aggregate done"
    );
    println!("{}", aggregate::report_to_json(system, &report)?);
    Ok(())
}
