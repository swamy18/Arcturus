//! Eigenbasis Memory Manager
//!
//! Maps data to Laplacian eigenvalues for spectral storage.
//! The Laplacian L = D - A has eigenvalues λ₀ ≤ λ₁ ≤ ... ≤ λₙ₋₁
//! where λ₀ = 0 (for connected graph).
//!
//! Data is encoded in eigenvalue perturbations:
//! - Data bit 0: λ_k stays at base value
//! - Data bit 1: λ_k is perturbed by δ

use super::{Complex, Fixed, float_to_fixed};
use heapless::Vec;

/// Number of Laplacian eigenvalues (matches number of nodes: 10,000)
pub const NUM_EIGENVALUES: usize = 10000;

/// Number of eigenmodes used for data storage (subset for practical storage)
pub const STORAGE_MODES: usize = 1024;

/// Eigenvalue perturbation magnitude for data encoding (δ)
/// This value is chosen to be detectable but not disrupt graph structure
pub const EIGEN_PERTURBATION_DELTA: Fixed = 0x00001000; // Small fixed-point value

/// Minimum eigenvalue index used for storage (skip λ₀ = 0)
pub const MIN_EIGEN_INDEX: usize = 1;

/// Maximum eigenvalue index for storage
pub const MAX_EIGEN_INDEX: usize = MIN_EIGEN_INDEX + STORAGE_MODES;

/// Base eigenvalue for a 100x100 grid Laplacian
/// Approximate formula: λ_k ≈ 4 - 2*cos(π*i/100) - 2*cos(π*j/100)
/// where k = i*100 + j
pub fn base_eigenvalue(index: usize) -> Fixed {
    if index == 0 {
        return 0; // λ₀ = 0 for connected graph
    }

    // Convert index to 2D grid coordinates
    let i = index / 100;
    let j = index % 100;

    // Calculate approximate eigenvalue using discrete cosine
    // λ_{i,j} = 4 - 2*cos(π*i/100) - 2*cos(π*j/100)
    let pi = 3.14159265359f32;
    let cos_i = libm::cosf(pi * i as f32 / 100.0);
    let cos_j = libm::cosf(pi * j as f32 / 100.0);

    let eigenval = 4.0 - 2.0 * cos_i - 2.0 * cos_j;
    
    float_to_fixed(eigenval)
}

/// Data encoding in eigenvalue perturbations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EigenDataValue {
    /// No perturbation (logical 0)
    Zero = 0,
    /// Positive perturbation (logical 1)
    One = 1,
    /// Undefined/invalid
    Invalid = 0xFF,
}

impl From<u8> for EigenDataValue {
    fn from(value: u8) -> Self {
        match value {
            0 => EigenDataValue::Zero,
            1 => EigenDataValue::One,
            _ => EigenDataValue::Invalid,
        }
    }
}

impl Default for EigenDataValue {
    fn default() -> Self {
        Self::Zero
    }
}

/// Single eigenmode storage entry
#[derive(Debug, Clone, Copy, Default)]
pub struct EigenModeStorage {
    /// Eigenvalue index (k)
    pub index: u16,
    /// Base eigenvalue λ_k (without perturbation)
    pub base_value: Fixed,
    /// Perturbation data (0 or 1)
    pub data: EigenDataValue,
}

impl EigenModeStorage {
    /// Create a new eigenmode storage entry
    pub fn new(index: u16, data: EigenDataValue) -> Self {
        let base = base_eigenvalue(index as usize);
        Self {
            index,
            base_value: base,
            data,
        }
    }

    /// Get the perturbed eigenvalue (base + δ * data)
    pub fn perturbed_value(&self) -> Fixed {
        match self.data {
            EigenDataValue::One => self.base_value.saturating_add(EIGEN_PERTURBATION_DELTA),
            _ => self.base_value,
        }
    }

    /// Check if measured value matches expected (within tolerance)
    pub fn verify(&self, measured: Fixed, tolerance: Fixed) -> bool {
        let expected = self.perturbed_value();
        let diff = if measured > expected {
            measured - expected
        } else {
            expected - measured
        };
        diff <= tolerance
    }
}

/// Eigenbasis memory manager
pub struct EigenManager {
    /// Storage for eigenmode data (1024 modes)
    pub modes: Vec<EigenModeStorage, STORAGE_MODES>,
    /// Current mode for streaming access
    current_mode: usize,
}

impl EigenManager {
    /// Create a new eigenbasis manager
    pub fn new() -> Self {
        Self {
            modes: Vec::new(),
            current_mode: 0,
        }
    }

    /// Initialize with all storage modes (set to zero)
    pub fn initialize_zeros(&mut self) -> Result<(), ()> {
        self.modes.clear();
        for i in MIN_EIGEN_INDEX..MAX_EIGEN_INDEX {
            let mode = EigenModeStorage::new(i as u16, EigenDataValue::Zero);
            self.modes.push(mode).map_err(|_| ())?;
        }
        Ok(())
    }

