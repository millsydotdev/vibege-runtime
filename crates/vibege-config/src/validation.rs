/// Trait for configuration validation and sanitisation.
///
/// Every config section implements this trait so that invalid values can be
/// detected (validate) and automatically corrected (sanitize) before use.
pub trait Validate {
    /// Validate the current configuration, returning a list of error messages.
    /// Returns `Ok(())` if the config is valid.
    fn validate(&self) -> Result<(), Vec<String>>;

    /// Auto-correct invalid values by clamping to valid ranges or resetting
    /// to defaults. This should never fail — it always produces a valid config.
    fn sanitize(&mut self);
}
