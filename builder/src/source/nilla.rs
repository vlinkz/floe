use anyhow::{Context, Result, anyhow};

use crate::build_json::Source;
use crate::manifest::NillaSource;
use crate::nix;

use super::{BuildOutput, Driver, nix_attr_path_list, nix_string};

pub struct NillaDriver<'a> {
    src: &'a NillaSource,
}

impl<'a> NillaDriver<'a> {
    pub fn new(src: &'a NillaSource) -> Self {
        Self { src }
    }

    fn header(&self) -> String {
        format!(
            r#"let
  src = builtins.fetchTarball {{
    url = {url};
    sha256 = {hash};
  }};
  project = import (src + ("/" + {project_file}));
  attrPath = path: builtins.foldl' (acc: name: acc.${{name}}) project path;
in
"#,
            url = nix_string(&self.src.url),
            hash = nix_string(&self.src.hash),
            project_file = nix_string(&self.src.project_file),
        )
    }
}

impl Driver for NillaDriver<'_> {
    fn resolve(&self) -> Result<Source> {
        let probe = format!("{}builtins.pathExists src", self.header());
        nix::eval_bool_expr(&probe)
            .ok_or_else(|| anyhow!("failed to fetch nilla source {url}", url = self.src.url))?;
        Ok(Source::Nilla {
            url: self.src.url.clone(),
            hash: self.src.hash.clone(),
            project_file: self.src.project_file.clone(),
        })
    }

    fn build(&self, _system: &str, attr: &str) -> Result<BuildOutput> {
        let header = self.header();
        let label = format!("{} ({})", self.src.url, attr);
        let path = nix_attr_path_list(attr);

        let store_path = nix::build_expr(&label, &format!("{header}attrPath {path}"))
            .with_context(|| format!("building {label}"))?;
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
