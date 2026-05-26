//! Release management — tag and track gerber fabrication releases.
//!
//! A release is a point-in-time snapshot of everything needed to send the
//! PCB out for fabrication: gerbers, drill files, and an optional
//! RELEASE_NOTES.md, bundled into a single `.zip` under
//! `<project_dir>/outputs/<rev_name>/`.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

use crate::event_logger::ReactiveEventLogger;
use crate::vendor::VendorKind;

/// A tagged fabrication release. Persisted under `ProjectData.releases`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Release {
    /// User-chosen rev tag, e.g. "rev_01" or "rev_01a".
    pub tag: String,
    /// When the release was created.
    pub created_at: DateTime<Utc>,
    /// Path to the archived gerber/drill package (`<project>/outputs/<tag>/<name>.zip`).
    pub archive_path: PathBuf,
    /// Path to the RELEASE_NOTES.md file (always created next to the zip).
    pub notes_path: PathBuf,
    /// Short "what this board is about" description (from the modal).
    pub description: String,
    /// User-provided changes-from-previous-version text (markdown).
    pub changes: String,
    /// Detected KiCad version at release time.
    pub kicad_version: Option<String>,
    /// Git commit hash at release time, if the project dir is inside a repo.
    pub git_hash: Option<String>,
    /// Whether the zip filename includes the date (e.g. `_18Apr2026`).
    pub include_date_in_name: bool,
    /// Whether RELEASE_NOTES.md was bundled into the zip.
    pub include_notes_in_zip: bool,
    /// Vendor target. `None` = vendor-neutral standard release; `Some(...)`
    /// adds vendor-specific files to the zip (e.g. PCBWay's fab specs).
    /// `#[serde(default)]` keeps old DB records loadable as Standard.
    #[serde(default)]
    pub target: Option<VendorKind>,
}

/// Input collected from the Release modal.
pub struct ReleaseRequest {
    pub rev_tag: String,
    pub description: String,
    pub changes: String,
    pub include_date_in_name: bool,
    pub include_notes_in_zip: bool,
    pub target: Option<VendorKind>,
}

/// Where source artifacts are found; supplied by the caller after a normal
/// Generate+Load has produced gerbers.
pub struct ReleaseSources<'a> {
    pub pcb_path: &'a Path,
    pub gerber_dir: &'a Path,
    pub kicad_cli: std::process::Command,
    pub kicad_version: Option<String>,
    pub os_description: String,
}

/// Result of a create-release operation.
pub struct ReleaseOutcome {
    pub release: Release,
}

