//! Test B: Alpha Dead Zone Detection
//!
//! Tests the IBM-verified alpha dead zone where signal cancels.
//! At alpha ≈ 0.8, the parity correlation should approach zero.

use std::f32;

/// Fixed-point type (Q15.16)
type Fixed = i32;
const FIXED_ONE: Fixed = 1 << 16;

fn float_to_fixed(f: f32) -> Fixed {
    (f * FIXED_ONE as f32) as Fixed
}

fn fixed_to_float(f: Fixed) -> f32 {
    f as f32 / FIXED_ONE as f32
}

fn fixed_mul(a: Fixed, b: Fixed) -> Fixed {
    ((a as i64 * b as i64) >> 16) as Fixed
}

/// Ring graph Laplacian (N=16 for test)
fn create_ring_laplacian(n: usize) -> Vec<Vec<Fixed>> {
    let mut l = vec![vec![0 as Fixed; n]; n];
    
    for i in 0..n {
        // Diagonal = degree (2 for ring)
        l[i][i] = 2 * FIXED_ONE;
        
        // Off-diagonal = -1 for neighbors
        let prev = (i + n - 1) % n;
        let next = (i + 1) % n;
        l[i][prev] = -(FIXED_ONE as i32);
        l[i][next] = -(FIXED_ONE as i32);
    }
    
    l
}

/// Compute U = exp(i*alpha*L) for ring graph
/// For small alpha: U ≈ I + i*alpha*L
fn compute_unitary(l: &[Vec<Fixed>], alpha: Fixed) -> Vec<Vec<(Fixed, Fixed)>> {
    let n = l.len();
    let mut u = vec![vec![(0 as Fixed, 0 as Fixed); n]; n];
    
    for i in 0..n {
        for j in 0..n {
            if i == j {
                // Diagonal: 1 + i*alpha*L[i,i]
                let real = FIXED_ONE; // 1
                let imag = fixed_mul(alpha, l[i][j]); // alpha * L[i,i]
                u[i][j] = (real, imag);
            } else {
                // Off-diagonal: i*alpha*L[i,j]
                let real = 0;
                let imag = fixed_mul(alpha, l[i][j]);
                u[i][j] = (real, imag);
            }
        }
    }
    
    u
}

/// Parity operator for ring graph
/// P = product of Z operators on all qubits
/// Simplified: use correlation between opposite nodes
fn compute_parity_correlation(u: &[Vec<(Fixed, Fixed)>]) -> f32 {
    let n = u.len();
    
    // For ring graph, compute correlation between node 0 and node n/2
    let opposite = n / 2;
    
    // Extract the evolved state overlap
    let (re, im) = u[0][opposite];
    
    // Correlation ~ |<0|opposite>|^2
    let mag_sq = (re as f32 / FIXED_ONE as f32).powi(2) + 
                 (im as f32 / FIXED_ONE as f32).powi(2);
    
    mag_sq
}

/// Test B: Alpha Dead Zone Detection
/// 
/// According to IBM verification, alpha ≈ 0.8 causes signal cancellation.
/// We sweep alpha and find where parity correlation is minimized.
#[test]
fn test_alpha_dead_zone() {
    const N: usize = 16; // Ring graph size
    const ALPHA_MIN: f32 = 0.1;
    const ALPHA_MAX: f32 = 2.0;
    const ALPHA_STEPS: usize = 50;
    
    println!("=== Test B: Alpha Dead Zone Detection ===");
    println!("Graph size: N={}", N);
    println!("Alpha range: {} to {}", ALPHA_MIN, ALPHA_MAX);
    
    // Create ring Laplacian
    let l = create_ring_laplacian(N);
    
    // Sweep alpha values
    let mut min_correlation = f32::INFINITY;
    let mut min_alpha = 0.0f32;
    let mut correlations = Vec::new();
    
    for i in 0..=ALPHA_STEPS {
        let alpha_f = ALPHA_MIN + (ALPHA_MAX - ALPHA_MIN) * (i as f32) / (ALPHA_STEPS as f32);
        let alpha = float_to_fixed(alpha_f);
        
        // Compute unitary
        let u = compute_unitary(&l, alpha);
        
        // Compute parity correlation
        let corr = compute_parity_correlation(&u);
        correlations.push((alpha_f, corr));
        
        if corr < min_correlation {
            min_correlation = corr;
            min_alpha = alpha_f;
        }
    }
    
    // Print correlation sweep
    println!("\nAlpha sweep results (selected points):");
    for (alpha, corr) in correlations.iter().step_by(ALPHA_STEPS / 10) {
        println!("  alpha = {:.3}, correlation = {:.6}", alpha, corr);
    }
    
    println!("\nMinimum correlation: {:.6} at alpha = {:.4}", min_correlation, min_alpha);
    
    // Expected dead zone at alpha ≈ 0.8
    let expected_dead_zone = 0.8f32;
    let tolerance = 0.15f32; // Allow ±0.15 deviation
    
    println!("\nExpected dead zone: {} ± {}", expected_dead_zone, tolerance);
    
    // Check if minimum is near expected dead zone
    let alpha_diff = (min_alpha - expected_dead_zone).abs();
    
    if alpha_diff <= tolerance {
        println!("✓ PASS: Dead zone detected at alpha = {:.4} (within tolerance)", min_alpha);
    } else {
        println!("✗ FAIL: Dead zone at alpha = {:.4}, expected near {:.2} (diff = {:.4})", 
                 min_alpha, expected_dead_zone, alpha_diff);
        panic!("Alpha dead zone test failed: dead zone not at expected location");
    }
    
    // Additional check: correlation should drop significantly at dead zone
    let baseline_correlation = correlations[0].1; // First point (alpha=0.1)
    let correlation_drop = baseline_correlation - min_correlation;
    let drop_ratio = correlation_drop / baseline_correlation;
    
    println!("\nCorrelation analysis:");
    println!("  Baseline (alpha=0.1): {:.6}", baseline_correlation);
    println!("  Minimum: {:.6}", min_correlation);
    println!("  Drop ratio: {:.2}%", drop_ratio * 100.0);
    
    if drop_ratio > 0.3 { // At least 30% drop expected
        println!("✓ PASS: Significant correlation drop detected at dead zone");
    } else {
        println!("⚠ WARNING: Correlation drop may be insufficient ({:.1}% < 30%)", 
                 drop_ratio * 100.0);
    }
}

fn main() {
    test_alpha_dead_zone();
    println!("\n=== Test B complete ===");
}
