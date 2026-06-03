//! Rolling Shannon Entropy Circuit Breaker.
//!
//! Zero-day jailbreaks and structured data leaks (partial SSNs, routing keys,
//! system-prompt override variables) often exhibit anomalous byte-level
//! clustering — either *too ordered* (low entropy, e.g. `4111-1111-1111-1111`)
//! or *too random* (high entropy, e.g. base64-encoded injection strings).
//!
//! This module performs a **sliding-window Shannon entropy** calculation over
//! the prompt byte stream in O(n) time.  If a window falls outside the
//! calibrated safe band, the circuit breaker flips instantly.
//!
//! The hot path is allocation-free: a 256-slot byte-frequency table lives on
//! the stack, and the rolling update only touches the falling/rising byte pair.

/// Default window size in bytes.  Large enough to let a random 32-byte slice
/// reach ~5.0 bits (32 distinct symbols), while still catching short
/// structured fragments when they dominate a window.
const DEFAULT_WINDOW_BYTES: usize = 32;

/// Minimum prompt length before entropy scanning is meaningful.
/// Inputs shorter than this return Safe — too few symbols for a reliable
/// frequency distribution.
const MIN_LENGTH_FOR_SCAN: usize = 16;

/// Lower bound: below this entropy (bits per byte) the window looks
/// suspiciously structured / repetitive.  Catches long runs of repeated
/// characters, sequential digits, and other highly ordered patterns.
const ENTROPY_FLOOR: f64 = 1.2;

/// Upper bound: above this entropy the window looks like random / encoded
/// injection material.  A 32-byte window of random printable ASCII can reach
/// ~5.0 bits; normal English hovers around 3.7–4.2, so 4.8 catches encoded
/// strings without false-positiving on prose.
const ENTROPY_CEILING: f64 = 4.8;

/// Precomputed byte-frequency LUT — avoids per-window heap allocation.
const LUT_SIZE: usize = 256;

/// Result of an entropy scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntropyVerdict {
    /// Entropy within the safe band — no anomaly detected.
    Safe,
    /// Window entropy below floor — possible structured PII / credential leak.
    LowEntropy,
    /// Window entropy above ceiling — possible encoded injection / obfuscation.
    HighEntropy,
}

/// Scan a byte slice using a sliding-window Shannon entropy calculation.
///
/// Returns the first anomalous window encountered, or [`EntropyVerdict::Safe`]
/// if all windows are within the calibrated band.  Inputs shorter than
/// [`MIN_LENGTH_FOR_SCAN`] are skipped and always return Safe.
///
/// Time complexity: O(n) where n = bytes.len().
/// Space complexity: O(1) — only a 256-element stack array.
pub fn scan(bytes: &[u8]) -> EntropyVerdict {
    if bytes.len() < MIN_LENGTH_FOR_SCAN {
        return EntropyVerdict::Safe;
    }

    let win = DEFAULT_WINDOW_BYTES.min(bytes.len());
    let mut freq = [0u32; LUT_SIZE];

    // Seed the first window.
    for b in &bytes[0..win] {
        freq[*b as usize] += 1;
    }

    if let Some(v) = verdict_for_window(&freq, win) {
        return v;
    }

    // Slide the window: drop the left byte, add the new right byte.
    for i in win..bytes.len() {
        let out_byte = bytes[i - win] as usize;
        let in_byte = bytes[i] as usize;
        freq[out_byte] -= 1;
        freq[in_byte] += 1;

        if let Some(v) = verdict_for_window(&freq, win) {
            return v;
        }
    }

    EntropyVerdict::Safe
}

/// Compute Shannon entropy H = -Σ p_i · log₂(p_i) for the current window and
/// compare against the floor / ceiling thresholds.
#[inline(always)]
fn verdict_for_window(freq: &[u32; LUT_SIZE], window_len: usize) -> Option<EntropyVerdict> {
    let mut entropy = 0.0_f64;
    let w = window_len as f64;

    for &count in freq.iter() {
        if count == 0 {
            continue;
        }
        let p = count as f64 / w;
        entropy -= p * p.log2();
    }

    if entropy < ENTROPY_FLOOR {
        return Some(EntropyVerdict::LowEntropy);
    }
    if entropy > ENTROPY_CEILING {
        return Some(EntropyVerdict::HighEntropy);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_text_is_safe() {
        let text = b"Hello, this is a normal business inquiry about quarterly earnings.";
        assert_eq!(scan(text), EntropyVerdict::Safe);
    }

    #[test]
    fn test_repetitive_data_is_low_entropy() {
        // A run of identical characters has zero entropy — the floor trips.
        let text = b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        assert_eq!(scan(text), EntropyVerdict::LowEntropy);
    }

    #[test]
    fn test_random_injection_is_high_entropy() {
        // Long random mixed-alphanumeric string → entropy spikes above ceiling.
        let text = b"aB9xZqW2mN7vL4kP0jR8sT3uY5wE1iO6cD";
        assert_eq!(scan(text), EntropyVerdict::HighEntropy);
    }

    #[test]
    fn test_empty_is_safe() {
        assert_eq!(scan(b""), EntropyVerdict::Safe);
    }

    #[test]
    fn test_short_is_safe() {
        // Below MIN_LENGTH_FOR_SCAN → skipped.
        assert_eq!(scan(b"hi"), EntropyVerdict::Safe);
    }
}
