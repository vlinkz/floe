use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use libappstream::prelude::*;
use libappstream::{ComponentKind, FormatKind, FormatStyle, Metadata, UrlKind};
use tracing::info;

#[derive(Debug)]
pub struct AppMetainfo {
    pub source: MetainfoSource,
    pub path: PathBuf,
    pub extracted: Extracted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetainfoSource {
    Closure,
    Repo,
}

#[derive(Debug, Clone, Default)]
pub struct Extracted {
    pub kind: Option<ComponentKind>,
    pub summary: Option<String>,
    pub description_text: Option<String>,
    pub homepage: Option<String>,
    pub project_license: Option<String>,
}

pub fn resolve(app_id: &str, closure: &Path, component_dir: &Path) -> Result<AppMetainfo> {
    let closure_path = closure
        .join("share")
        .join("metainfo")
        .join(format!("{app_id}.metainfo.xml"));
    let repo_path = component_dir.join("metainfo.xml");

    let closure_present = closure_path.exists();
    let repo_present = repo_path.exists();

    let (source, path) = match (closure_present, repo_present) {
        (false, false) => bail!(
            "{}: no metainfo found. Expected either {} (shipped by upstream) or {} (hand-written)",
            app_id,
            closure_path.display(),
            repo_path.display(),
        ),
        (true, true) => bail!(
            "{}: both upstream metainfo ({}) and repo metainfo ({}) are present; pick one source",
            app_id,
            closure_path.display(),
            repo_path.display(),
        ),
        (true, false) => (MetainfoSource::Closure, closure_path),
        (false, true) => (MetainfoSource::Repo, repo_path),
    };

    let extracted = parse_with_libappstream(&path)?;

    let actual_id = extracted.id_for_validation.as_deref().unwrap_or_default();
    if actual_id != app_id {
        bail!(
            "{}: <id> in {} is '{}', expected '{}'",
            app_id,
            path.display(),
            actual_id,
            app_id,
        );
    }

    validate_strict(&path).with_context(|| {
        format!(
            "validating {} metainfo {}",
            source_label(source),
            path.display()
        )
    })?;

    Ok(AppMetainfo {
        source,
        path,
        extracted: extracted.public,
    })
}

fn source_label(s: MetainfoSource) -> &'static str {
    match s {
        MetainfoSource::Closure => "closure",
        MetainfoSource::Repo => "repo",
    }
}

struct Parsed {
    id_for_validation: Option<String>,
    public: Extracted,
}

fn parse_with_libappstream(path: &Path) -> Result<Parsed> {
    let metadata = Metadata::new();
    metadata.set_format_style(FormatStyle::Metainfo);
    let file = gio::File::for_path(path);
    metadata
        .parse_file(&file, FormatKind::Xml)
        .with_context(|| format!("parsing {} via libappstream", path.display()))?;

    let component = metadata.component().ok_or_else(|| {
        anyhow::anyhow!(
            "{}: libappstream parsed the file but produced no component",
            path.display()
        )
    })?;

    let id = component.id().map(|s| s.to_string());
    let kind = Some(component.kind());
    let summary = component.summary().map(|s| s.to_string());
    let description_text = component.description().map(|s| flatten_description(&s));
    let homepage = component.url(UrlKind::Homepage).map(|s| s.to_string());
    let project_license = component.project_license().map(|s| s.to_string());

    Ok(Parsed {
        id_for_validation: id,
        public: Extracted {
            kind,
            summary,
            description_text,
            homepage,
            project_license,
        },
    })
}

/// appstream description -> nix meta.longDescription
fn flatten_description(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    let mut prev_was_space = true;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                if !prev_was_space && !out.is_empty() {
                    out.push(' ');
                    prev_was_space = true;
                }
            }
            _ if in_tag => {}
            c if c.is_whitespace() => {
                if !prev_was_space && !out.is_empty() {
                    out.push(' ');
                    prev_was_space = true;
                }
            }
            c => {
                out.push(c);
                prev_was_space = false;
            }
        }
    }
    out.trim().to_owned()
}

fn appstreamcli_path() -> String {
    std::env::var("FLOE_APPSTREAMCLI").unwrap_or_else(|_| "appstreamcli".to_owned())
}

fn validate_strict(path: &Path) -> Result<()> {
    let output = Command::new(appstreamcli_path())
        .args(["validate", "--strict", "--no-net"])
        .arg(path)
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .output()
        .context("spawning `appstreamcli validate`")?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "appstreamcli validate failed for {}:\n--- stdout ---\n{}\n--- stderr ---\n{}",
            path.display(),
            stdout,
            stderr,
        );
    }
    Ok(())
}

pub fn stage_app(
    app_id: &str,
    closure: &Path,
    metainfo: &AppMetainfo,
    staging_root: &Path,
) -> Result<PathBuf> {
    let app_root = staging_root.join(app_id);
    let metainfo_dir = app_root.join("share").join("metainfo");
    std::fs::create_dir_all(&metainfo_dir)
        .with_context(|| format!("creating {}", metainfo_dir.display()))?;
    let dest = metainfo_dir.join(format!("{app_id}.metainfo.xml"));
    std::fs::copy(&metainfo.path, &dest)
        .with_context(|| format!("copying metainfo to {}", dest.display()))?;

    for subdir in ["applications", "icons"] {
        let src = closure.join("share").join(subdir);
        if src.exists() {
            let dst = app_root.join("share").join(subdir);
            symlink_tree(&src, &dst).with_context(|| format!("staging {}", src.display()))?;
        }
    }

    Ok(app_root)
}

fn symlink_tree(src: &Path, dst: &Path) -> Result<()> {
    if src.is_file() {
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if dst.exists() {
            std::fs::remove_file(dst)?;
        }
        std::os::unix::fs::symlink(src, dst)?;
        return Ok(());
    }
    if src.is_dir() {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            symlink_tree(&entry.path(), &dst.join(entry.file_name()))?;
        }
    }
    Ok(())
}

pub fn compose(
    app_id: &str,
    system: &str,
    staged_app_root: &Path,
    publish_dir: &Path,
) -> Result<()> {
    info!("composing AppStream catalog");

    let data_dir = publish_dir.join("xmls");
    let icons_dir = publish_dir.join("icons");
    for d in [&data_dir, &icons_dir] {
        std::fs::create_dir_all(d).with_context(|| format!("creating {}", d.display()))?;
    }

    let mut cmd = Command::new(appstreamcli_path());
    cmd.arg("compose")
        .arg("--prefix=/")
        .arg(format!("--origin={app_id}"))
        .arg(format!("--result-root={}", publish_dir.display()))
        .arg(format!("--data-dir={}", data_dir.display()))
        .arg(format!("--icons-dir={}", icons_dir.display()))
        .arg(format!("--components={app_id}"));

    if let Ok(base) = std::env::var("FLOE_APPSTREAM_BASE_URL") {
        let media_dir = publish_dir.join("media");
        std::fs::create_dir_all(&media_dir)
            .with_context(|| format!("creating {}", media_dir.display()))?;
        let base = base.trim_end_matches('/');
        cmd.arg(format!("--media-dir={}", media_dir.display()))
            .arg(format!("--media-baseurl={base}/{system}/media"));
    }

    let status = cmd
        .arg(staged_app_root)
        .stderr(Stdio::inherit())
        .stdout(Stdio::inherit())
        .status()
        .context("spawning `appstreamcli compose`")?;

    if !status.success() {
        bail!("`appstreamcli compose` failed for {app_id}: {status}");
    }
    Ok(())
}
