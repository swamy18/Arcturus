//! Block Universe Synchronization
//!
//! Implements GHZ-like entanglement-based time synchronization across
//! the 10,000-node grid. This creates a coherent "block universe" where
//! past, present, and future states are quantum-correlated.

pub mod block_universe;

use super::memory::{Fixed, FIXED_ONE};

/// Synchronization states for block universe
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TimeState {
    Past = 0,
    Present = 1,
    Future = 2,
    Superposition = 3, // Mixed state
}

/// GHZ-like correlation strength (0-1)
pub type CorrelationStrength = Fixed;

/// Maximum correlation value
pub const MAX_CORRELATION: Fixed = FIXED_ONE;

/// Minimum correlation for "entanglement"
pub const ENTANGLEMENT_THRESHOLD: Fixed = 0x0000B333; // ~0.7
