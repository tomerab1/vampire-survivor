//! Tiny xorshift RNG so gameplay randomness works identically on native and wasm.

use bevy::prelude::*;
use std::f32::consts::TAU;

#[derive(Resource)]
pub struct Rng(u64);

impl Rng {
    pub fn seeded(seed: u64) -> Self {
        Self(seed.max(1))
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// Uniform in [0, 1).
    pub fn f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    pub fn range(&mut self, min: f32, max: f32) -> f32 {
        min + (max - min) * self.f32()
    }

    pub fn chance(&mut self, probability: f32) -> bool {
        self.f32() < probability
    }

    pub fn angle(&mut self) -> f32 {
        self.f32() * TAU
    }

    pub fn index(&mut self, len: usize) -> usize {
        (self.next_u64() % len as u64) as usize
    }
}
