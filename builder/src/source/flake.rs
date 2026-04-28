use anyhow::{Context, Result};

use crate::build_json::Source;
use crate::manifest::FlakeSource;
use crate::nix;

use super::{BuildOutput, Driver};

pub struct FlakeDriver<'a> {
    src: &'a FlakeSource,
}

impl<'a> FlakeDriver<'a> {
    pub fn new(src: &'a FlakeSource) -> Self {
        Self { src }
    }

    fn flake_url(&self) -> String {
        format!("{}/{}", self.src.url, self.src.rev)
    }
}

impl Driver for FlakeDriver<'_> {
    fn resolve(&self) -> Result<Source> {
        let url = self.flake_url();
        let meta =
            nix::flake_metadata(&url).with_context(|| format!("resolving metadata for {url}"))?;
        Ok(Source::Flake {
            url: self.src.url.clone(),
            locked_url: meta.url,
            locked_rev: meta.rev,
        })
    }

    fn build(&self, _system: &str, attr: &str) -> Result<BuildOutput> {
        let installable = format!("{}#{}", self.flake_url(), attr);
        let store_path =
            nix::build(&installable).with_context(|| format!("building {installable}"))?;
        let pkg: nix::PkgMetadata = nix::eval_json(&[
            "eval",
            "--json",
            "--accept-flake-config",
            &installable,
            "--apply",
            nix::PKG_METADATA_FN,
        ])
        .unwrap_or_default();
        Ok(BuildOutput {
            attr: attr.to_owned(),
            store_path,
            unfree: pkg.unfree,
            version: pkg.version,
        })
    }
}
