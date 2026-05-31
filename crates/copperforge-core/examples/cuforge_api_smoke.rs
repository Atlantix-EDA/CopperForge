//! Slice 3 smoke test — exercise the typed client against a running
//! cuforge-services. Symmetric to the curl tests in slices 1 + 2.
//!
//! Run: start the server (`cd cuforge-services && cargo run`), then
//! `cargo run --example cuforge_api_smoke -p copperforge-core`.

use copperforge_core::cuforge_api::{ApiCallError, CuforgeApi, NewProject, NewRelease, ProjectUpdate};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

const SAMPLE_ZIP: &str = "assets/media/cparti-fpga-dev-board.zip";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api = CuforgeApi::new(
        std::env::var("CUFORGE_SERVICES_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8421".to_string()),
    );

    println!("── base url: {}", api.base_url());

    // Load the bundled CPArti zip from the workspace root.
    let workspace_root = workspace_root();
    let zip_path = workspace_root.join(SAMPLE_ZIP);
    let zip_bytes = std::fs::read(&zip_path)
        .map_err(|e| format!("read {}: {e}", zip_path.display()))?;
    let original_sha = hex::encode(Sha256::digest(&zip_bytes));
    println!("── sample zip: {} bytes, sha {}", zip_bytes.len(), &original_sha[..16]);

    // ─── Projects ───────────────────────────────────────────────────────────
    println!("\n[1] list_projects (initial)");
    let initial = api.list_projects().await?;
    println!("    got {} project(s)", initial.len());

    println!("\n[2] create_project");
    let created = api
        .create_project(&NewProject {
            name: "Slice 3 smoke".to_string(),
            description: "Created by cuforge_api_smoke example".to_string(),
            author: "smoke-test".to_string(),
            tags: vec!["smoke".to_string(), "slice3".to_string()],
            version: "0.1.0".to_string(),
            ..Default::default()
        })
        .await?;
    println!("    id={} created_at={}", created.id, created.created_at);

    println!("\n[3] get_project");
    let fetched = api.get_project(created.id).await?;
    assert_eq!(fetched.id, created.id);
    assert_eq!(fetched.name, "Slice 3 smoke");
    println!("    name='{}' tags={:?}", fetched.name, fetched.tags);

    println!("\n[4] update_project (partial: description + tags)");
    let updated = api
        .update_project(
            created.id,
            &ProjectUpdate {
                description: Some("Updated via partial PUT".to_string()),
                tags: Some(vec!["smoke".to_string(), "updated".to_string()]),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(updated.description, "Updated via partial PUT");
    assert_eq!(updated.tags, vec!["smoke".to_string(), "updated".to_string()]);
    assert!(updated.updated_at > updated.created_at);
    println!("    updated_at > created_at ✓, partial fields applied ✓");

    println!("\n[5] validation: empty name on create");
    match api
        .create_project(&NewProject {
            name: "  ".to_string(),
            ..Default::default()
        })
        .await
    {
        Err(ApiCallError::Server { status: 400, error }) => {
            println!("    HTTP 400 [{}] {} ✓", error.code, error.message);
            assert_eq!(error.code, "validation");
        }
        other => panic!("expected 400 validation error, got {other:?}"),
    }

    // ─── Releases ───────────────────────────────────────────────────────────
    println!("\n[6] list_releases (empty for new project)");
    let releases = api.list_releases(created.id).await?;
    assert!(releases.is_empty());
    println!("    got 0 ✓");

    println!("\n[7] create_release (multipart upload of real CPArti zip)");
    let release = api
        .create_release(
            created.id,
            &NewRelease {
                revision: "v1.0".to_string(),
                vendor: "pcbway".to_string(),
                notes: "smoke test release".to_string(),
            },
            "cparti-fpga-dev-board.zip",
            zip_bytes.clone(),
        )
        .await?;
    println!(
        "    id={} size={} sha={}",
        release.id,
        release.file_size,
        &release.file_sha256[..16]
    );
    assert_eq!(release.file_sha256, original_sha);
    assert_eq!(release.file_size as usize, zip_bytes.len());
    println!("    server SHA matches client SHA ✓");

    println!("\n[8] get_release");
    let got = api.get_release(release.id).await?;
    assert_eq!(got.revision, "v1.0");
    assert_eq!(got.vendor, "pcbway");
    println!("    revision='{}' vendor='{}' ✓", got.revision, got.vendor);

    println!("\n[9] download_release + verify SHA-256");
    let downloaded = api.download_release(release.id).await?;
    let downloaded_sha = hex::encode(Sha256::digest(&downloaded));
    assert_eq!(downloaded_sha, original_sha);
    assert_eq!(downloaded.len(), zip_bytes.len());
    println!(
        "    downloaded {} bytes, sha {} ✓ (matches original)",
        downloaded.len(),
        &downloaded_sha[..16]
    );

    println!("\n[10] list_releases (1 entry now)");
    let releases = api.list_releases(created.id).await?;
    assert_eq!(releases.len(), 1);
    println!("    got 1 ✓");

    println!("\n[11] delete_release");
    api.delete_release(release.id).await?;
    match api.get_release(release.id).await {
        Err(ApiCallError::Server { status: 404, .. }) => println!("    404 ✓"),
        other => panic!("expected 404, got {other:?}"),
    }

    // ─── Cascade cleanup ────────────────────────────────────────────────────
    println!("\n[12] delete_project (cascade)");
    api.delete_project(created.id).await?;
    match api.get_project(created.id).await {
        Err(ApiCallError::Server { status: 404, .. }) => println!("    project gone ✓"),
        other => panic!("expected 404, got {other:?}"),
    }

    println!("\n✅ slice 3 smoke test green");
    Ok(())
}

fn workspace_root() -> PathBuf {
    // Examples run from the crate dir; walk up to the workspace root.
    let mut p = std::env::current_dir().expect("cwd");
    while !p.join("Cargo.toml").exists()
        || std::fs::read_to_string(p.join("Cargo.toml"))
            .map(|s| !s.contains("[workspace]"))
            .unwrap_or(true)
    {
        if !p.pop() {
            panic!("could not locate workspace root from {:?}", std::env::current_dir());
        }
    }
    p
}
