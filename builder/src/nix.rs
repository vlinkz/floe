use std::process::{Command, Stdio};

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct FlakeMetadata {
    pub url: String,
    pub rev: String,
}

pub fn flake_metadata(flake_url: &str) -> Result<FlakeMetadata> {
    let stdout = run_capture(
        &["flake", "metadata", "--json", flake_url],
        &format!("nix flake metadata {flake_url}"),
    )?;

    #[derive(Deserialize)]
    struct Raw {
        url: String,
        locked: Option<RawLocked>,
    }
    #[derive(Deserialize)]
    struct RawLocked {
        rev: Option<String>,
        #[serde(rename = "narHash")]
        nar_hash: Option<String>,
    }

    let raw: Raw = serde_json::from_slice(&stdout)
        .with_context(|| format!("parsing `nix flake metadata --json {flake_url}` output"))?;
    let rev = raw
        .locked
        .and_then(|l| l.rev.or(l.nar_hash))
        .unwrap_or_default();
    Ok(FlakeMetadata { url: raw.url, rev })
}

pub fn build(installable: &str) -> Result<String> {
    build_args(
        &["build", "--no-link", "--print-out-paths", installable],
        installable,
    )
}

pub fn build_expr(label: &str, expr: &str) -> Result<String> {
    build_args(
        &["build", "--no-link", "--print-out-paths", "--expr", expr],
        label,
    )
}

fn build_args(args: &[&str], label: &str) -> Result<String> {
    let stdout = run_capture(args, &format!("nix build for {label}"))?;
    let s = std::str::from_utf8(&stdout).context("`nix build` stdout was not UTF-8")?;
    s.lines()
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("`nix build` for {label} produced no out path"))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathInfo {
    pub nar_hash: String,
    pub closure_size: u64,
}

pub fn path_info(store_path: &str) -> Result<PathInfo> {
    let stdout = run_capture(
        &["path-info", "--json", "--closure-size", store_path],
        &format!("nix path-info {store_path}"),
    )?;
    let value: serde_json::Value = serde_json::from_slice(&stdout)
        .with_context(|| format!("parsing `nix path-info --json {store_path}` output"))?;

    let entry = match value {
        serde_json::Value::Array(mut arr) => arr.drain(..).next(),
        serde_json::Value::Object(map) => map.into_iter().next().map(|(_, v)| v),
        _ => None,
    }
    .ok_or_else(|| anyhow!("`nix path-info` returned an unexpected JSON shape"))?;
    serde_json::from_value(entry).context("decoding `nix path-info` entry")
}

pub fn eval_bool_expr(expr: &str) -> Option<bool> {
    eval_json(&["eval", "--json", "--expr", expr])
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PkgMetadata {
    #[serde(default)]
    pub unfree: bool,
    #[serde(default)]
    pub version: Option<String>,
}

pub const PKG_METADATA_FN: &str = "p: { \
    unfree = p.meta.unfree or false; \
    version = let v = p.version or (builtins.parseDrvName (p.name or \"\")).version; \
              in if v == \"\" then null else v; \
}";

pub const PKG_METADATA_BODY: &str = "{ \
    unfree = p.meta.unfree or false; \
    version = let v = p.version or (builtins.parseDrvName (p.name or \"\")).version; \
              in if v == \"\" then null else v; \
}";

pub fn eval_json<T: for<'de> Deserialize<'de>>(args: &[&str]) -> Option<T> {
    let output = Command::new("nix")
        .args(args)
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    serde_json::from_slice(&output.stdout).ok()
}

/// Run `nix <args>` and return captured stdout on success.
fn run_capture(args: &[&str], label: &str) -> Result<Vec<u8>> {
    let output = Command::new("nix")
        .args(args)
        .stderr(Stdio::inherit())
        .output()
        .with_context(|| format!("spawning `{label}`"))?;
    if !output.status.success() {
        bail!("`{label}` failed: {}", output.status);
    }
    Ok(output.stdout)
}
