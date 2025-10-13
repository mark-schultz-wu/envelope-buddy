//! `EnvelopeBuddy` - A Discord bot for envelope budgeting
//!
//! This crate provides a complete envelope budgeting system accessible via Discord,
//! allowing users to track spending across categorized envelopes with monthly allocations,
//! rollover support, and comprehensive reporting.

// Deny the most critical lints that could lead to bugs or security issues
#![deny(
    // Security and correctness
    unsafe_code,
    unsafe_op_in_unsafe_fn,

    // Code quality - things that are almost always bugs
    unreachable_code,
    unreachable_patterns,
    unused_must_use,

    // Documentation - broken links are bugs
    rustdoc::broken_intra_doc_links,
    rustdoc::private_intra_doc_links,
)]
// Warn on things that should be fixed but aren't necessarily bugs
#![warn(
    // Documentation - missing docs should be added gradually
    missing_docs,

    // Clippy categories for overall code quality
    clippy::all,
    clippy::pedantic,
    clippy::nursery,

    // Performance
    clippy::inefficient_to_string,
    clippy::large_types_passed_by_value,
    clippy::needless_pass_by_value,
    clippy::unnecessary_wraps,

    // Correctness
    clippy::clone_on_ref_ptr,
    clippy::dbg_macro,
    clippy::exit,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented,
    clippy::unwrap_used,

    // Complexity and readability
    clippy::cognitive_complexity,
    clippy::large_enum_variant,
    clippy::match_same_arms,
    clippy::too_many_lines,

    // Style consistency
    clippy::enum_glob_use,
    clippy::inconsistent_struct_constructor,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::redundant_closure_for_method_calls,
    clippy::semicolon_if_nothing_returned,
    clippy::wildcard_imports,

    // Future compatibility
    future_incompatible,
    rust_2018_idioms,
)]
// Allow some pedantic lints that are too noisy or not applicable
#![allow(
    clippy::module_name_repetitions,  // Common pattern in Rust
    clippy::missing_errors_doc,        // Will add gradually
    clippy::missing_panics_doc,        // Will add gradually
)]

// Note: `missing_docs` is set to `warn` instead of `deny` because:
// 1. Macro-generated code (e.g., `poise::command`) doesn't include docs
// 2. We want to gradually add documentation rather than block compilation

/// Discord bot interface - commands, handlers, and bot context
pub mod bot;
/// Configuration management for database and application settings
pub mod config;
/// Core business logic - framework-agnostic envelope, transaction, and reporting operations
pub mod core;
/// SeaORM entity definitions for database tables
pub mod entities;
/// Unified error types and result handling
pub mod errors;

#[cfg(test)]
pub mod test_utils;