/// Create a release on disk: `<project_dir>/outputs/<rev_tag>/` containing
/// - `<project>_<rev>[_<DDMMMYYYY>].zip` (gerbers + drill [+ notes if opted in])
/// - `RELEASE_NOTES.md` (always written next to the zip)
///
/// Returns the `Release` metadata; the caller is responsible for appending
/// it to the project's DB record.
pub fn create_release(
    req: &ReleaseRequest,
    sources: ReleaseSources<'_>,
    logger: &ReactiveEventLogger,
) -> Result<ReleaseOutcome, String> {
    let project_dir = sources
        .pcb_path
        .parent()
        .ok_or_else(|| "PCB path has no parent directory".to_string())?;

    let project_stem = sources
        .pcb_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("project");

    let outputs_root = project_dir.join("outputs");
    let rev_dir = outputs_root.join(&req.rev_tag);
    std::fs::create_dir_all(&rev_dir)
        .map_err(|e| format!("Failed to create release dir {}: {}", rev_dir.display(), e))?;

    logger.log_info(&format!("Release dir: {}", rev_dir.display()));

    // 1. Export drill files (gerbers are already in sources.gerber_dir).
    let drill_dir = rev_dir.join("drill_staging");
    std::fs::create_dir_all(&drill_dir)
        .map_err(|e| format!("Failed to create drill staging dir: {}", e))?;
    export_drill(sources.kicad_cli, sources.pcb_path, &drill_dir, logger)?;

    // 2. Resolve git commit (optional). A `-dirty` suffix means the project
    //    directory has uncommitted or untracked files — the hash alone does
    //    not identify the files that went into this release.
    let git_hash = git_head_short(project_dir);
    if let Some(ref h) = git_hash {
        logger.log_info(&format!("Git commit: {}", h));
        if h.ends_with("-dirty") {
            logger.log_warning(
                "Working tree is dirty — release contains uncommitted or untracked \
                 files. Commit your changes before releasing to make the git hash \
                 authoritative.",
            );
        }
    }

    let now_utc = Utc::now();
    let now_local = Local::now();
    let date_stamp = now_local.format("%d%b%Y").to_string();

    // 3. Zip filename.
    let zip_name = if req.include_date_in_name {
        format!("{}_{}_{}.zip", project_stem, req.rev_tag, date_stamp)
    } else {
        format!("{}_{}.zip", project_stem, req.rev_tag)
    };
    let zip_path = rev_dir.join(&zip_name);

    // 4. Write RELEASE_NOTES.md next to the zip.
    let notes_path = rev_dir.join("RELEASE_NOTES.md");
    let notes_markdown = build_release_notes(
        project_stem,
        &req.rev_tag,
        &req.description,
        &req.changes,
        &now_local.format("%Y-%m-%d %H:%M:%S %Z").to_string(),
        sources.kicad_version.as_deref(),
        &sources.os_description,
        git_hash.as_deref(),
    );
    std::fs::write(&notes_path, &notes_markdown)
        .map_err(|e| format!("Failed to write RELEASE_NOTES.md: {}", e))?;

    // 4b. Fabrication data: centroid (CPL) + BOM (CSV and XLSX), written next
    //     to the zip and bundled into it. Non-fatal — a BOM parse failure logs
    //     a warning and the release proceeds without these files.
    let mut fab_files: Vec<PathBuf> = Vec::new();
    match crate::bom::extract_bom(sources.pcb_path) {
        Ok(entries) if !entries.is_empty() => {
            type Writer = fn(&[crate::bom::BomEntry], &Path) -> Result<(), String>;
            let targets: [(&str, PathBuf, Writer); 3] = [
                (
                    "Centroid file",
                    rev_dir.join(format!("{}-centroid.csv", project_stem)),
                    crate::export::centroid::write_cpl_csv,
                ),
                (
                    "BOM CSV",
                    rev_dir.join(format!("{}-bom.csv", project_stem)),
                    crate::export::bom::write_bom_csv,
                ),
                (
                    "BOM XLSX",
                    rev_dir.join(format!("{}-bom.xlsx", project_stem)),
                    crate::export::bom::write_bom_xlsx,
                ),
            ];
            for (label, path, write) in targets {
                match write(&entries, &path) {
                    Ok(()) => {
                        logger.log_info(&format!("{}: {}", label, path.display()));
                        fab_files.push(path);
                    }
                    Err(e) => logger.log_warning(&format!("{} skipped: {}", label, e)),
                }
            }
        }
        Ok(_) => logger
            .log_warning("BOM extraction found no components — centroid/BOM not bundled."),
        Err(e) => logger.log_warning(&format!(
            "BOM extraction failed — centroid/BOM not bundled: {}",
            e
        )),
    }

    // 4c. Vendor-specific extras (PCBWay fab-specs sheet, etc.). Treated
    //     the same as the generic fab_files above — non-fatal if it
    //     fails, and just gets appended to the zip.
    if let Some(vendor) = req.target {
        match vendor {
            VendorKind::PcbWay => {
                match crate::vendor::pcbway::compute_fab_stats(sources.pcb_path) {
                    Ok(stats) => {
                        let path = rev_dir.join("PCBWAY_FAB_SPECS.md");
                        match crate::vendor::pcbway::write_fab_specs_md(
                            &stats, project_stem, &req.rev_tag, &path,
                        ) {
                            Ok(()) => {
                                let dims = match (stats.board_width_mm, stats.board_height_mm) {
                                    (Some(w), Some(h)) => format!(", {:.2}×{:.2} mm", w, h),
                                    _ => String::new(),
                                };
                                logger.log_info(&format!(
                                    "PCBWay fab specs: {} (smt {}, tht {}, smt pads {}, top {}, bot {}{})",
                                    path.display(),
                                    stats.smt_parts,
                                    stats.tht_parts,
                                    stats.smt_pads,
                                    stats.parts_top,
                                    stats.parts_bottom,
                                    dims,
                                ));
                                fab_files.push(path);
                            }
                            Err(e) => logger.log_warning(
                                &format!("PCBWay fab specs skipped: {}", e),
                            ),
                        }
                    }
                    Err(e) => logger.log_warning(
                        &format!("PCBWay fab specs skipped — could not scan PCB: {}", e),
                    ),
                }
            }
            // Other vendors (Sierra, JLCPCB, OshPark, Custom) fall through
            // to the standard release — their extras will land here when
            // the corresponding vendor submodules ship.
            _ => {}
        }
    }

    // 5. Build the archive.
    let notes_for_zip = if req.include_notes_in_zip {
        Some((notes_path.as_path(), "RELEASE_NOTES.md"))
    } else {
        None
    };
    write_release_zip(&zip_path, sources.gerber_dir, &drill_dir, notes_for_zip, &fab_files)
        .map_err(|e| format!("Failed to build zip: {}", e))?;

    // 6. Clean up drill staging (its contents are in the zip now).
    let _ = std::fs::remove_dir_all(&drill_dir);

    logger.log_info(&format!("Release archive: {}", zip_path.display()));

    // Open the release folder in the system file manager.
    open_directory(&rev_dir);
    logger.log_info(&format!("Opened release folder: {}", rev_dir.display()));

    let release = Release {
        tag: req.rev_tag.clone(),
        created_at: now_utc,
        archive_path: zip_path,
        notes_path,
        description: req.description.clone(),
        changes: req.changes.clone(),
        kicad_version: sources.kicad_version,
        git_hash,
        include_date_in_name: req.include_date_in_name,
        include_notes_in_zip: req.include_notes_in_zip,
        target: req.target,
    };

    Ok(ReleaseOutcome { release })
}

