//! Runtime state machine with explicit transition validation.
//!
//! # States
//!
//! ```text
//!    Created → Initialising → Running ⇄ Suspended
//!                  ↓                          ↓
//!              ShuttingDown ←──────────────────┘
//!                  ↓
//!               Exited
//! ```
//!
//! Invalid transitions are rejected with a [`TransitionError`].

use crate::ErrorCode;
use crate::error::RuntimeError;

/// Describes the current state of the runtime application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RuntimeState {
    Created,
    Initialising,
    Running,
    Suspended,
    ShuttingDown,
    Exited,
    Error,
}

impl RuntimeState {
    pub fn label(&self) -> &'static str {
        match self {
            RuntimeState::Created => "created",
            RuntimeState::Initialising => "initialising",
            RuntimeState::Running => "running",
            RuntimeState::Suspended => "suspended",
            RuntimeState::ShuttingDown => "shutting_down",
            RuntimeState::Exited => "exited",
            RuntimeState::Error => "error",
        }
    }
}

/// A runtime state machine that enforces valid transitions.
pub struct StateMachine {
    current: RuntimeState,
    last_error: Option<String>,
}

impl Default for StateMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl StateMachine {
    pub fn new() -> Self {
        Self {
            current: RuntimeState::Created,
            last_error: None,
        }
    }

    pub fn state(&self) -> RuntimeState {
        self.current
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Attempt a transition. Returns an error if the transition is invalid.
    pub fn transition(&mut self, target: RuntimeState) -> std::result::Result<(), TransitionError> {
        if self.current == target {
            return Ok(());
        }
        let valid = self.is_valid_transition(target);
        if !valid {
            let err = TransitionError {
                from: self.current,
                to: target,
            };
            self.last_error = Some(err.to_string());
            return Err(err);
        }
        self.current = target;
        self.last_error = None;
        Ok(())
    }

    fn is_valid_transition(&self, target: RuntimeState) -> bool {
        use RuntimeState::*;
        matches!(
            (self.current, target),
            (Created, Initialising)
                | (Created, ShuttingDown)
                | (Initialising, Running)
                | (Initialising, ShuttingDown)
                | (Initialising, Error)
                | (Running, Suspended)
                | (Running, ShuttingDown)
                | (Running, Error)
                | (Suspended, Running)
                | (Suspended, ShuttingDown)
                | (Suspended, Error)
                | (ShuttingDown, Exited)
                | (ShuttingDown, Error)
                | (Error, ShuttingDown)
                | (Error, Initialising)
        )
    }
}

/// Error returned when an invalid state transition is attempted.
#[derive(Debug, Clone)]
pub struct TransitionError {
    pub from: RuntimeState,
    pub to: RuntimeState,
}

impl std::fmt::Display for TransitionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Invalid state transition: {} → {}",
            self.from.label(),
            self.to.label()
        )
    }
}

impl From<TransitionError> for RuntimeError {
    fn from(e: TransitionError) -> Self {
        RuntimeError::new(ErrorCode::INVALID_STATE_TRANSITION, e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let sm = StateMachine::new();
        assert_eq!(sm.state(), RuntimeState::Created);
    }

    #[test]
    fn test_valid_transitions_created_to_running() {
        let mut sm = StateMachine::new();
        assert!(sm.transition(RuntimeState::Initialising).is_ok());
        assert!(sm.transition(RuntimeState::Running).is_ok());
        assert_eq!(sm.state(), RuntimeState::Running);
    }

    #[test]
    fn test_valid_transitions_suspend_resume() {
        let mut sm = StateMachine::new();
        sm.transition(RuntimeState::Initialising).ok();
        sm.transition(RuntimeState::Running).ok();
        assert!(sm.transition(RuntimeState::Suspended).is_ok());
        assert!(sm.transition(RuntimeState::Running).is_ok());
    }

    #[test]
    fn test_shutdown_sequence() {
        let mut sm = StateMachine::new();
        sm.transition(RuntimeState::Initialising).ok();
        sm.transition(RuntimeState::Running).ok();
        assert!(sm.transition(RuntimeState::ShuttingDown).is_ok());
        assert!(sm.transition(RuntimeState::Exited).is_ok());
    }

    #[test]
    fn test_shutdown_from_initialising() {
        let mut sm = StateMachine::new();
        assert!(sm.transition(RuntimeState::Initialising).is_ok());
        assert!(sm.transition(RuntimeState::ShuttingDown).is_ok());
    }

    #[test]
    fn test_invalid_created_to_exited() {
        let mut sm = StateMachine::new();
        let result = sm.transition(RuntimeState::Exited);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_running_to_initialising() {
        let mut sm = StateMachine::new();
        sm.transition(RuntimeState::Initialising).ok();
        sm.transition(RuntimeState::Running).ok();
        assert!(sm.transition(RuntimeState::Initialising).is_err());
    }

    #[test]
    fn test_invalid_exited_to_running() {
        let mut sm = StateMachine::new();
        sm.transition(RuntimeState::Initialising).ok();
        sm.transition(RuntimeState::Running).ok();
        sm.transition(RuntimeState::ShuttingDown).ok();
        sm.transition(RuntimeState::Exited).ok();
        assert!(sm.transition(RuntimeState::Running).is_err());
    }

    #[test]
    fn test_error_recovery() {
        let mut sm = StateMachine::new();
        sm.transition(RuntimeState::Initialising).ok();
        sm.transition(RuntimeState::Error).ok();
        // Can recover from error by reinitialising
        assert!(sm.transition(RuntimeState::Initialising).is_ok());
    }

    #[test]
    fn test_transition_error_display() {
        let err = TransitionError {
            from: RuntimeState::Running,
            to: RuntimeState::Created,
        };
        let msg = err.to_string();
        assert!(msg.contains("running"));
        assert!(msg.contains("created"));
    }

    #[test]
    fn test_every_state_has_unique_label() {
        use std::collections::HashSet;
        let states = [
            RuntimeState::Created,
            RuntimeState::Initialising,
            RuntimeState::Running,
            RuntimeState::Suspended,
            RuntimeState::ShuttingDown,
            RuntimeState::Exited,
            RuntimeState::Error,
        ];
        let labels: HashSet<&str> = states.iter().map(|s| s.label()).collect();
        assert_eq!(labels.len(), states.len());
    }

    #[test]
    fn test_last_error_tracked() {
        let mut sm = StateMachine::new();
        assert!(sm.transition(RuntimeState::Exited).is_err());
        assert!(sm.last_error().is_some());
        assert!(sm.last_error().unwrap().contains("created"));
    }

    #[test]
    fn test_idempotent_transition() {
        let mut sm = StateMachine::new();
        assert!(sm.transition(RuntimeState::Created).is_ok());
        assert_eq!(sm.state(), RuntimeState::Created);
    }
}
