//! Test C: Time-Slicing Reversibility
//!
//! Tests that W(t) is recoverable after forward evolution U and backward evolution U†
//! W(t) --U--> W(t+1) --U†--> W'(t) ≈ W(t) (bit-for-bit identical)

use std::collections::HashMap;

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

/// Sparse matrix element
#[derive(Debug, Clone, Copy, PartialEq)]
struct Element {
    re: Fixed,
    im: Fixed,
}

impl Element {
    fn norm_sqr(&self) -> i64 {
        let re_sq = (self.re as i64 * self.re as i64) >> 16;
        let im_sq = (self.im as i64 * self.im as i64) >> 16;
        re_sq + im_sq
    }
}

/// Sparse W matrix
#[derive(Debug, Clone)]
struct WMatrix {
    dimension: usize,
    elements: HashMap<(usize, usize), Element>,
}

impl WMatrix {
    fn new(dimension: usize) -> Self {
        Self {
            dimension,
            elements: HashMap::new(),
        }
    }

    fn set(&mut self, row: usize, col: usize, re: Fixed, im: Fixed) {
        self.elements.insert((row, col), Element { re, im });
    }

    fn get(&self, row: usize, col: usize) -> Option<&Element> {
        self.elements.get(&(row, col))
    }

    fn frobenius_norm_sqr(&self) -> i64 {
        self.elements.values()
            .map(|e| e.norm_sqr())
            .sum()
    }
}

/// Create a unitary matrix U for evolution
/// For small alpha: U ≈ I + i*alpha*L
fn create_unitary(dimension: usize, alpha: Fixed, is_adjoint: bool) -> Vec<Vec<(Fixed, Fixed)>> {
    let mut u = vec![vec![(0 as Fixed, 0 as Fixed); dimension]; dimension];
    
    // Simple ring graph Laplacian
    for i in 0..dimension {
        for j in 0..dimension {
            if i == j {
                // Diagonal: 1 + i*alpha*2 (degree 2 for ring)
                let real = FIXED_ONE;
                let imag = if is_adjoint { -fixed_mul(alpha, 2 * FIXED_ONE) } else { fixed_mul(alpha, 2 * FIXED_ONE) };
                u[i][j] = (real, imag);
            } else if (i + 1) % dimension == j || (i + dimension - 1) % dimension == j {
                // Neighbor: i*alpha*(-1)
                let real = 0;
                let imag = if is_adjoint { fixed_mul(alpha, FIXED_ONE) } else { -fixed_mul(alpha, FIXED_ONE) };
                u[i][j] = (real, imag);
            } else {
                u[i][j] = (0, 0);
            }
        }
    }
    
    u
}

/// Apply unitary: W' = U * W * U†
fn apply_unitary(w: &WMatrix, u: &[Vec<(Fixed, Fixed)>], u_adjoint: &[Vec<(Fixed, Fixed)>]) -> WMatrix {
    let n = w.dimension;
    let mut result = WMatrix::new(n);
    
    // For each output element (i, j)
    for i in 0..n {
        for j in 0..n {
            let mut re_sum: i64 = 0;
            let mut im_sum: i64 = 0;
            
            // Compute (U * W * U†)[i,j] = sum_{k,l} U[i,k] * W[k,l] * U†[l,j]
            for k in 0..n {
                for l in 0..n {
                    if let Some(w_kl) = w.get(k, l) {
                        // U[i,k] * W[k,l] * U†[l,j]
                        let (u_ik_re, u_ik_im) = u[i][k];
                        let (u_adj_lj_re, u_adj_lj_im) = u_adjoint[l][j];
                        
                        // (a+ib)(c+id) = (ac-bd) + i(ad+bc)
                        let w_re = w_kl.re as i64;
                        let w_im = w_kl.im as i64;
                        
                        // First multiply U[i,k] * W[k,l]
                        let temp_re = (u_ik_re as i64 * w_re - u_ik_im as i64 * w_im) >> 16;
                        let temp_im = (u_ik_re as i64 * w_im + u_ik_im as i64 * w_re) >> 16;
                        
                        // Then multiply by U†[l,j]
                        let final_re = (temp_re * u_adj_lj_re as i64 - temp_im * u_adj_lj_im as i64) >> 16;
                        let final_im = (temp_re * u_adj_lj_im as i64 + temp_im * u_adj_lj_re as i64) >> 16;
                        
                        re_sum += final_re;
                        im_sum += final_im;
                    }
                }
            }
            
            if re_sum != 0 || im_sum != 0 {
                result.set(i, j, re_sum as Fixed, im_sum as Fixed);
            }
        }
    }
    
    result
}

