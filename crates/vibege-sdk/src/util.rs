use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use mlua::{Lua, Table};

/// A fast, deterministic 64-bit PRNG using xorshift64*.
struct SeededRng {
    state: u64,
}

fn lock_rng(rng: &Arc<Mutex<SeededRng>>) -> std::sync::MutexGuard<'_, SeededRng> {
    rng.lock().unwrap_or_else(|e| {
        tracing::warn!("RNG mutex poisoned — recovering inner data");
        e.into_inner()
    })
}

impl SeededRng {
    fn new() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64
            ^ (std::process::id() as u64).wrapping_shl(32);
        Self { state: seed }
    }

    fn from_seed(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Generate the next f64 in [0.0, 1.0).
    fn next_f64(&mut self) -> f64 {
        self.state ^= self.state >> 12;
        self.state ^= self.state << 25;
        self.state ^= self.state >> 27;
        let x = self.state.wrapping_mul(0x2545F4914F6CDD1Du64);
        // Convert to [0.0, 1.0) using only the 53 most significant mantissa bits
        (x >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// Generate the next f64 in [min, max].
    fn range_f64(&mut self, min: f64, max: f64) -> f64 {
        min + self.next_f64() * (max - min)
    }

    /// Generate the next i64 in [min, max] (inclusive).
    fn range_i64(&mut self, min: i64, max: i64) -> i64 {
        let range = (max - min).unsigned_abs().saturating_add(1);
        min + (self.next_f64() * range as f64) as i64
    }
}

pub fn register_util_api(lua: &Lua) -> Result<Table, String> {
    let util_table = lua.create_table().map_err(|e| e.to_string())?;

    let rng = Arc::new(Mutex::new(SeededRng::new()));

    // Logging
    let log_fn = lua
        .create_function(|_, message: String| {
            tracing::info!(target: "game", "{message}");
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    util_table.set("log", log_fn).map_err(|e| e.to_string())?;

    // Random float in [min, max]
    let rng_clone = Arc::clone(&rng);
    let random_fn = lua
        .create_function(move |_, (min, max): (f64, f64)| {
            let mut rng = lock_rng(&rng_clone);
            Ok(rng.range_f64(min, max))
        })
        .map_err(|e| e.to_string())?;
    util_table
        .set("random", random_fn)
        .map_err(|e| e.to_string())?;

    // Random integer in [min, max] (inclusive)
    let rng_clone = Arc::clone(&rng);
    let random_int_fn = lua
        .create_function(move |_, (min, max): (i64, i64)| {
            let mut rng = lock_rng(&rng_clone);
            Ok(rng.range_i64(min, max))
        })
        .map_err(|e| e.to_string())?;
    util_table
        .set("random_int", random_int_fn)
        .map_err(|e| e.to_string())?;

    // Set seed for deterministic mode
    let rng_clone = Arc::clone(&rng);
    let set_seed_fn = lua
        .create_function(move |_, seed: u64| {
            let mut rng = lock_rng(&rng_clone);
            *rng = SeededRng::from_seed(seed);
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    util_table
        .set("set_seed", set_seed_fn)
        .map_err(|e| e.to_string())?;

    // Clamp utility
    let clamp_fn = lua
        .create_function(|_, (value, min, max): (f64, f64, f64)| Ok(value.clamp(min, max)))
        .map_err(|e| e.to_string())?;
    util_table
        .set("clamp", clamp_fn)
        .map_err(|e| e.to_string())?;

    // Lerp utility
    let lerp_fn = lua
        .create_function(|_, (a, b, t): (f64, f64, f64)| Ok(a + (b - a) * t.clamp(0.0, 1.0)))
        .map_err(|e| e.to_string())?;
    util_table.set("lerp", lerp_fn).map_err(|e| e.to_string())?;

    Ok(util_table)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rng_deterministic() {
        let mut a = SeededRng::from_seed(42);
        let mut b = SeededRng::from_seed(42);
        for _ in 0..100 {
            assert_eq!(a.next_f64(), b.next_f64());
        }
    }

    #[test]
    fn test_rng_different_seeds_different_sequences() {
        let mut a = SeededRng::from_seed(42);
        let mut b = SeededRng::from_seed(99);
        let mut different = false;
        for _ in 0..100 {
            if a.next_f64() != b.next_f64() {
                different = true;
                break;
            }
        }
        assert!(different);
    }

    #[test]
    fn test_rng_range_f64() {
        let mut rng = SeededRng::from_seed(42);
        for _ in 0..1000 {
            let val = rng.range_f64(5.0, 10.0);
            assert!(val >= 5.0);
            assert!(val < 10.0);
        }
    }

    #[test]
    fn test_rng_range_i64() {
        let mut rng = SeededRng::from_seed(42);
        for _ in 0..1000 {
            let val = rng.range_i64(1, 6);
            assert!(val >= 1);
            assert!(val <= 6);
        }
    }

    #[test]
    fn test_rng_value_in_expected_range() {
        let mut rng = SeededRng::from_seed(42);
        let val = rng.next_f64();
        assert!(val >= 0.0);
        assert!(val < 1.0);
    }

    #[test]
    fn test_rng_from_seed_resets_state() {
        let mut rng = SeededRng::from_seed(42);
        let first = rng.next_f64();
        rng = SeededRng::from_seed(42);
        let second = rng.next_f64();
        assert_eq!(first, second);
    }

    #[test]
    fn test_rng_not_all_zeros() {
        let mut rng = SeededRng::new();
        let mut all_zero = true;
        for _ in 0..100 {
            if rng.next_f64() > 0.0 {
                all_zero = false;
                break;
            }
        }
        assert!(!all_zero, "PRNG should produce non-zero values");
    }
}
