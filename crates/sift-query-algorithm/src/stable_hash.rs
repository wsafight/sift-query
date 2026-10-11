pub const FNV1A_64_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV1A_64_PRIME: u64 = 0x0000_0100_0000_01b3;

/// A small deterministic FNV-1a 64-bit accumulator.
///
/// This is intended for stable fingerprints and cache keys, not for
/// cryptographic authentication or hash-flooding protection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StableHasher64 {
    state: u64,
}

impl Default for StableHasher64 {
    fn default() -> Self {
        Self::new(FNV1A_64_OFFSET_BASIS)
    }
}

impl StableHasher64 {
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub fn write(&mut self, bytes: impl AsRef<[u8]>) {
        for byte in bytes.as_ref() {
            self.state ^= u64::from(*byte);
            self.state = self.state.wrapping_mul(FNV1A_64_PRIME);
        }
    }

    pub fn write_delimited(&mut self, bytes: impl AsRef<[u8]>) {
        self.write(bytes);
        self.write([0xff]);
    }

    #[must_use]
    pub const fn finish(self) -> u64 {
        self.state
    }
}
