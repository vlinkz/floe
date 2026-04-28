use anyhow::{Context, Result, bail};

use crate::build_json::Source;
use crate::manifest::{LegacySource, ManifestPath};
use crate::nix;

use super::{BuildOutput, Driver, nix_attr_path_list, nix_string};

pub struct LegacyDriver<'a> {
    src: &'a LegacySource,
    label: String,
}

impl<'a> LegacyDriver<'a> {
    pub fn new(src: &'a LegacySource, manifest_path: &'a ManifestPath) -> Self {
        Self {
            src,
            label: manifest_path.component_id.clone(),
        }
    }

    fn header(&self) -> String {
        format!(
            r#"let
  src = builtins.fetchTarball {{
    url = {url};
    sha256 = {hash};
  }};
  project = import (src + ("/" + {entry})) {{}};
  attrPath = path: builtins.foldl' (acc: name: acc.${{name}}) project path;
in
"#,
            url = nix_string(&self.src.url),
            hash = nix_string(&self.src.hash),
            entry = nix_string(&self.src.entry),
        )
    }
}

impl Driver for LegacyDriver<'_> {
    fn resolve(&self) -> Result<Source> {
        Ok(Source::Legacy {
            url: self.src.url.clone(),
            hash: self.src.hash.clone(),
            entry: self.src.entry.clone(),
        })
    }

    fn build(&self, _system: &str, attr: &str) -> Result<BuildOutput> {
        if attr.is_empty() || attr.split('.').any(str::is_empty) {
            bail!(
                "{}: legacy attr {:?} must be a non-empty dotted attr path",
                self.label,
                attr
            );
        }
        let header = self.header();
        let path = nix_attr_path_list(attr);

        let store_path = nix::build_expr(&self.label, &format!("{header}attrPath {path}"))
            .with_context(|| format!("building legacy {} ({attr})", self.label))?;
        let body = nix::PKG_METADATA_BODY;
        let pkg: nix::PkgMetadata = nix::eval_json(&[
            "eval",
            "--json",
            "--expr",
            &format!("{header}let p = attrPath {path}; in {body}"),
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
