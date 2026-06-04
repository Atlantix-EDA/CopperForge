//! Typed async client for the cuforge-services REST API.
//!
//! Slice 3 of WASM-Phase-E. Wraps `reqwest::Client` with one struct
//! (`CuforgeApi`) that exposes the project + release CRUD surface
//! defined in `develop/wasm-demo-plan.md`. Same source compiles for
//! native (hyper + rustls backend) and wasm32 (fetch backend) — that's
//! the whole point of choosing reqwest over ureq + gloo-net.
//!
//! ## Type duplication, deliberate (for now)
//!
//! The wire-format structs below — `Project`, `NewProject`, etc. —
//! mirror those in the private `cuforge-services-types` crate. We
//! shadow rather than path-depend because CopperForge is public and
//! cuforge-services is private; a path dep would break public-clone
//! builds. When the wire format stabilizes and is worth publishing as
//! its own crate, dedupe and replace these with re-exports.
//!
//! ## Health-check note
//!
//! The existing native-only `cuforge_client::spawn_health_poller` (ureq
//! + background thread) stays unchanged for now. This module is purely
//! the new project/release API surface. They can be unified later.

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ─── Wire-format types (shadow of cuforge-services-types) ───────────────────

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub author: String,
    pub pcb_path: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub version: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct NewProject {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub pcb_path: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub version: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ProjectUpdate {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pcb_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Release {
    pub id: Uuid,
    pub project_id: Uuid,
    pub revision: String,
    #[serde(default)]
    pub vendor: String,
    #[serde(default)]
    pub notes: String,
    pub file_name: String,
    pub file_size: i64,
    pub file_sha256: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct NewRelease {
    pub revision: String,
    #[serde(default)]
    pub vendor: String,
    #[serde(default)]
    pub notes: String,
}

/// Uniform error body returned by the server on non-2xx responses.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

// ─── Client error type ──────────────────────────────────────────────────────

/// What can go wrong when calling the API. Callers usually only care
/// whether the operation succeeded; `Display` gives a human-readable
/// reason for UI surfacing.
#[derive(Debug)]
pub enum ApiCallError {
    /// Network / transport / parse failure — couldn't even reach a
    /// server response.
    Transport(String),
    /// Server returned a structured error body (4xx / 5xx with the
    /// `ApiError` shape).
    Server { status: u16, error: ApiError },
    /// Server returned a non-2xx response without the structured
    /// `ApiError` body (or with one we couldn't parse).
    Status { status: u16, body: String },
}

impl std::fmt::Display for ApiCallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport(msg) => write!(f, "transport error: {msg}"),
            Self::Server { status, error } => {
                write!(f, "HTTP {status}: [{}] {}", error.code, error.message)
            }
            Self::Status { status, body } => write!(f, "HTTP {status}: {body}"),
        }
    }
}

impl std::error::Error for ApiCallError {}

impl From<reqwest::Error> for ApiCallError {
    fn from(e: reqwest::Error) -> Self {
        Self::Transport(e.to_string())
    }
}

pub type ApiResult<T> = Result<T, ApiCallError>;

// ─── Client ─────────────────────────────────────────────────────────────────

/// Async REST client for `cuforge-services`.
///
/// Clone is cheap (reqwest::Client is internally Arc-wrapped). Hold one
/// per app and clone freely into futures / panel handlers.
#[derive(Clone, Debug)]
pub struct CuforgeApi {
    client: reqwest::Client,
    /// Base URL like `http://127.0.0.1:8421` (no trailing slash).
    base_url: String,
}

impl CuforgeApi {
    /// Build a client pointed at the given base URL.
    ///
    /// On native, a 30s request timeout is set on the underlying client.
    /// On wasm the timeout is ignored by the fetch backend (browsers
    /// don't expose per-request timeouts through the Fetch API); rely
    /// on UI-level cancellation instead.
    pub fn new(base_url: impl Into<String>) -> Self {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        let client = build_client();
        Self { client, base_url }
    }

    /// Underlying reqwest client — exposed so callers can extend with
    /// custom headers (auth, tracing) when the time comes.
    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    // ── Projects ────────────────────────────────────────────────────────────

    pub async fn list_projects(&self) -> ApiResult<Vec<Project>> {
        let resp = self.client.get(self.url("/api/projects")).send().await?;
        decode_json(resp).await
    }

    pub async fn create_project(&self, req: &NewProject) -> ApiResult<Project> {
        let resp = self
            .client
            .post(self.url("/api/projects"))
            .json(req)
            .send()
            .await?;
        decode_json(resp).await
    }

    pub async fn get_project(&self, id: Uuid) -> ApiResult<Project> {
        let resp = self
            .client
            .get(self.url(&format!("/api/projects/{id}")))
            .send()
            .await?;
        decode_json(resp).await
    }