/// Scan a project's `outputs/` directory and return one `Release` per
/// `rev_*/` subdirectory that contains a `.zip`.
///
/// Self-healing: lets the project tree show releases whose DB record was
/// lost (manual DB edits, schema migration that dropped releases, etc.).
/// Disk is the source of truth — if the zip is there, the release
/// existed. The caller decides whether to merge these into the DB.
///
/// Fields the DB record would have carried (description, changes,
/// git_hash, kicad_version) come back as empty/None — we only have the
/// filesystem here. `include_date_in_name` is inferred from the zip's
/// filename pattern.
pub fn discover_releases_on_disk(pcb_path: &Path) -> Vec<Release> {
    let Some(project_dir) = pcb_path.parent() else {
        return Vec::new();
    };
    let outputs_dir = project_dir.join("outputs");
    if !outputs_dir.is_dir() {
        return Vec::new();
    }

    let project_stem = pcb_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("project");

    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&outputs_dir) else {
        return Vec::new();
    };
    for entry in entries.flatten() {
        let rev_dir = entry.path();
        if !rev_dir.is_dir() {
            continue;
        }
        let Some(tag) = rev_dir.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        // Only consider tags that look like release tags. rev_<digits>[suffix].
        if !tag.starts_with("rev_") {
            continue;
        }

        // Find the .zip inside (there should be exactly one).
        let zip_entry = std::fs::read_dir(&rev_dir).ok().and_then(|it| {
            it.flatten()
                .map(|e| e.path())
                .find(|p| {
                    p.is_file()
                        && p.extension().and_then(|e| e.to_str()) == Some("zip")
                        && p.file_name()
                            .and_then(|n| n.to_str())
                            .is_some_and(|n| n.starts_with(&format!("{}_{}", project_stem, tag)))
                })
        });
        let Some(archive_path) = zip_entry else {
            continue;
        };

        let zip_name = archive_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        // include_date_in_name = filename has a trailing _DDMmmYYYY chunk.
        // Simple heuristic: more underscores than just project + tag means a date.
        let expected_base = format!("{}_{}", project_stem, tag);
        let include_date_in_name = zip_name != expected_base
            && zip_name.starts_with(&format!("{}_", expected_base));

        // Use the zip's mtime as a reasonable created_at.
        let created_at = std::fs::metadata(&archive_path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| DateTime::<Utc>::from(t).into())
            .unwrap_or_else(Utc::now);

        let notes_path = rev_dir.join("RELEASE_NOTES.md");

        out.push(Release {
            tag: tag.to_string(),
            created_at,
            archive_path,
            notes_path,
            description: String::new(),
            changes: String::new(),
            kicad_version: None,
            git_hash: None,
            include_date_in_name,
            include_notes_in_zip: true,
            target: None,
        });
    }
    out
}

