/// Deterministic lifecycle stages for a game session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeState {
    /// Package has been discovered on disk or in registry.
    Discovered,
    /// Package has been mounted (ZIP extracted, entries enumerated).
    Mounted,
    /// All validations passed (manifest, integrity, compatibility).
    Validated,
    /// Lua VM created, SDK registered, init() called.
    Initialized,
    /// Game is actively running and receiving updates/renders.
    Running,
    /// Game state has been captured, engine can be suspended.
    Suspended,
    /// Game is temporarily paused (UI overlay visible).
    Paused,
    /// Game has been stopped, resources released.
    Stopped,
    /// All resources freed, ready for reuse.
    Unloaded,
    /// Final cleanup complete.
    CleanedUp,
}

impl RuntimeState {
    /// Returns the set of valid transitions from this state.
    pub fn valid_transitions(&self) -> &[RuntimeState] {
        match self {
            RuntimeState::Discovered => &[RuntimeState::Mounted],
            RuntimeState::Mounted => &[RuntimeState::Validated, RuntimeState::Unloaded],
            RuntimeState::Validated => &[RuntimeState::Initialized, RuntimeState::Unloaded],
            RuntimeState::Initialized => &[RuntimeState::Running, RuntimeState::Stopped],
            RuntimeState::Running => &[
                RuntimeState::Paused,
                RuntimeState::Suspended,
                RuntimeState::Stopped,
            ],
            RuntimeState::Suspended => &[RuntimeState::Running, RuntimeState::Stopped],
            RuntimeState::Paused => &[RuntimeState::Running, RuntimeState::Stopped],
            RuntimeState::Stopped => &[RuntimeState::Unloaded],
            RuntimeState::Unloaded => &[RuntimeState::CleanedUp, RuntimeState::Discovered],
            RuntimeState::CleanedUp => &[],
        }
    }

    /// Check if a transition to `next` is valid.
    pub fn can_transition_to(&self, next: &RuntimeState) -> bool {
        self.valid_transitions().contains(next)
    }

    /// Returns a human-readable label for the state.
    pub fn label(&self) -> &'static str {
        match self {
            RuntimeState::Discovered => "discovered",
            RuntimeState::Mounted => "mounted",
            RuntimeState::Validated => "validated",
            RuntimeState::Initialized => "initialized",
            RuntimeState::Running => "running",
            RuntimeState::Suspended => "suspended",
            RuntimeState::Paused => "paused",
            RuntimeState::Stopped => "stopped",
            RuntimeState::Unloaded => "unloaded",
            RuntimeState::CleanedUp => "cleaned_up",
        }
    }
}

impl std::fmt::Display for RuntimeState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        assert_eq!(RuntimeState::Discovered, RuntimeState::Discovered);
    }

    #[test]
    fn test_valid_transitions_from_discovered() {
        let s = RuntimeState::Discovered;
        assert!(s.can_transition_to(&RuntimeState::Mounted));
        assert!(!s.can_transition_to(&RuntimeState::Running));
        assert!(!s.can_transition_to(&RuntimeState::CleanedUp));
    }

    #[test]
    fn test_valid_transitions_from_mounted() {
        let s = RuntimeState::Mounted;
        assert!(s.can_transition_to(&RuntimeState::Validated));
        assert!(s.can_transition_to(&RuntimeState::Unloaded));
        assert!(!s.can_transition_to(&RuntimeState::Running));
    }

    #[test]
    fn test_valid_transitions_from_validated() {
        let s = RuntimeState::Validated;
        assert!(s.can_transition_to(&RuntimeState::Initialized));
        assert!(s.can_transition_to(&RuntimeState::Unloaded));
        assert!(!s.can_transition_to(&RuntimeState::Suspended));
    }

    #[test]
    fn test_valid_transitions_from_running() {
        let s = RuntimeState::Running;
        assert!(s.can_transition_to(&RuntimeState::Paused));
        assert!(s.can_transition_to(&RuntimeState::Suspended));
        assert!(s.can_transition_to(&RuntimeState::Stopped));
        assert!(!s.can_transition_to(&RuntimeState::Mounted));
    }

    #[test]
    fn test_valid_transitions_from_suspended() {
        let s = RuntimeState::Suspended;
        assert!(s.can_transition_to(&RuntimeState::Running));
        assert!(s.can_transition_to(&RuntimeState::Stopped));
        assert!(!s.can_transition_to(&RuntimeState::Initialized));
    }

    #[test]
    fn test_valid_transitions_from_stopped() {
        let s = RuntimeState::Stopped;
        assert!(s.can_transition_to(&RuntimeState::Unloaded));
        assert!(!s.can_transition_to(&RuntimeState::Running));
    }

    #[test]
    fn test_valid_transitions_from_unloaded() {
        let s = RuntimeState::Unloaded;
        assert!(s.can_transition_to(&RuntimeState::CleanedUp));
        assert!(s.can_transition_to(&RuntimeState::Discovered));
    }

    #[test]
    fn test_valid_transitions_from_cleaned_up() {
        let s = RuntimeState::CleanedUp;
        assert!(s.valid_transitions().is_empty());
    }

    #[test]
    fn test_every_state_has_unique_label() {
        let states = [
            RuntimeState::Discovered,
            RuntimeState::Mounted,
            RuntimeState::Validated,
            RuntimeState::Initialized,
            RuntimeState::Running,
            RuntimeState::Suspended,
            RuntimeState::Paused,
            RuntimeState::Stopped,
            RuntimeState::Unloaded,
            RuntimeState::CleanedUp,
        ];
        let mut labels = std::collections::HashSet::new();
        for s in &states {
            assert!(labels.insert(s.label()), "Duplicate label: {}", s.label());
        }
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", RuntimeState::Discovered), "discovered");
        assert_eq!(format!("{}", RuntimeState::Running), "running");
        assert_eq!(format!("{}", RuntimeState::CleanedUp), "cleaned_up");
    }
}