    /// Write data bit to specific mode
    pub fn write_mode(&mut self, mode_index: usize, data: EigenDataValue) -> Result<(), ()> {
        if mode_index >= STORAGE_MODES {
            return Err(());
        }

        // Ensure mode exists
        while self.modes.len() <= mode_index {
            let idx = MIN_EIGEN_INDEX + mode_index;
            let mode = EigenModeStorage::new(idx as u16, EigenDataValue::Zero);
            self.modes.push(mode).map_err(|_| ())?;
        }

        // Update the data value
        self.modes[mode_index].data = data;
        Ok(())
    }

    /// Read data bit from specific mode
    pub fn read_mode(&self, mode_index: usize) -> EigenDataValue {
        self.modes.get(mode_index).map(|m| m.data).unwrap_or(EigenDataValue::Invalid)
    }

    /// Get mode storage entry
    pub fn get_mode(&self, mode_index: usize) -> Option<&EigenModeStorage> {
        self.modes.get(mode_index)
    }

    /// Compute spectral transform of current W matrix
    /// Projects W onto eigenbasis: W_hat = Q^T · W · Q
    /// where Q is the eigenvector matrix
    pub fn project_to_spectral(&self, _w_matrix: &super::time_slicer::SparseWMatrix<100>)
        -> Result<Vec<Complex, STORAGE_MODES>, ()> {
        // This is a placeholder for the actual spectral projection
        // In practice, this requires the full eigenvector matrix
        // For now, return zeroed spectral coefficients
        let mut coeffs = Vec::new();
        for _ in 0..self.modes.len() {
            coeffs.push(Complex::default()).map_err(|_| ())?;
        }
        Ok(coeffs)
    }

    /// Reconstruct W matrix from spectral coefficients
    /// W = Q · W_hat · Q^T
    pub fn reconstruct_from_spectral(&self, _coeffs: &[Complex])
        -> Result<super::time_slicer::SparseWMatrix<100>, ()> {
        // Placeholder for inverse spectral transform
        Ok(super::time_slicer::SparseWMatrix::new(100))
    }

    /// Get current spectral energy distribution
    /// Returns the power in each eigenmode
    pub fn spectral_energy(&self) -> Result<Vec<i64, STORAGE_MODES>, ()> {
        let mut energy = Vec::new();
        for mode in &self.modes {
            // Energy ~ |λ_k|^2
            let e = (mode.perturbed_value() as i64).saturating_pow(2);
            energy.push(e).map_err(|_| ())?;
        }
        Ok(energy)
    }

    /// Streaming mode iterator
    pub fn iter_modes(&self) -> impl Iterator<Item = &EigenModeStorage> {
        self.modes.iter()
    }

    /// Number of configured modes
    pub fn num_modes(&self) -> usize {
        self.modes.len()
    }

    /// Reset all modes to zero
    pub fn clear(&mut self) {
        for mode in &mut self.modes {
            mode.data = EigenDataValue::Zero;
        }
    }
}

impl Default for EigenManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base_eigenvalue() {
        // λ₀ should be 0
        let lambda_0 = base_eigenvalue(0);
        assert_eq!(lambda_0, 0);

        // λ₁ should be small positive
        let lambda_1 = base_eigenvalue(1);
        assert!(lambda_1 > 0);
    }

    #[test]
    fn test_eigen_mode_storage() {
        let mode = EigenModeStorage::new(1, EigenDataValue::One);
        
        assert_eq!(mode.index, 1);
        assert_eq!(mode.data, EigenDataValue::One);
        
        // Perturbed value should be base + delta
        let perturbed = mode.perturbed_value();
        assert!(perturbed > mode.base_value);
        assert_eq!(perturbed - mode.base_value, EIGEN_PERTURBATION_DELTA);
    }

    #[test]
    fn test_eigen_manager() {
        let mut manager = EigenManager::new();
        
        // Initialize with zeros
        manager.initialize_zeros().unwrap();
        assert!(manager.num_modes() > 0);
        
        // Write and read data
        manager.write_mode(0, EigenDataValue::One).unwrap();
        let data = manager.read_mode(0);
        assert_eq!(data, EigenDataValue::One);
        
        // Clear
        manager.clear();
        let data_after_clear = manager.read_mode(0);
        assert_eq!(data_after_clear, EigenDataValue::Zero);
    }

    #[test]
    fn test_phase_conversions() {
        use super::super::{float_to_fixed, fixed_to_float};
        
        let original = 1.5f32;
        let fixed = float_to_fixed(original);
        let recovered = fixed_to_float(fixed);
        
        // Allow small quantization error
        assert!((recovered - original).abs() < 0.001);
    }
}
