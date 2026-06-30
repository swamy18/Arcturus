//! Quantum-Relational Compute Engine
//!
//! Core computation modules for the Arcturus QRE system:
//! 
//! - `laplacian`: Graph Laplacian construction (L = D - A)
//! - `evolution`: Unitary evolution (U = exp(iαL)) and W matrix updates
//!
//! Mathematical basis:
//! - L: Graph Laplacian (10000×10000 sparse)
//! - U: Unitary evolution operator (10000×10000 sparse)
//! - W(t): Relational state matrix (10000×10000 sparse)
//! - W(t+1) = U · W(t) · U^T (preserves Frobenius norm)

pub mod laplacian;
pub mod evolution;

use super::memory::{Complex, Fixed, float_to_fixed, fixed_to_float, fixed_mul, fixed_div, fixed_sqrt};

/// Maximum matrix dimension (100x100 grid = 10,000 nodes)
pub const MAX_DIMENSION: usize = 10000;

/// Grid size (100x100)
pub const GRID_SIZE: usize = 100;

/// Number of nearest neighbors (4-connected grid)
pub const NUM_NEAREST_NEIGHBORS: usize = 4;

/// Number of long-range edges
pub const NUM_LONG_RANGE_EDGES: usize = 100;

/// Alpha parameter range (0.0 to 2.0 mapped to 0-65535)
/// Alpha ≈ 0.8 is the "dead zone" where signal cancels (IBM verified)
pub const ALPHA_DEAD_ZONE: Fixed = 0x0000CCCD; // ~0.8 in fixed-point

/// Default alpha for evolution (avoid dead zone)
pub const DEFAULT_ALPHA: Fixed = 0x00002666; // 0.15 in fixed-point

/// Matrix element index type
pub type MatrixIndex = u16;

/// Sparse matrix element for compute operations
#[derive(Debug, Clone, Copy, Default)]
pub struct SparseElement {
    /// Row index
    pub row: MatrixIndex,
    /// Column index
    pub col: MatrixIndex,
    /// Real component value
    pub re: Fixed,
    /// Imaginary component value
    pub im: Fixed,
}

impl SparseElement {
    /// Create a new sparse element
    pub fn new(row: MatrixIndex, col: MatrixIndex, re: Fixed, im: Fixed) -> Self {
        Self { row, col, re, im }
    }

    /// Create from real value only
    pub fn real(row: MatrixIndex, col: MatrixIndex, re: Fixed) -> Self {
        Self { row, col, re, im: 0 }
    }

    /// Create from complex value
    pub fn complex(row: MatrixIndex, col: MatrixIndex, c: &Complex) -> Self {
        Self {
            row,
            col,
            re: c.re,
            im: c.im,
        }
    }

    /// Get as complex number
    pub fn to_complex(&self) -> Complex {
        Complex::new(self.re, self.im)
    }

    /// Conjugate
    pub fn conj(&self) -> Self {
        Self {
            row: self.row,
            col: self.col,
            re: self.re,
            im: -self.im,
        }
    }

    /// Check if element is on diagonal
    pub fn is_diagonal(&self) -> bool {
        self.row == self.col
    }

    /// Get magnitude squared
    pub fn norm_sqr(&self) -> Fixed {
        let re_sq = ((self.re as i64 * self.re as i64) >> 16) as Fixed;
        let im_sq = ((self.im as i64 * self.im as i64) >> 16) as Fixed;
        re_sq.saturating_add(im_sq)
    }
}

/// Sparse matrix representation for computation
/// Stores only non-zero elements in coordinate format
#[derive(Debug, Clone)]
pub struct SparseMatrix<const MAX_ELEM: usize> {
    /// Matrix dimension (N for N×N matrix)
    pub dimension: MatrixIndex,
    /// Non-zero elements
    pub elements: heapless::Vec<SparseElement, MAX_ELEM>,
    /// Sorted flag (for optimization)
    pub sorted: bool,
}

