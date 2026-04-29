use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;

use crate::nix;
use crate::source::{BuildOutput, nix_string};

pub mod trim;

pub use trim::Trim;

/// nixpkgs pin for wrapper derivations, loaded from `npins/sources.json`.
#[derive(Debug, Clone)]
pub struct NixpkgsPin {
    pub url: String,
    pub hash: String,
}

impl NixpkgsPin {
    pub fn load(repo_root: &Path) -> Result<Self> {
        #[derive(Deserialize)]
        struct Sources {
            pins: std::collections::HashMap<String, Pin>,
        }
        #[derive(Deserialize)]
        struct Pin {
            url: String,
            hash: String,
        }

        let path = repo_root.join("npins/sources.json");
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let sources: Sources =
            serde_json::from_str(&raw).with_context(|| format!("parsing {}", path.display()))?;
        let pin = sources
            .pins
            .get("nixos-unstable")
            .ok_or_else(|| anyhow!("npins/sources.json missing 'nixos-unstable' pin"))?;
        Ok(Self {
            url: pin.url.clone(),
            hash: pin.hash.clone(),
        })
    }
}

pub struct WrapperContext<'a> {
    pub upstream: &'a BuildOutput,
    pub main_program: &'a str,
    pub system: &'a str,
    pub direct_refs: Vec<String>,
    pub nixpkgs: &'a NixpkgsPin,
}

impl<'a> WrapperContext<'a> {
    pub fn new(
        upstream: &'a BuildOutput,
        main_program: &'a str,
        system: &'a str,
        nixpkgs: &'a NixpkgsPin,
    ) -> Result<Self> {
        let direct_refs = direct_references(&upstream.store_path)?;
        Ok(Self {
            upstream,
            main_program,
            system,
            direct_refs,
            nixpkgs,
        })
    }
}

pub trait Wrapper {
    const NAME: &'static str;
    const NIX: &'static str;
    type Config;
    fn build_args(&self, cfg: &Self::Config) -> Vec<(&'static str, String)>;
}

pub struct WrapperResult {
    pub store_path: String,
    pub ops: Vec<String>,
}

pub fn apply<W: Wrapper>(
    wrapper: &W,
    ctx: &WrapperContext<'_>,
    cfg: &W::Config,
) -> Result<WrapperResult> {
    if !ctx.upstream.store_path.starts_with("/nix/store/") {
        return Err(anyhow!(
            "upstream store path is not under /nix/store: {}",
            ctx.upstream.store_path
        ));
    }

    let extra_args = wrapper.build_args(cfg);
    let expr = invocation::<W>(ctx, &extra_args);

    let label = format!("{}-{}", ctx.main_program, W::NAME);
    let store_path = nix::build_expr(&label, &expr)
        .with_context(|| format!("building {} wrapper derivation", W::NAME))?;
    let ops = eval_ops(&expr)?;

    Ok(WrapperResult { store_path, ops })
}

fn direct_references(store_path: &str) -> Result<Vec<String>> {
    let output = Command::new("nix-store")
        .args(["--query", "--references", store_path])
        .stderr(Stdio::inherit())
        .output()
        .with_context(|| format!("nix-store --query --references {store_path}"))?;
    if !output.status.success() {
        bail!("`nix-store --query --references {store_path}` failed");
    }
    let s = std::str::from_utf8(&output.stdout)
        .context("`nix-store --query --references` produced non-UTF-8 output")?;
    Ok(s.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect())
}

fn invocation<W: Wrapper>(ctx: &WrapperContext<'_>, extra_args: &[(&str, String)]) -> String {
    let refs_list = ctx
        .direct_refs
        .iter()
        .map(|p| nix_string(p))
        .collect::<Vec<_>>()
        .join(" ");

    let mut args = format!(
        "nixpkgsUrl = {url};
        nixpkgsHash = {hash};
        system = {system};
        upstreamPath = {upstream};
        directRefs = [ {refs_list} ];
        mainProgram = {main};",
        url = nix_string(&ctx.nixpkgs.url),
        hash = nix_string(&ctx.nixpkgs.hash),
        system = nix_string(ctx.system),
        upstream = nix_string(&ctx.upstream.store_path),
        main = nix_string(ctx.main_program),
    );
    for (key, value) in extra_args {
        args.push_str(&format!("\n        {key} = {value};"));
    }

    format!("({nix}) {{ {args} }}", nix = W::NIX, args = args)
}

fn eval_ops(expr: &str) -> Result<Vec<String>> {
    let ops_expr = format!("({expr}).passthru.ops");
    let output = Command::new("nix")
        .args(["eval", "--json", "--expr", &ops_expr])
        .stderr(Stdio::inherit())
        .output()
        .context("spawning `nix eval` for ops list")?;
    if !output.status.success() {
        bail!("`nix eval` for ops list failed: {}", output.status);
    }
    serde_json::from_slice(&output.stdout).context("parsing ops list as JSON")
}
