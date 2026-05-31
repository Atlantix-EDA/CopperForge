//! Headless CLI mode for `copperforge`.
//!
//! Runs without the GUI:
//!
//! ```text
//! copperforge release path/to/board.kicad_pcb            # cuts rev_01 (or next)
//! copperforge release board.kicad_pcb --rev rev_demo     # explicit rev tag
//! copperforge release board.kicad_pcb --out ~/releases   # override output dir
//! copperforge release board.kicad_pcb --pcbway           # bundle fab-specs sheet
//! ```
//!
//! Works from any directory — paths are canonicalized at parse time so the
//! user doesn't have to `cd` into the project. Produces the same release zip
//! the desktop GUI cuts: gerbers + drill + BOM (CSV + XLSX) + centroid +
//! RELEASE_NOTES.md, with optional `PCBWAY_FAB_SPECS.md` on `--pcbway`.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use copperforge_core::CopperForgeApp;
use copperforge_core::event_logger::{LogColors, ReactiveEventLogger, ReactiveEventLoggerState};
use copperforge_core::project::manager::ProjectConfig;
use copperforge_core::project::gerber_ops::generate_gerbers_from_pcb;
use copperforge_core::release::{
    create_release, suggest_next_rev_tag, ReleaseRequest, ReleaseSources,
};
use copperforge_core::vendor::VendorKind;
use egui_mobius_reactive::Dynamic;

#[derive(Parser, Debug)]
#[command(
    name = "copperforge",
    version,
    about = "PCB release & manufacturing companion for KiCad",
    long_about = "Run without arguments to launch the GUI.\n\
                  Run `copperforge release <pcb>` to cut a release zip from\n\
                  the terminal — works from any directory."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Produce a release zip (gerbers + drill + BOM + centroid + notes)
    /// from a `.kicad_pcb` file. No GUI launched.
    Release(ReleaseArgs),
}

#[derive(clap::Args, Debug)]
pub struct ReleaseArgs {
    /// Path to the `.kicad_pcb` to release. Relative paths are
    /// canonicalised against the current working directory, so this
    /// works from anywhere.
    pub pcb: PathBuf,

    /// Rev tag (e.g. `rev_01`, `rev_demo`). If omitted, the next
    /// sequential `rev_NN` is picked by scanning the project's
    /// `outputs/` directory.
    #[arg(long)]
    pub rev: Option<String>,

    /// Output directory. Default: `<project_dir>/outputs/<rev>/`.
    #[arg(long)]
    pub out: Option<PathBuf>,

    /// Bundle `PCBWAY_FAB_SPECS.md` (board dimensions + SMT/THT part
    /// and pad counts) into the zip.
    #[arg(long)]
    pub pcbway: bool,

    /// Short description carried into `RELEASE_NOTES.md`.
    #[arg(long, default_value = "")]
    pub description: String,

    /// Changes-from-previous-rev text (markdown), carried into
    /// `RELEASE_NOTES.md`. Pass `-` to read from stdin.
    #[arg(long, default_value = "")]
    pub changes: String,

    /// Skip the `_DDMmmYYYY` suffix on the zip filename.
    #[arg(long)]
    pub no_date: bool,
}

/// Run the CLI command. Returns the process exit code so `main` can
/// propagate it cleanly.
pub fn run(cli: Cli) -> ExitCode {
    match cli.command {
        Some(Command::Release(args)) => match run_release(args) {
            Ok(zip_path) => {
                println!("✓ {}", zip_path.display());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("✗ {}", e);
                ExitCode::FAILURE
            }
        },
        None => unreachable!("dispatch from main only forwards a Some(...)"),
    }
}