impl<const MAX_ELEM: usize> SparseMatrix<MAX_ELEM> {
    /// Create a new empty sparse matrix
    pub fn new(dimension: MatrixIndex) -> Self {
        Self {
            dimension,
            elements: heapless::Vec::new(),
            sorted: true,
        }
    }

    /// Get number of stored elements
    pub fn len(&self) -> usize {
        self.elements.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    /// Clear all elements
    pub fn clear(&mut self) {
        self.elements.clear();
        self.sorted = true;
    }

    /// Insert or update an element
    pub fn set(&mut self, row: MatrixIndex, col: MatrixIndex, re: Fixed, im: Fixed) -> Result<(), ()> {
        // Check bounds
        if row >= self.dimension || col >= self.dimension {
            return Err(());
        }

        // Check if element already exists
        for i in 0..self.elements.len() {
            if self.elements[i].row == row && self.elements[i].col == col {
                // Update existing
                self.elements[i].re = re;
                self.elements[i].im = im;
                return Ok(());
            }
        }

        // Insert new element
        let elem = SparseElement::new(row, col, re, im);
        self.elements.push(elem).map_err(|_| ())?;
        self.sorted = false;

        Ok(())
    }

    /// Get an element (returns zero if not found)
    pub fn get(&self, row: MatrixIndex, col: MatrixIndex) -> SparseElement {
        if row >= self.dimension || col >= self.dimension {
            return SparseElement::new(0, 0, 0, 0);
        }

        for elem in &self.elements {
            if elem.row == row && elem.col == col {
                return *elem;
            }
        }

        SparseElement::new(row, col, 0, 0)
    }

    /// Get reference to element if it exists
    pub fn get_ref(&self, row: MatrixIndex, col: MatrixIndex) -> Option<&SparseElement> {
        if row >= self.dimension || col >= self.dimension {
            return None;
        }

        self.elements.iter().find(|e| e.row == row && e.col == col)
    }

    /// Sort elements by (row, col) for optimized access
    pub fn sort(&mut self) {
        if self.sorted {
            return;
        }

        // Simple insertion sort for heapless Vec
        for i in 1..self.elements.len() {
            let key = self.elements[i];
            let mut j = i;
            
            while j > 0 {
                let prev = &self.elements[j - 1];
                let key_before = key.row < prev.row || 
                    (key.row == prev.row && key.col < prev.col);
                
                if !key_before {
                    break;
                }
                
                self.elements[j] = self.elements[j - 1];
                j -= 1;
            }
            
            self.elements[j] = key;
        }

        self.sorted = true;
    }

    /// Iterate over elements in a specific row
    pub fn row_iter(&self, row: MatrixIndex) -> impl Iterator<Item = &SparseElement> {
        self.elements.iter().filter(move |e| e.row == row)
    }

    /// Iterate over diagonal elements
    pub fn diagonal_iter(&self) -> impl Iterator<Item = &SparseElement> {
        self.elements.iter().filter(|e| e.is_diagonal())
    }

    /// Compute Frobenius norm squared
    pub fn frobenius_norm_sqr(&self) -> i64 {
        let mut sum: i64 = 0;
        for elem in &self.elements {
            let norm_sq = elem.norm_sqr() as i64;
            sum = sum.saturating_add(norm_sq);
        }
        sum
    }

    /// Compute the Frobenius norm as a floating-point value.
    pub fn frobenius_norm(&self) -> f64 {
        (self.frobenius_norm_sqr() as f64).sqrt()
    }

    /// Return the transpose of the sparse matrix.
    pub fn transpose(&self) -> Self {
        let mut transposed = Self::new(self.dimension);

        for elem in &self.elements {
            let _ = transposed.set(elem.col, elem.row, elem.re, elem.im);
        }

        transposed.sorted = self.sorted;
        transposed
    }

    /// Sparse-sparse matrix multiplication.
    ///
    /// The output keeps the same compile-time storage bound as `self`.
    pub fn sparse_mul<const RHS_MAX: usize>(
        &self,
        other: &SparseMatrix<RHS_MAX>,
    ) -> Self {
        assert_eq!(
            self.dimension, other.dimension,
            "matrix dimensions must match for multiplication"
        );

        let mut result = Self::new(self.dimension);

        for left in &self.elements {
            for right in other.elements.iter().filter(|e| e.row == left.col) {
                let product = left.to_complex().mul(&right.to_complex());
                let current = result.get(left.row, right.col).to_complex();
                let updated = current.add(&product);
                let _ = result.set(left.row, right.col, updated.re, updated.im);
            }
        }

        result
    }

    /// Check if matrix is Hermitian (symmetric for real case)
    pub fn is_hermitian(&self) -> bool {
        // For each element, check if conjugate transpose exists
        for i in 0..self.elements.len() {
            let elem = &self.elements[i];
            let conj_elem = elem.conj();
            
            // Check if (col, row) has conjugate value
            let found = self.elements.iter().any(|e| {
                e.row == elem.col && e.col == elem.row &&
                e.re == conj_elem.re && e.im == conj_elem.im
            });
            
            if !found {
                return false;
            }
        }
        true
    }

    /// Perform matrix-vector multiplication: y = A * x
    /// Returns number of non-zero contributions
    pub fn matvec(&self, x: &[Complex], y: &mut [Complex]) -> usize {
        assert!(x.len() >= self.dimension as usize);
        assert!(y.len() >= self.dimension as usize);

        // Initialize output to zero
        for yi in y.iter_mut().take(self.dimension as usize) {
            *yi = Complex::default();
        }

        let mut nnz_contributions = 0;

        // For each matrix element, contribute to result
        for elem in &self.elements {
            let i = elem.row as usize;
            let j = elem.col as usize;
            let a_ij = elem.to_complex();
            let x_j = x[j];

            // y[i] += A[i,j] * x[j]
            let product = a_ij.mul(&x_j);
            y[i] = y[i].add(&product);

            nnz_contributions += 1;
        }

        nnz_contributions
    }

    /// Convert to dense matrix format (for small matrices or debugging).
    /// Panics if the matrix would exceed the fixed 4096-element scratch buffer.
    pub fn to_dense(&self) -> heapless::Vec<Complex, 4096> {
        let mut dense = heapless::Vec::<Complex, 4096>::new();
        let size = self.dimension as usize;
        let total_elems = size
            .checked_mul(size)
            .expect("dense matrix size overflow");

        assert!(
            total_elems <= 4096,
            "to_dense only supports matrices up to 64x64; got {}x{}",
            size,
            size
        );

        // Initialize with zeros
        for _ in 0..total_elems {
            let _ = dense.push(Complex::default());
        }

        // Fill in non-zero elements
        for elem in &self.elements {
            let idx = (elem.row as usize) * size + (elem.col as usize);
            if idx < dense.len() {
                dense[idx] = elem.to_complex();
            }
        }

        dense
    }
}

/// Matrix type aliases for convenience
pub type LaplacianMatrix<const MAX_ELEM: usize> = SparseMatrix<MAX_ELEM>;
pub type UnitaryMatrixType<const MAX_ELEM: usize> = SparseMatrix<MAX_ELEM>;
pub type WMatrixType<const MAX_ELEM: usize> = SparseMatrix<MAX_ELEM>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::edge_cache::{
        EdgeCache, EdgeCacheEntry, LEVEL_0_MAX, LEVEL_1_MAX, LEVEL_1_MIN,
    };
    use crate::memory::eigen_manager::base_eigenvalue;