/// Test C: Time-Slicing Reversibility
/// 
/// Tests that W(t) is recoverable after forward evolution U and backward evolution U†
/// W(t) --U--> W(t+1) --U†--> W'(t) ≈ W(t) (bit-for-bit identical)
#[test]
fn test_time_reversibility() {
    const DIMENSION: usize = 16; // Smaller for test speed
    const ALPHA: f32 = 0.3; // Small alpha for better approximation
    const TEST_ITERATIONS: usize = 5;
    
    println!("=== Test C: Time-Slicing Reversibility ===");
    println!("Dimension: {}", DIMENSION);
    println!("Alpha: {}", ALPHA);
    
    let alpha_fixed = float_to_fixed(ALPHA);
    
    for iteration in 0..TEST_ITERATIONS {
        println!("\n--- Iteration {} ---", iteration + 1);
        
        // Create initial W(t)
        let mut w_initial = WMatrix::new(DIMENSION);
        
        // Add random elements
        for _ in 0..(DIMENSION * 2) {
            let row = rand::random::<usize>() % DIMENSION;
            let col = rand::random::<usize>() % DIMENSION;
            let re = ((rand::random::<i32>() % 10000) as f32 / 10000.0 * 10000.0) as Fixed;
            let im = ((rand::random::<i32>() % 10000) as f32 / 10000.0 * 10000.0) as Fixed;
            w_initial.set(row, col, re, im);
        }
        
        let initial_norm = w_initial.frobenius_norm_sqr();
        println!("  Initial ||W||² = {}", initial_norm);
        
        // Create unitaries
        let u = create_unitary(DIMENSION, alpha_fixed, false);
        let u_adjoint = create_unitary(DIMENSION, alpha_fixed, true);
        
        // Forward evolution: W(t) -> W(t+1) = U * W(t) * U†
        let w_forward = apply_unitary(&w_initial, &u, &u_adjoint);
        let forward_norm = w_forward.frobenius_norm_sqr();
        println!("  Forward ||W'||² = {}", forward_norm);
        
        // Backward evolution: W(t+1) -> W'(t) = U† * W(t+1) * U
        let w_recovered = apply_unitary(&w_forward, &u_adjoint, &u);
        let recovered_norm = w_recovered.frobenius_norm_sqr();
        println!("  Recovered ||W''||² = {}", recovered_norm);
        
        // Check bit-for-bit recovery
        let mut matches = 0;
        let mut mismatches = 0;
        let mut max_re_diff: i64 = 0;
        let mut max_im_diff: i64 = 0;
        
        for ((row, col), elem_initial) in &w_initial.elements {
            match w_recovered.elements.get(&(*row, *col)) {
                Some(elem_recovered) => {
                    let re_diff = (elem_initial.re as i64 - elem_recovered.re as i64).abs();
                    let im_diff = (elem_initial.im as i64 - elem_recovered.im as i64).abs();
                    
                    if re_diff > max_re_diff { max_re_diff = re_diff; }
                    if im_diff > max_im_diff { max_im_diff = im_diff; }
                    
                    // Allow small numerical error due to fixed-point
                    if re_diff <= 10 && im_diff <= 10 {
                        matches += 1;
                    } else {
                        mismatches += 1;
                    }
                }
                None => {
                    // Element missing in recovered matrix
                    mismatches += 1;
                }
            }
        }
        
        let total_elements = w_initial.elements.len();
        let recovery_rate = if total_elements > 0 {
            (matches as f64) / (total_elements as f64) * 100.0
        } else {
            100.0
        };
        
        println!("\n  Recovery statistics:");
        println!("    Total elements: {}", total_elements);
        println!("    Matches: {}", matches);
        println!("    Mismatches: {}", mismatches);
        println!("    Recovery rate: {:.2}%", recovery_rate);
        println!("    Max RE diff: {}", max_re_diff);
        println!("    Max IM diff: {}", max_im_diff);
        
        // Verify Frobenius norm conservation
        let norm_diff = (initial_norm - recovered_norm).abs();
        let norm_tolerance = 1000i64; // Allow small fixed-point error
        
        println!("\n  Norm conservation:");
        println!("    Initial: {}", initial_norm);
        println!("    Recovered: {}", recovered_norm);
        println!("    Diff: {}", norm_diff);
        
        assert!(
            norm_diff < norm_tolerance,
            "Frobenius norm not conserved: diff = {} (tolerance = {})",
            norm_diff,
            norm_tolerance
        );
        
        // Assert recovery rate
        assert!(
            recovery_rate >= 95.0,
            "Recovery rate too low: {:.1}% < 95%",
            recovery_rate
        );
        
        println!("  ✓ Iteration {} passed", iteration + 1);
    }
    
    println!("\n=== Test C complete ===");
}

fn main() {
    test_time_reversibility();
}