/// Suggest the next rev tag based on the project's existing releases.
/// e.g. ["rev_01"] → "rev_02"; [] → "rev_01"; anything unrecognized → "rev_01".
pub fn suggest_next_rev_tag(existing: &[Release]) -> String {
    let mut highest: u32 = 0;
    for r in existing {
        if let Some(num_str) = r.tag.strip_prefix("rev_") {
            // Take leading digits only, tolerate suffixes like "_01a".
            let digits: String = num_str.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(n) = digits.parse::<u32>() {
                highest = highest.max(n);
            }
        }
    }
    format!("rev_{:02}", highest + 1)
}

// ── internals ────────────────────────────────────────────────────────────

fn export_drill(
    mut cmd: std::process::Command,
    pcb_path: &Path,
    drill_dir: &Path,
    logger: &ReactiveEventLogger,
) -> Result<(), String> {
    let output = cmd
        .arg("pcb")
        .arg("export")
        .arg("drill")
        .arg("--output")
        .arg(drill_dir)
        .arg(pcb_path)
        .output()
        .map_err(|e| format!("kicad-cli drill export failed to spawn: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("kicad-cli drill export failed: {}", stderr.trim()));
    }
    let listed: Vec<_> = std::fs::read_dir(drill_dir)
        .map_err(|e| format!("Failed to read drill dir: {}", e))?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    logger.log_info(&format!("Drill files: {}", listed.join(", ")));
    Ok(())
}

fn git_head_short(project_dir: &Path) -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(project_dir)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let mut s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        return None;
    }
    if git_tree_dirty(project_dir) {
        s.push_str("-dirty");
    }
    Some(s)
}

/// Are there tracked-but-uncommitted or untracked files *within the project
/// directory*? The pathspec (`--  .`) scopes the check so that unrelated edits
/// elsewhere in the repo don't flag this release as dirty. Gitignored files
/// are correctly excluded by `git status --porcelain`.
fn git_tree_dirty(project_dir: &Path) -> bool {
    match std::process::Command::new("git")
        .args(["status", "--porcelain", "--", "."])
        .current_dir(project_dir)
        .output()
    {
        Ok(o) if o.status.success() => !o.stdout.is_empty(),
        _ => false,
    }
}

#[allow(clippy::too_many_arguments)]
fn build_release_notes(
    project: &str,
    rev_tag: &str,
    description: &str,
    changes: &str,
    when: &str,
    kicad_version: Option<&str>,
    os_description: &str,
    git_hash: Option<&str>,
) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {} — {}\n\n", project, rev_tag));
    out.push_str(&format!("**Released:** {}\n\n", when));
    out.push_str(&format!(
        "**KiCad version:** {}\n\n",
        kicad_version.unwrap_or("(not detected)")
    ));
    out.push_str(&format!("**Host OS:** {}\n\n", os_description));
    match git_hash {
        None => out.push_str("**Git commit:** (not in a git repository)\n\n"),
        Some(h) if h.ends_with("-dirty") => {
            let clean = h.trim_end_matches("-dirty");
            out.push_str(&format!(
                "**Git commit:** `{}` ⚠ working tree dirty — \
                 release contains uncommitted or untracked files \
                 not represented by this hash\n\n",
                clean
            ));
        }
        Some(h) => out.push_str(&format!("**Git commit:** `{}`\n\n", h)),
    }
    out.push_str("## Description\n\n");
    if description.trim().is_empty() {
        out.push_str("_(none provided)_\n\n");
    } else {
        out.push_str(description.trim());
        out.push_str("\n\n");
    }
    out.push_str("## Changes from previous version\n\n");
    if changes.trim().is_empty() {
        out.push_str("_(none provided)_\n\n");
    } else {
        out.push_str(changes.trim());
        out.push_str("\n\n");
    }
    out
}