    #[test]
    fn test_sparse_matrix_basic() {
        let mut mat = SparseMatrix::<100>::new(10);

        // Set some elements
        mat.set(0, 0, 1000, 0).unwrap();
        mat.set(5, 5, 2000, 0).unwrap();
        mat.set(9, 9, 3000, 0).unwrap();

        assert_eq!(mat.len(), 3);

        // Retrieve
        let elem = mat.get(5, 5);
        assert_eq!(elem.re, 2000);

        // Non-existent
        let zero = mat.get(1, 1);
        assert_eq!(zero.re, 0);
    }

    #[test]
    fn test_frobenius_norm() {
        let mut mat = SparseMatrix::<100>::new(10);

        // Identity matrix with ones on diagonal
        for i in 0..5 {
            mat.set(i, i, 1000, 0).unwrap(); // 1000 in fixed-point
        }

        // Frobenius norm squared should be derived from the actual stored entries.
        let norm_sq = mat.frobenius_norm_sqr();
        let expected: i64 = mat
            .elements
            .iter()
            .map(|elem| elem.norm_sqr() as i64)
            .sum();

        assert_eq!(norm_sq, expected);
    }

    #[test]
    fn test_edge_cache_entry() {
        let mut entry = EdgeCacheEntry::new(42);
        
        assert_eq!(entry.node_id, 42);
        assert!(!entry.valid);

        // Update from conductance (level 1)
        entry.update_from_conductance(5000, 0);
        
        assert!(entry.valid);
        assert_eq!(entry.data, 1);
        assert_eq!(entry.last_conductance, 5000);

        // Target conductance for data=1
        let target = entry.target_conductance();
        assert!(target >= LEVEL_1_MIN && target <= LEVEL_1_MAX);
    }

