use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use flate2::Compression;
use flate2::GzBuilder;
use flate2::read::GzDecoder;
use tracing::{info, warn};

/// Catalog origin; cached icons live under `share/swcatalog/icons/<origin>/`.
pub const COMBINED_ORIGIN: &str = "floe";

/// Combined catalog filename for `<system>`.
pub fn catalog_filename(system: &str) -> String {
    format!("Components-{system}.xml.gz")
}

/// Combine per-app slices into one `share/swcatalog` tree (deterministic).
pub fn run(slices_dir: &Path, out_dir: &Path, system: &str) -> Result<AggregateReport> {
    let arch_slice = slices_dir.join(system);
    if !arch_slice.is_dir() {
        bail!(
            "no AppStream slice directory at {} for system {system}",
            arch_slice.display()
        );
    }

    let xml_dir = arch_slice.join("xmls");
    let icons_root = arch_slice.join("icons");

    let entries = read_xml_slices(&xml_dir)?;
    if entries.is_empty() {
        bail!(
            "no per-app catalog slices found at {}; refusing to publish an empty catalog",
            xml_dir.display()
        );
    }

    let swcatalog = out_dir.join("share").join("swcatalog");
    let xml_out_dir = swcatalog.join("xml");
    let icons_origin_dir = swcatalog.join("icons").join(COMBINED_ORIGIN);
    fs::create_dir_all(&xml_out_dir)
        .with_context(|| format!("creating {}", xml_out_dir.display()))?;

    let mut components_xml = String::new();
    let mut report = AggregateReport::default();
    let mut errors: Vec<String> = Vec::new();
    let mut media_baseurl: Option<String> = None;
    for entry in entries {
        match ingest_slice(&entry, &icons_root, &icons_origin_dir) {
            Ok(slice) => {
                if let Some(url) = slice.media_baseurl {
                    match &media_baseurl {
                        None => media_baseurl = Some(url),
                        Some(existing) if existing != &url => {
                            errors.push(format!(
                                "{}: media_baseurl mismatch (slice={url:?}, prior={existing:?})",
                                entry.path.display()
                            ));
                        }
                        _ => {}
                    }
                }
                components_xml.push_str(&slice.body);
                report.apps.push(slice.app_id);
                report.icons += slice.icons_copied;
            }
            Err(err) => {
                let msg = format!("{}: {err:#}", entry.path.display());
                warn!(slice = %entry.path.display(), error = %format!("{err:#}"), "slice ingestion failed");
                errors.push(msg);
            }
        }
    }
    if !errors.is_empty() {
        bail!(
            "{} slice(s) failed to ingest; refusing to publish a partial catalog:\n  - {}",
            errors.len(),
            errors.join("\n  - ")
        );
    }

    let catalog = compose_catalog(&components_xml, media_baseurl.as_deref());
    let catalog_path = xml_out_dir.join(catalog_filename(system));
    write_gzipped(&catalog_path, catalog.as_bytes())
        .with_context(|| format!("writing {}", catalog_path.display()))?;

    info!(
        catalog = %catalog_path.display(),
        apps = report.apps.len(),
        icons = report.icons,
        "wrote combined catalog"
    );
    Ok(report)
}

#[derive(Debug, Default)]
pub struct AggregateReport {
    pub apps: Vec<String>,
    pub icons: usize,
}

#[derive(Debug)]
struct SliceEntry {
    app_id: String,
    path: PathBuf,
    gzipped: bool,
}

