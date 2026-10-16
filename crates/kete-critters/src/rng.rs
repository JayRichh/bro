//! The seeded random source, bit-for-bit the stream overlay's: `hashStr`
//! turns a seed string into a u32 and `mulberry32` walks from it. Both are
//! written with wrapping u32 arithmetic so they match JavaScript's
//! `Math.imul` and `>>> 0` exactly, which is what lets the desktop's port
//! replay a Rust scenario from the same seed.

/// The overlay's `hashStr`: a murmur-style mix over the string's UTF-16
/// code units (JavaScript's `charCodeAt`), so a seed hashes the same in
/// both languages even with macrons in it.
pub fn hash_str(s: &str) -> u32 {
    let units: Vec<u16> = s.encode_utf16().collect();
    let mut h: u32 = 1_779_033_703 ^ units.len() as u32;
    for c in units {
        h = (h ^ u32::from(c)).wrapping_mul(3_432_918_353);
        h = h.rotate_left(13);
    }
    h
}

/// mulberry32: a 32-bit state, one f64 in [0, 1) per call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rng {
    state: u32,
}

impl Rng {
    pub fn new(seed: u32) -> Self {
        Rng { state: seed }
    }

    pub fn from_seed(seed: &str) -> Self {
        Rng::new(hash_str(seed))
    }

    /// The next value in [0, 1).
    pub fn float(&mut self) -> f64 {
        self.state = self.state.wrapping_add(0x6D2B_79F5);
        let a = self.state;
        let mut t = (a ^ (a >> 15)).wrapping_mul(1 | a);
        t = t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t)) ^ t;
        f64::from(t ^ (t >> 14)) / 4_294_967_296.0
    }

    /// Uniform in [a, b).
    pub fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.float()
    }

    /// True with probability `p`.
    pub fn chance(&mut self, p: f64) -> bool {
        self.float() < p
    }

    /// An index into a slice of length `n` (n > 0).
    pub fn index(&mut self, n: usize) -> usize {
        ((self.float() * n as f64) as usize).min(n.saturating_sub(1))
    }

    /// -1 or 1, evenly.
    pub fn sign(&mut self) -> f64 {
        if self.chance(0.5) {
            1.0
        } else {
            -1.0
        }
    }

    /// A weighted pick: the index whose cumulative weight first covers a
    /// uniform draw over the total, the overlay's `wpick`. Zero weights are
    /// never picked unless every weight is zero (then index 0).
    pub fn weighted(&mut self, weights: &[f64]) -> usize {
        let total: f64 = weights.iter().sum();
        if total <= 0.0 {
            return 0;
        }
        let mut r = self.float() * total;
        for (i, w) in weights.iter().enumerate() {
            if *w <= 0.0 {
                continue;
            }
            r -= w;
            if r <= 0.0 {
                return i;
            }
        }
        weights.iter().rposition(|w| *w > 0.0).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the JavaScript `hashStr("wevd")` and the first four `mulberry32`
    /// draws from it print (kept here and in the desktop README).
    const JS_HASH_WEVD: u32 = 215_071_857;
    const JS_FIRST_FOUR: [f64; 4] = [
        0.3900833611842245,
        0.8546483409591019,
        0.12886597937904298,
        0.9828977438155562,
    ];

    /// Pinned against the overlay's JavaScript (node, Math.imul): the
    /// desktop port's test asserts the same numbers.
    #[test]
    fn matches_the_javascript_stream() {
        assert_eq!(hash_str("wevd"), JS_HASH_WEVD);
        let mut r = Rng::from_seed("wevd");
        let got: Vec<f64> = (0..4).map(|_| r.float()).collect();
        assert_eq!(got, JS_FIRST_FOUR);
    }

    #[test]
    fn weighted_skips_zero_weights() {
        let mut r = Rng::new(7);
        for _ in 0..200 {
            assert_ne!(r.weighted(&[0.0, 1.0, 0.0, 2.0]), 0);
        }
        assert_eq!(r.weighted(&[0.0, 0.0]), 0);
    }
}
