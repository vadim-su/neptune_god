//! Rolling hash of world state for cheap equality checks in tests and saves.

/// Fingerprint combined incrementally during world digest passes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct WorldDigest(pub u64);

impl WorldDigest {
    pub fn combine_u64(&mut self, value: u64) {
        self.0 = self.0.wrapping_mul(1_099_511_628_211).wrapping_add(value);
    }
}
