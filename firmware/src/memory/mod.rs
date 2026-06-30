//! Memory Management Subsystem
//!
//! Provides three memory tiers for the Arcturus quantum-relational compute system:
//! 
//! 1. **Time-Slicing Memory** (`time_slicer.rs`): 1000 time-step banks for reversible computation
//! 2. **Eigenbasis Memory** (`eigen_manager.rs`): Laplacian eigenvalue-based data storage
//! 3. **Edge Cache** (`edge_cache.rs`): L1 cache using bismuthene edge state conductance

pub mod time_slicer;
pub mod eigen_manager;
pub mod edge_cache;

use core::ops::{Add, Mul, Sub};

/// Fixed-point scalar type for matrix elements (Q15.16 format)
/// Provides deterministic arithmetic without FPU dependency
pub type Fixed = i32;

/// Fixed-point representation of 1.0
pub const FIXED_ONE: Fixed = 1 << 16;

/// Convert f32 to fixed-point
#[inline]
pub fn float_to_fixed(f: f32) -> Fixed {
    (f * FIXED_ONE as f32) as Fixed
}

/// Convert fixed-point to f32
#[inline]
pub fn fixed_to_float(f: Fixed) -> f32 {
    f as f32 / FIXED_ONE as f32
}

/// Multiply two fixed-point numbers
#[inline]
pub fn fixed_mul(a: Fixed, b: Fixed) -> Fixed {
    ((a as i64 * b as i64) >> 16) as Fixed
}

/// Divide two fixed-point numbers
#[inline]
pub fn fixed_div(a: Fixed, b: Fixed) -> Fixed {
    if b == 0 {
        0
    } else {
        (((a as i64) << 16) / b as i64) as Fixed
    }
}

/// Square root of fixed-point number (integer approximation)
#[inline]
pub fn fixed_sqrt(x: Fixed) -> Fixed {
    if x <= 0 {
        return 0;
    }
    
    let mut x_i64 = (x as i64) << 16;
    let mut res = x_i64;
    let mut bit = 1i64 << 62;
    
    // Find highest power of 4 <= x
    while bit > x_i64 {
        bit >>= 2;
    }
    
    while bit != 0 {
        if x_i64 >= res + bit {
            x_i64 -= res + bit;
            res = (res >> 1) + bit;
        } else {
            res >>= 1;
        }
        bit >>= 2;
    }
    
    (res as i32).min(FIXED_ONE * 2) // Clamp to reasonable range
}

/// Complex number in fixed-point (a + bi)
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Complex {
    pub re: Fixed,
    pub im: Fixed,
}

impl Complex {
    /// Create a new complex number
    pub fn new(re: Fixed, im: Fixed) -> Self {
        Self { re, im }
    }

    /// Create from float values
    pub fn from_float(re: f32, im: f32) -> Self {
        Self {
            re: float_to_fixed(re),
            im: float_to_fixed(im),
        }
    }

    /// Get magnitude squared
    pub fn norm_sqr(&self) -> Fixed {
        fixed_mul(self.re, self.re) + fixed_mul(self.im, self.im)
    }

    /// Complex conjugate
    pub fn conj(&self) -> Self {
        Self {
            re: self.re,
            im: -self.im,
        }
    }

    /// Complex multiplication
    pub fn mul(&self, other: &Self) -> Self {
        Self {
            re: fixed_mul(self.re, other.re) - fixed_mul(self.im, other.im),
            im: fixed_mul(self.re, other.im) + fixed_mul(self.im, other.re),
        }
    }

    /// Complex addition
    pub fn add(&self, other: &Self) -> Self {
        Self {
            re: self.re.saturating_add(other.re),
            im: self.im.saturating_add(other.im),
        }
    }

    /// Scale by real factor
    pub fn scale(&self, factor: Fixed) -> Self {
        Self {
            re: fixed_mul(self.re, factor),
            im: fixed_mul(self.im, factor),
        }
    }
}

/// Memory bank identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u16)]
pub enum MemoryBank {
    /// Time-slicing banks (0-999)
    TimeBank(u16),
    /// Edge cache (local node storage)
    EdgeCache = 1000,
    /// Eigenbasis storage
    EigenStorage = 1001,
    /// System configuration
    Config = 1002,
    /// Undefined
    #[default]
    Undefined = 255,
}

impl MemoryBank {
    /// Convert to numeric bank ID
    pub fn to_id(self) -> u16 {
        match self {
            MemoryBank::TimeBank(t) => t.min(999),
            MemoryBank::EdgeCache => 1000,
            MemoryBank::EigenStorage => 1001,
            MemoryBank::Config => 1002,
            MemoryBank::Undefined => 255,
        }
    }

    /// Create from numeric bank ID
    pub fn from_id(id: u16) -> Self {
        match id {
            0..=999 => MemoryBank::TimeBank(id),
            1000 => MemoryBank::EdgeCache,
            1001 => MemoryBank::EigenStorage,
            1002 => MemoryBank::Config,
            _ => MemoryBank::Undefined,
        }
    }
}
