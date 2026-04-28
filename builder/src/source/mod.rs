use anyhow::Result;

use crate::build_json::Source;
use crate::manifest::{Manifest, ManifestPath, SourceKind};

pub mod flake;
pub mod legacy;
pub mod nilla;

#[derive(Debug, Clone)]
pub struct BuildOutput {
    pub attr: String,
    pub store_path: String,
    pub unfree: bool,
    pub version: Option<String>,
}

pub trait Driver {
    fn resolve(&self) -> Result<Source>;
    fn build(&self, system: &str, attr: &str) -> Result<BuildOutput>;
}

pub fn make_driver<'a>(
    manifest: &'a Manifest,
    manifest_path: &'a ManifestPath,
) -> Box<dyn Driver + 'a> {
    match manifest.kind() {
        SourceKind::Flake => Box::new(flake::FlakeDriver::new(manifest.flake.as_ref().unwrap())),
        SourceKind::Nilla => Box::new(nilla::NillaDriver::new(manifest.nilla.as_ref().unwrap())),
        SourceKind::Legacy => Box::new(legacy::LegacyDriver::new(
            manifest.legacy.as_ref().unwrap(),
            manifest_path,
        )),
    }
}

/// Rust `&str` -> Nix string literal.
pub fn nix_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str(r#"\""#),
            '\\' => out.push_str(r"\\"),
            '\n' => out.push_str(r"\n"),
            '\r' => out.push_str(r"\r"),
            '\t' => out.push_str(r"\t"),
            '$' => out.push_str(r"\$"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `a.b.c` -> `[ "a" "b" "c" ]`.
pub fn nix_attr_path_list(attr: &str) -> String {
    let parts: Vec<String> = attr.split('.').map(nix_string).collect();
    format!("[ {} ]", parts.join(" "))
}
