//! # Game Runtime — Package & Game Execution Framework
//!
//! Manages the complete lifecycle of VibeGE games from package discovery
//! through to cleanup. Provides deterministic state transitions,
//! comprehensive package validation, and safe session management.

pub mod context;
pub mod error;
pub mod lifecycle;
pub mod orchestrator;
pub mod session;
pub mod state;
pub mod validator;