fn run_release(args: ReleaseArgs) -> Result<PathBuf, String> {
    // ── 1. Resolve paths ──────────────────────────────────────────
    let pcb_path = args
        .pcb
        .canonicalize()
        .map_err(|e| format!("Cannot resolve {}: {}", args.pcb.display(), e))?;
    if pcb_path.extension().and_then(|e| e.to_str()) != Some("kicad_pcb") {
        return Err(format!(
            "{} is not a .kicad_pcb file",
            pcb_path.display()
        ));
    }
    let project_dir = pcb_path
        .parent()
        .ok_or_else(|| "PCB path has no parent directory".to_string())?
        .to_path_buf();

    // ── 2. Probe kicad-cli ────────────────────────────────────────
    let config_path = dirs::config_dir()
        .map(|d| d.join("copperforge"))
        .unwrap_or_else(|| PathBuf::from("."));
    let config = ProjectConfig::load_from_file(&config_path).unwrap_or_default();
    let (kicad_version, kicad_cli_method, _candidates) =
        CopperForgeApp::probe_kicad_cli(&config);
    let kicad_cli_method = kicad_cli_method.ok_or_else(|| {
        "kicad-cli not found. Install KiCad (PATH, Flatpak, or Snap) and retry.".to_string()
    })?;
    let kicad_cli = CopperForgeApp::build_kicad_cli_command(&kicad_cli_method);

    // ── 3. Logger sink (CLI mode prints log lines as they queue) ─
    let logger_state = Dynamic::new(ReactiveEventLoggerState::new());
    let log_colors = Dynamic::new(LogColors::default());
    let logger = ReactiveEventLogger::with_colors(&logger_state, &log_colors);

    // ── 4. Generate gerbers + drills ──────────────────────────────
    eprintln!("→ Generating gerbers via kicad-cli ({})…", kicad_cli_method);
    let gerber_dir = generate_gerbers_from_pcb(&pcb_path, &kicad_cli_method, &logger)
        .ok_or_else(|| "Gerber generation failed (see kicad-cli output above).".to_string())?;

    // ── 5. Build the release request ──────────────────────────────
    // Existing rev list — scan the project's outputs/ for `rev_*`
    // subdirectories so `suggest_next_rev_tag` can pick the next number.
    let existing_releases = scan_existing_revs(&project_dir);
    let rev_tag = args
        .rev
        .clone()
        .unwrap_or_else(|| suggest_next_rev_tag(&existing_releases));

    // Read `--changes -` from stdin if requested.
    let changes = if args.changes == "-" {
        use std::io::Read;
        let mut s = String::new();
        std::io::stdin()
            .read_to_string(&mut s)
            .map_err(|e| format!("Reading --changes from stdin: {}", e))?;
        s
    } else {
        args.changes.clone()
    };

    let req = ReleaseRequest {
        rev_tag: rev_tag.clone(),
        description: args.description.clone(),
        changes,
        include_date_in_name: !args.no_date,
        include_notes_in_zip: true,
        target: if args.pcbway { Some(VendorKind::PcbWay) } else { None },
    };

    let sources = ReleaseSources {
        pcb_path: &pcb_path,
        gerber_dir: &gerber_dir,
        kicad_cli,
        kicad_version,
        os_description: build_os_description(),
    };

    // ── 6. Cut the release ────────────────────────────────────────
    eprintln!("→ Packaging release '{}'…", rev_tag);
    let outcome = create_release(&req, sources, &logger)
        .map_err(|e| format!("Release packaging failed: {}", e))?;

    // ── 7. Optional --out: move the zip to the user-specified dir ─
    let zip_path = if let Some(out_dir) = args.out {
        std::fs::create_dir_all(&out_dir)
            .map_err(|e| format!("Create --out dir {}: {}", out_dir.display(), e))?;
        let dest = out_dir.join(
            outcome
                .release
                .archive_path
                .file_name()
                .ok_or_else(|| "Release archive has no filename".to_string())?,
        );
        std::fs::rename(&outcome.release.archive_path, &dest)
            .or_else(|_| {
                // rename can fail across filesystems; fall back to copy + remove.
                std::fs::copy(&outcome.release.archive_path, &dest)
                    .and_then(|_| std::fs::remove_file(&outcome.release.archive_path))
                    .map(|_| ())
            })
            .map_err(|e| format!("Move zip to {}: {}", dest.display(), e))?;
        dest
    } else {
        outcome.release.archive_path
    };

    // ── 8. Drain the log buffer to stderr — visibility into what
    //      kicad-cli + the packager actually did, without obscuring
    //      the final `✓ <path>` on stdout that callers (Make, CI)
    //      should be able to pipe.
    let state = logger_state.get();
    for entry in &state.logs {
        eprintln!("  [{:?}] {}", entry.log_type, entry.message);
    }

    Ok(zip_path)
}

/// Scan `<project_dir>/outputs/` for already-cut rev subdirectories,
/// returning a synthetic `Vec<Release>` populated only with `tag` so
/// `suggest_next_rev_tag` can do its rev counting. Other fields stay
/// empty — they're not used by the suggestion logic.
fn scan_existing_revs(project_dir: &std::path::Path) -> Vec<copperforge_core::release::Release> {
    use chrono::Utc;
    let outputs = project_dir.join("outputs");
    let Ok(entries) = std::fs::read_dir(&outputs) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let tag = e.file_name().to_string_lossy().into_owned();
            if !tag.starts_with("rev_") {
                return None;
            }
            Some(copperforge_core::release::Release {
                tag,
                created_at: Utc::now(),
                archive_path: PathBuf::new(),
                notes_path: PathBuf::new(),
                description: String::new(),
                changes: String::new(),
                kicad_version: None,
                git_hash: None,
                include_date_in_name: false,
                include_notes_in_zip: false,
                target: None,
            })
        })
        .collect()
}

/// Same `Host OS:` line the GUI release flow records, so notes from
/// CLI and GUI releases look identical.
fn build_os_description() -> String {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    format!("{} ({})", os, arch)
}
