//! # Library Platform & Game Management
//!
//! Manages installed games, collections, play history, updates, and integrity.
//!
//! ## Architecture
//!
//! ```text
//! LibraryScene ──→ LibraryManager
//!                      │
//!              ┌───────┼──────┬────────┬──────────┐
//!              │       │      │        │          │
//!          Registry  Coll.  History  Updates  Integrity
//!          (disk)   (auto) (track)  (check)  (verify)
//! ```

pub mod collections;
pub mod history;
pub mod integrity;
pub mod manager;
pub mod models;
pub mod registry;
pub mod search;
pub mod updates;
