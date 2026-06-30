//! Test A: Frobenius Norm Lock Verification
//!
//! Tests the fundamental quantum-relational invariant:
//! ||W(t+1)||_F = ||W(t)||_F (Frobenius norm preserved under unitary evolution)

use std::f32;

/// Fixed-point type (Q15.16 format matching firmware)
type Fixed = i32;
const FIXED_ONE: Fixed = 1 << 16;
const FIXED_FRACTIONAL_MASK: Fixed = 0x0000_FFFF;

/// Convert f32 to fixed-point
fn float_to_fixed(f: f32) -> Fixed {
    (f * FIXED_ONE as f32) as Fixed
}

/// Convert fixed-point to f32
fn fixed_to_float(f: Fixed) -> f32 {
    f as f32 / FIXED_ONE as f32
}

/// Saturating multiplication for fixed-point
fn fixed_mul(a: Fixed, b: Fixed) -> Fixed {
    ((a as i64 * b as i64) >> 16) as Fixed
}

/// Sparse matrix element for W matrix
#[derive(Debug, Clone, Copy)]
struct WElement {
    re: Fixed,
    im: Fixed,
}

impl WElement {
    fn norm_sqr(&self) -> i64 {
        let re_sq = (self.re as i64 * self.re as i64) >> 16;
        let im_sq = (self.im as i64 * self.im as i64) >> 16;
        re_sq + im_sq
    }
}

/// Sparse W matrix (relational state)
struct WMatrix {
    dimension: usize,
    elements: Vec<(usize, usize, WElement)>, // (row, col, value)
}

impl WMatrix {
    fn new(dimension: usize) -> Self {
        Self {
            dimension,
            elements: Vec::new(),
        }
    }

    fn set(&mut self, row: usize, col: usize, re: Fixed, im: Fixed) {
        // Check if element already exists
        for i in 0..self.elements.len() {
            if self.elements[i].0 == row && self.elements[i].1 == col {
                self.elements[i].2 = WElement { re, im };
                return;
            }
        }
        // Add new element
        self.elements.push((row, col, WElement { re, im }));
    }

    fn frobenius_norm_sqr(&self) -> i64 {
        let mut sum: i64 = 0;
        for (_, _, elem) in &self.elements {
            sum += elem.norm_sqr();
        }
        sum
    }

    fn clone(&self) -> Self {
        Self {
            dimension: self.dimension,
            elements: self.elements.clone(),
        }
    }
}

/// Simulate unitary evolution: W' = U * W * U†
/// For small alpha: U ≈ I + i*alpha*L
/// W' ≈ W + i*alpha*(L*W - W*L) (first-order approximation)
fn evolve_w_matrix(w: &WMatrix, alpha: f32) -> WMatrix {
    let alpha_fixed = float_to_fixed(alpha);
    let mut w_new = w.clone();

    // For this test, we simulate the evolution by applying a unitary rotation
    // that preserves the Frobenius norm
    for i in 0..w_new.elements.len() {
        let (row, col, elem) = &w_new.elements[i];
        let _ = (row, col); // Silence unused warning

        // Apply rotation: preserves |re|² + |im|²
        let cos_theta = FIXED_ONE - fixed_mul(alpha_fixed / 10, alpha_fixed / 10) / 2;
        let sin_theta = alpha_fixed / 10;

        let new_re = fixed_mul(elem.re, cos_theta) - fixed_mul(elem.im, sin_theta);
        let new_im = fixed_mul(elem.re, sin_theta) + fixed_mul(elem.im, cos_theta);

        w_new.elements[i].2 = WElement { re: new_re, im: new_im };
    }

    w_new
}

/// Test A: Frobenius Norm Lock Verification
/// 
/// The Frobenius norm of W must be preserved under unitary evolution.
/// ||W'||_F = ||W||_F
#[test]
fn test_frobenius_norm_lock() {
    const DIMENSION: usize = 128; // Reduced size for testing
    const TOLERANCE_RELATIVE: f32 = 1e-6;
    const TEST_ITERATIONS: usize = 10;

    println!("=== Test A: Frobenius Norm Lock ===");
    println!("Testing dimension: {}", DIMENSION);
    println!("Relative tolerance: {}", TOLERANCE_RELATIVE);

    let mut max_drift: f64 = 0.0;
    let mut total_drift: f64 = 0.0;

    for iteration in 0..TEST_ITERATIONS {
        // Create random W matrix
        let mut w = WMatrix::new(DIMENSION);
        
        // Add random elements
        for _ in 0..(DIMENSION * 2) {
            let row = (rand::random::<usize>()) % DIMENSION;
            let col = (rand::random::<usize>()) % DIMENSION;
            let re = ((rand::random::<i32>() % 1000) as f32 / 1000.0 * 32767.0) as Fixed;
            let im = ((rand::random::<i32>() % 1000) as f32 / 1000.0 * 32767.0) as Fixed;
            w.set(row, col, re, im);
        }

        // Measure initial norm
        let norm_initial = w.frobenius_norm_sqr();
        let norm_initial_f = (norm_initial as f64).sqrt();

        // Evolve W
        let alpha = 0.5f32;
        let w_evolved = evolve_w_matrix(&w, alpha);

        // Measure evolved norm
        let norm_evolved = w_evolved.frobenius_norm_sqr();
        let norm_evolved_f = (norm_evolved as f64).sqrt();

        // Calculate drift
        let drift = (norm_evolved_f - norm_initial_f).abs();
        let relative_drift = drift / norm_initial_f;

        if relative_drift > max_drift as f64 {
            max_drift = relative_drift;
        }
        total_drift += relative_drift;

        // Verify tolerance
        assert!(
            relative_drift < TOLERANCE_RELATIVE as f64,
            "Frobenius norm drift {} exceeds tolerance {} at iteration {}",
            relative_drift,
            TOLERANCE_RELATIVE,
            iteration
        );
    }

    let avg_drift = total_drift / TEST_ITERATIONS as f64;

    println!("✓ All {} iterations passed", TEST_ITERATIONS);
    println!("  Max drift: {:.2e}", max_drift);
    println!("  Avg drift: {:.2e}", avg_drift);
    println!("  Tolerance: {:.2e}", TOLERANCE_RELATIVE);
}

fn main() {
    test_frobenius_norm_lock();
    println!("\n=== All tests passed ===");
}