    #[test]
    fn test_conductance_levels() {
        // Test conductance to data conversion
        assert_eq!(EdgeCacheEntry::conductance_to_data(1000), 0);
        assert_eq!(EdgeCacheEntry::conductance_to_data(5000), 1);
        assert_eq!(EdgeCacheEntry::conductance_to_data(9000), 2);
        assert_eq!(EdgeCacheEntry::conductance_to_data(13000), 3);

        // Test data to conductance conversion
        assert!(EdgeCacheEntry::data_to_conductance(0) <= LEVEL_0_MAX);
        assert!(EdgeCacheEntry::data_to_conductance(1) >= LEVEL_1_MIN);
        assert!(EdgeCacheEntry::data_to_conductance(1) <= LEVEL_1_MAX);
    }

    #[test]
    fn test_edge_cache() {
        let mut cache = EdgeCache::<64>::new();

        // Write some values
        cache.write(0, 1).unwrap();
        cache.write(10, 2).unwrap();
        cache.write(20, 3).unwrap();

        // Read back
        assert_eq!(cache.read(0), Some(1));
        assert_eq!(cache.read(10), Some(2));
        assert_eq!(cache.read(20), Some(3));

        // Cache miss
        assert_eq!(cache.read(5), None);

        // Check stats
        let stats = cache.stats();
        assert!(stats.hits >= 3);
        assert!(stats.misses >= 1);
    }

    #[test]
    fn test_base_eigenvalue() {
        // λ₀ should be 0
        let lambda_0 = base_eigenvalue(0);
        assert_eq!(lambda_0, 0);

        // λ₁ should be small and positive
        let lambda_1 = base_eigenvalue(1);
        assert!(lambda_1 > 0);

        // Larger indices should have larger eigenvalues
        let lambda_100 = base_eigenvalue(100);
        let lambda_200 = base_eigenvalue(200);
        assert!(lambda_200 >= lambda_100);
    }

    #[test]
    fn test_sparse_matrix_sort() {
        let mut mat = SparseMatrix::<100>::new(10);

        // Add elements out of order
        mat.set(5, 5, 100, 0).unwrap();
        mat.set(0, 0, 200, 0).unwrap();
        mat.set(9, 9, 300, 0).unwrap();
        mat.set(2, 3, 400, 0).unwrap();

        assert!(!mat.sorted);

        // Sort
        mat.sort();

        assert!(mat.sorted);

        // Elements should be in row-major order
        let elements: Vec<_> = mat.elements.iter().collect();
        assert_eq!(elements[0].row, 0);
        assert_eq!(elements[1].row, 2);
        assert_eq!(elements[2].row, 5);
        assert_eq!(elements[3].row, 9);
    }
}
