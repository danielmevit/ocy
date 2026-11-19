//! Ocy's break engine, settings, statistics, exercises, schedule, and persistence.
//!
//! Pure Rust port of `src/Ocy.Core` with zero UI or OS dependencies.

#![forbid(unsafe_code)]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]
#![warn(missing_docs)]

pub mod engine;
pub mod exercises;
pub mod persistence;
pub mod schedule;
pub mod settings;
pub mod statistics;
pub mod time;