fn read_xml_slices(xml_dir: &Path) -> Result<Vec<SliceEntry>> {
    if !xml_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(xml_dir).with_context(|| format!("reading {}", xml_dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = match path.file_name().and_then(|s| s.to_str()) {
            Some(n) => n,
            None => continue,
        };
        let (app_id, gzipped) = if let Some(stripped) = name.strip_suffix(".xml.gz") {
            (stripped.to_owned(), true)
        } else if let Some(stripped) = name.strip_suffix(".xml") {
            (stripped.to_owned(), false)
        } else {
            continue;
        };
        out.push(SliceEntry {
            app_id,
            path,
            gzipped,
        });
    }
    out.sort_by(|a, b| a.app_id.cmp(&b.app_id));
    Ok(out)
}

#[derive(Debug)]
struct IngestedSlice {
    app_id: String,
    body: String,
    icons_copied: usize,
    media_baseurl: Option<String>,
}

fn ingest_slice(
    entry: &SliceEntry,
    icons_root: &Path,
    icons_origin_dir: &Path,
) -> Result<IngestedSlice> {
    let raw = read_maybe_gzip(&entry.path, entry.gzipped)?;
    let xml = std::str::from_utf8(&raw)
        .with_context(|| format!("{} is not valid UTF-8", entry.path.display()))?;

    let media_baseurl = extract_components_attr(xml, "media_baseurl");
    let inner = extract_components_inner(xml)
        .with_context(|| format!("parsing {}", entry.path.display()))?;
    let component = extract_first_component(inner)
        .with_context(|| format!("{}: no <component> in slice", entry.path.display()))?;

    let icons = collect_cached_icons(component);
    let mut rewritten = component.to_owned();
    let icon_filename = format!("{}.png", entry.app_id);
    for icon in &icons {
        let new_tag = rebuild_icon_tag(&icon.full_tag, &icon_filename);
        rewritten = rewritten.replacen(&icon.full_tag, &new_tag, 1);
    }

    let mut icons_copied = 0;
    for icon in &icons {
        let copied = copy_icon(
            &entry.app_id,
            icon,
            icons_root,
            icons_origin_dir,
            &icon_filename,
        )
        .with_context(|| {
            format!(
                "copying cached {} icon for {} from {}",
                icon.size_dir,
                entry.app_id,
                icons_root.display()
            )
        })?;
        if copied {
            icons_copied += 1;
        }
    }

    let mut body = String::new();
    body.push_str("  ");
    body.push_str(rewritten.trim());
    body.push('\n');

    Ok(IngestedSlice {
        app_id: entry.app_id.clone(),
        body,
        icons_copied,
        media_baseurl,
    })
}

fn extract_components_attr(xml: &str, name: &str) -> Option<String> {
    let start = xml.find("<components")?;
    let close = xml[start..].find('>')? + start;
    let head = &xml[start..close];
    let needle = format!("{name}=\"");
    let attr_start = head.find(&needle)? + needle.len();
    let attr_end = head[attr_start..].find('"')? + attr_start;
    Some(head[attr_start..attr_end].to_owned())
}

fn read_maybe_gzip(path: &Path, gzipped: bool) -> Result<Vec<u8>> {
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    if !gzipped {
        return Ok(bytes);
    }
    let mut buf = Vec::new();
    GzDecoder::new(bytes.as_slice())
        .read_to_end(&mut buf)
        .with_context(|| format!("decompressing {}", path.display()))?;
    Ok(buf)
}

/// Inner body of the top-level `<components>` element.
fn extract_components_inner(xml: &str) -> Result<&str> {
    let start = xml
        .find("<components")
        .ok_or_else(|| anyhow!("no <components> tag found"))?;
    let header_close = xml[start..]
        .find('>')
        .ok_or_else(|| anyhow!("malformed <components> opening tag"))?
        + start
        + 1;
    let end = xml
        .rfind("</components>")
        .ok_or_else(|| anyhow!("missing </components>"))?;
    Ok(xml[header_close..end].trim())
}

fn extract_first_component(inner: &str) -> Option<&str> {
    let start = inner.find("<component")?;
    let end = inner[start..].find("</component>")? + start + "</component>".len();
    Some(&inner[start..end])
}

#[derive(Debug)]
struct CachedIcon {
    full_tag: String,
    /// Subdir used by compose, e.g. `64x64`, `128x128@2`.
    size_dir: String,
    /// Original cached filename inside the slice tree.
    src_name: String,
}

/// Collect cached `<icon>` elements from a single component.
fn collect_cached_icons(component: &str) -> Vec<CachedIcon> {
    let mut out = Vec::new();
    let mut cursor = 0;
    while let Some(start) = component[cursor..].find("<icon").map(|i| i + cursor) {
        let after = match component[start..].find('>') {
            Some(i) => start + i + 1,
            None => break,
        };
        let head = &component[start..after];
        let end = match component[after..].find("</icon>") {
            Some(i) => after + i,
            None => break,
        };
        let tag_end = end + "</icon>".len();
        let inner = component[after..end].trim();
        cursor = tag_end;

        if !head.contains("type=\"cached\"") {
            continue;
        }
        let width = attr(head, "width").and_then(|w| w.parse::<u32>().ok());
        let height = attr(head, "height").and_then(|h| h.parse::<u32>().ok());
        let scale = attr(head, "scale")
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(1);
        let size_dir = match (width, height) {
            (Some(w), Some(h)) if w == h => size_dir(w, scale),
            _ => continue,
        };
        out.push(CachedIcon {
            full_tag: component[start..tag_end].to_owned(),
            size_dir,
            src_name: inner.to_owned(),
        });
    }
    out
}

fn attr(head: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = head.find(&needle)? + needle.len();
    let end = head[start..].find('"')? + start;
    Some(head[start..end].to_owned())
}

fn size_dir(size: u32, scale: u32) -> String {
    if scale <= 1 {
        format!("{size}x{size}")
    } else {
        format!("{size}x{size}@{scale}")
    }
}

fn rebuild_icon_tag(original: &str, new_name: &str) -> String {
    let head_end = match original.find('>') {
        Some(i) => i + 1,
        None => return original.to_owned(),
    };
    let close_start = match original.rfind("</icon>") {
        Some(i) => i,
        None => return original.to_owned(),
    };
    let mut out = String::with_capacity(original.len());
    out.push_str(&original[..head_end]);
    out.push_str(new_name);
    out.push_str(&original[close_start..]);
    out
}

/// Copy a cached icon; symlinks/escapes are fatal, missing icons are skipped.
fn copy_icon(
    app_id: &str,
    icon: &CachedIcon,
    icons_root: &Path,
    icons_origin_dir: &Path,
    icon_filename: &str,
) -> Result<bool> {
    let safe_name = sanitize_filename(&icon.src_name)
        .ok_or_else(|| anyhow!("rejecting unsafe icon name {:?}", icon.src_name))?;
    let safe_size = sanitize_filename(&icon.size_dir)
        .ok_or_else(|| anyhow!("rejecting unsafe icon size dir {:?}", icon.size_dir))?;

    let size_dir = icons_root.join(safe_size);
    match fs::symlink_metadata(&size_dir) {
        Ok(meta) if meta.file_type().is_symlink() => bail!(
            "rejecting icon size dir {} via symlink for {app_id}",
            size_dir.display(),
        ),
        Ok(_) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => {
            return Err(err).with_context(|| format!("stat {} for {app_id}", size_dir.display()));
        }
    }

    let src = size_dir.join(safe_name);
    match fs::symlink_metadata(&src) {
        Ok(meta) if meta.file_type().is_symlink() => bail!(
            "rejecting icon source {} via symlink for {app_id}",
            src.display(),
        ),
        Ok(meta) if meta.file_type().is_file() => {}
        Ok(_) => return Ok(false),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => {
            return Err(err).with_context(|| format!("stat {} for {app_id}", src.display()));
        }
    }

    let dst_dir = icons_origin_dir.join(&icon.size_dir);
    fs::create_dir_all(&dst_dir).with_context(|| format!("creating {}", dst_dir.display()))?;
    let dst = dst_dir.join(icon_filename);
    fs::copy(&src, &dst).with_context(|| {
        format!(
            "copying {} -> {} for {app_id}",
            src.display(),
            dst.display()
        )
    })?;
    Ok(true)
}

/// Accept only a single non-empty filename component (no `/`, `..`, NUL).
fn sanitize_filename(raw: &str) -> Option<&str> {
    if raw.is_empty() || raw.contains('\0') {
        return None;
    }
    let path = Path::new(raw);
    let mut comps = path.components();
    let only = comps.next()?;
    if comps.next().is_some() {
        return None;
    }
    match only {
        Component::Normal(name) => name.to_str(),
        _ => None,
    }
}

/// Final catalog XML; no timestamp so identical inputs produce identical bytes.
fn compose_catalog(components_inner: &str, media_baseurl: Option<&str>) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(&format!(
        "<components version=\"1.0\" origin=\"{COMBINED_ORIGIN}\""
    ));
    if let Some(url) = media_baseurl {
        out.push_str(&format!(" media_baseurl=\"{}\"", xml_escape(url)));
    }
    out.push_str(">\n");
    let trimmed = components_inner.trim_end();
    if !trimmed.is_empty() {
        out.push_str(trimmed);
        out.push('\n');
    }
    out.push_str("</components>\n");
    out
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Gzip-compress with `mtime=0` so output is bit-stable.
fn write_gzipped(path: &Path, payload: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let file = fs::File::create(path).with_context(|| format!("creating {}", path.display()))?;
    let mut encoder = GzBuilder::new().mtime(0).write(file, Compression::best());
    encoder
        .write_all(payload)
        .with_context(|| format!("compressing {}", path.display()))?;
    encoder
        .finish()
        .with_context(|| format!("finalizing {}", path.display()))?;
    Ok(())
}

/// Per-system summary as JSON for greppable CI logs.
pub fn report_to_json(system: &str, report: &AggregateReport) -> Result<String> {
    let mut map: BTreeMap<&str, serde_json::Value> = BTreeMap::new();
    map.insert("system", system.into());
    map.insert("apps", report.apps.clone().into());
    map.insert("icons", report.icons.into());
    Ok(serde_json::to_string_pretty(&map)?)
}
