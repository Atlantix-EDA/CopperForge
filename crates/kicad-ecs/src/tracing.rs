//! Tracing utilities for kicad-ecs
//! 
//! This module provides convenient functions for setting up structured logging
//! with appropriate filters and formatting.

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Initialize tracing with sensible defaults
/// 
/// This sets up:
/// - Environment-based log level filtering (RUST_LOG)
/// - Structured JSON output if KICAD_ECS_JSON_LOGS=1
/// - Pretty console output otherwise
pub fn init() {
    init_with_filter("info")
}

/// Initialize tracing with a custom default filter
pub fn init_with_filter(default_filter: &str) {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| default_filter.into());

    let registry = tracing_subscriber::registry().with(env_filter);

    // Use JSON output if requested via environment variable
    if std::env::var("KICAD_ECS_JSON_LOGS").is_ok() {
        registry
            .with(tracing_subscriber::fmt::layer().json())
            .init();
    } else {
        registry
            .with(
                tracing_subscriber::fmt::layer()
                    .with_target(false)
                    .with_thread_ids(true)
                    .with_file(true)
                    .with_line_number(true)
            )
            .init();
    }
}

/// Initialize tracing for examples and development
/// 
/// Uses more verbose logging and includes debug info
pub fn init_for_examples() {
    init_with_filter("kicad_ecs=debug,info")
}