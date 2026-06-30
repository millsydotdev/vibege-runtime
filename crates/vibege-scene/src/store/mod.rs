//! # Store Platform & Discovery Engine
//!
//! Modular store services for browsing, searching, and downloading games.
//!
//! ## Architecture
//!
//! ```text
//! StoreScene ──→ StoreManager
//!                    │
//!              ┌─────┼─────┬──────┬─────────┐
//!              │     │     │      │         │
//!           Search  Cache  DL    Metadata  Discovery
//!           Engine         Queue Provider  Engine
//! ```
//!
//! All data flows through `StoreManager`, which coordinates caching,
//! fetching, and discovery. The scene only handles input and rendering.

pub mod cache;
pub mod discovery;
pub mod download;
pub mod manager;
pub mod metadata;
pub mod models;
pub mod search;