    pub async fn update_project(&self, id: Uuid, patch: &ProjectUpdate) -> ApiResult<Project> {
        let resp = self
            .client
            .put(self.url(&format!("/api/projects/{id}")))
            .json(patch)
            .send()
            .await?;
        decode_json(resp).await
    }

    pub async fn delete_project(&self, id: Uuid) -> ApiResult<()> {
        let resp = self
            .client
            .delete(self.url(&format!("/api/projects/{id}")))
            .send()
            .await?;
        decode_empty(resp).await
    }

    // ── Releases ────────────────────────────────────────────────────────────

    pub async fn list_releases(&self, project_id: Uuid) -> ApiResult<Vec<Release>> {
        let resp = self
            .client
            .get(self.url(&format!("/api/projects/{project_id}/releases")))
            .send()
            .await?;
        decode_json(resp).await
    }

    /// Upload a release zip for a project.
    ///
    /// `file_bytes` is the full file in memory — fine for typical
    /// gerber/drill bundles (a few MB). For very large archives we'd
    /// stream from a `reqwest::Body::wrap_stream`, but reqwest's
    /// wasm32 backend doesn't support streaming uploads as of 0.12, so
    /// we keep it simple and bytes-shaped for cross-target consistency.
    pub async fn create_release(
        &self,
        project_id: Uuid,
        metadata: &NewRelease,
        file_name: impl Into<String>,
        file_bytes: Vec<u8>,
    ) -> ApiResult<Release> {
        let metadata_json = serde_json::to_string(metadata)
            .map_err(|e| ApiCallError::Transport(format!("metadata json: {e}")))?;

        let metadata_part = reqwest::multipart::Part::text(metadata_json)
            .mime_str("application/json")
            .map_err(|e| ApiCallError::Transport(format!("metadata mime: {e}")))?;

        let file_part = reqwest::multipart::Part::bytes(file_bytes)
            .file_name(file_name.into())
            .mime_str("application/zip")
            .map_err(|e| ApiCallError::Transport(format!("file mime: {e}")))?;

        let form = reqwest::multipart::Form::new()
            .part("metadata", metadata_part)
            .part("file", file_part);

        let resp = self
            .client
            .post(self.url(&format!("/api/projects/{project_id}/releases")))
            .multipart(form)
            .send()
            .await?;
        decode_json(resp).await
    }

    pub async fn get_release(&self, id: Uuid) -> ApiResult<Release> {
        let resp = self
            .client
            .get(self.url(&format!("/api/releases/{id}")))
            .send()
            .await?;
        decode_json(resp).await
    }

    /// Download a release's file as raw bytes.
    ///
    /// In the browser this is the path to "save to disk" — callers
    /// turn the `Vec<u8>` into a Blob + anchor click. On native the
    /// caller writes it to a filesystem path of its choosing.
    pub async fn download_release(&self, id: Uuid) -> ApiResult<Vec<u8>> {
        let resp = self
            .client
            .get(self.url(&format!("/api/releases/{id}/download")))
            .send()
            .await?;
        let resp = check_status(resp).await?;
        let bytes = resp.bytes().await?;
        Ok(bytes.to_vec())
    }

    pub async fn delete_release(&self, id: Uuid) -> ApiResult<()> {
        let resp = self
            .client
            .delete(self.url(&format!("/api/releases/{id}")))
            .send()
            .await?;
        decode_empty(resp).await
    }
}

// ─── Helpers ────────────────────────────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
fn build_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("default reqwest client should build")
}

#[cfg(target_arch = "wasm32")]
fn build_client() -> reqwest::Client {
    // The wasm backend ignores timeout; no other knobs need turning.
    let _ = Duration::from_secs(0); // silence unused-import on Duration
    reqwest::Client::new()
}

async fn check_status(resp: reqwest::Response) -> ApiResult<reqwest::Response> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    let status_u16 = status.as_u16();
    let body = resp.text().await.unwrap_or_default();
    // Try the structured error shape first; fall back to a generic
    // status carrying the raw body so the caller can still log it.
    match serde_json::from_str::<ApiError>(&body) {
        Ok(error) => Err(ApiCallError::Server {
            status: status_u16,
            error,
        }),
        Err(_) => Err(ApiCallError::Status {
            status: status_u16,
            body,
        }),
    }
}

async fn decode_json<T: for<'de> Deserialize<'de>>(resp: reqwest::Response) -> ApiResult<T> {
    let resp = check_status(resp).await?;
    let body = resp.text().await?;
    serde_json::from_str(&body)
        .map_err(|e| ApiCallError::Transport(format!("response json parse: {e} (body: {body})")))
}

async fn decode_empty(resp: reqwest::Response) -> ApiResult<()> {
    let _ = check_status(resp).await?;
    Ok(())
}