fn write_release_zip(
    zip_path: &Path,
    gerber_dir: &Path,
    drill_dir: &Path,
    notes: Option<(&Path, &str)>,
    extra_files: &[PathBuf],
) -> std::io::Result<()> {
    let file = File::create(zip_path)?;
    let mut zw = ZipWriter::new(file);
    let opts: SimpleFileOptions = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    // Gerbers
    for entry in std::fs::read_dir(gerber_dir)? {
        let path = entry?.path();
        if path.is_file() {
            add_to_zip(&mut zw, &path, None, opts)?;
        }
    }

    // Drills
    for entry in std::fs::read_dir(drill_dir)? {
        let path = entry?.path();
        if path.is_file() {
            add_to_zip(&mut zw, &path, None, opts)?;
        }
    }

    // Notes
    if let Some((notes_path, name_in_zip)) = notes {
        add_to_zip(&mut zw, notes_path, Some(name_in_zip), opts)?;
    }

    // Fabrication data (centroid, BOM)
    for path in extra_files {
        if path.is_file() {
            add_to_zip(&mut zw, path, None, opts)?;
        }
    }

    zw.finish()?;
    Ok(())
}

/// Open `dir` in the system file manager. Best-effort — failures are ignored.
fn open_directory(dir: &Path) {
    let opener = if cfg!(target_os = "windows") {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(opener).arg(dir).spawn();
}

fn add_to_zip(
    zw: &mut ZipWriter<File>,
    src: &Path,
    alias: Option<&str>,
    opts: SimpleFileOptions,
) -> std::io::Result<()> {
    let name = alias
        .map(|s| s.to_string())
        .unwrap_or_else(|| src.file_name().unwrap().to_string_lossy().into_owned());
    zw.start_file(name, opts)?;
    let mut f = File::open(src)?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;
    zw.write_all(&buf)?;
    Ok(())
}

/// Extract the `.gbr` / `.drl` entries from a release ZIP into a
/// per-archive cache directory and return that directory.
///
/// Cache location: `<user_cache_dir>/copperforge/extracted/<archive_stem>/`.
/// Re-extraction is skipped if the cache dir is at-or-newer than the
/// ZIP (cheap mtime check), so the second-and-later "Load Release
/// Gerbers" click on the same rev is effectively instant.
///
/// Only `.gbr` and `.drl` entries are extracted — BOM, centroid, and
/// RELEASE_NOTES live alongside the ZIP and aren't needed by the
/// gerber viewer. Sub-directories inside the archive are flattened
/// (the bare filename is used for the on-disk path).
pub fn extract_release_gerbers(archive_path: &Path) -> io::Result<PathBuf> {
    let cache_root = dirs::cache_dir()
        .ok_or_else(|| io::Error::other("no user cache directory available"))?
        .join("copperforge")
        .join("extracted");
    let stem = archive_path
        .file_stem()
        .ok_or_else(|| io::Error::other("release archive has no file stem"))?;
    let target = cache_root.join(stem);

    // Up-to-date check — skip the whole extract if the cache dir's
    // mtime is at-or-newer than the ZIP's. (Linux updates dir mtime on
    // each create_dir_all so this naturally tracks "last extracted at".)
    if let (Ok(cache_meta), Ok(zip_meta)) =
        (fs::metadata(&target), fs::metadata(archive_path))
    {
        if let (Ok(cache_mtime), Ok(zip_mtime)) =
            (cache_meta.modified(), zip_meta.modified())
        {
            if cache_mtime >= zip_mtime {
                return Ok(target);
            }
        }
    }

    // Fresh extract — wipe + recreate to avoid stale entries from old runs.
    if target.exists() {
        let _ = fs::remove_dir_all(&target);
    }
    fs::create_dir_all(&target)?;

    let file = File::open(archive_path)?;
    let mut archive = ZipArchive::new(file).map_err(io::Error::other)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(io::Error::other)?;
        let name = entry.name().to_string();
        let lower = name.to_ascii_lowercase();
        // Only the manufacturing geometry — gerbers + drill. Everything
        // else is irrelevant to the viewer.
        if !lower.ends_with(".gbr") && !lower.ends_with(".drl") {
            continue;
        }
        // Flatten any archive subdirs — flat dir of files is what
        // `LayerStore::load_from_directory` expects.
        let filename = Path::new(&name).file_name().ok_or_else(|| {
            io::Error::other(format!("zip entry has no filename: {name}"))
        })?;
        let out_path = target.join(filename);
        let mut out_file = File::create(&out_path)?;
        io::copy(&mut entry, &mut out_file)?;
    }

    Ok(target)
}
