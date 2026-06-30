//! Time-Slicing Memory Manager
//!
//! Implements reversible time evolution with 1000 time-step banks.
//! Each bank stores a snapshot of the relational state W(t).
//!
//! Key operations:
//! - Forward evolution: W(t+1) = U · W(t) · Uᵀ
//! - Backward evolution: W(t-1) = Uᵀ · W(t) · U
//! - Time-step banking: Store/retrieve W matrices

extern crate alloc;

use alloc::vec::Vec as StdVec;
use crate::hal::spi::{psram_enable, psram_read, psram_reset, psram_write};
use super::{Complex, Fixed, float_to_fixed};
use heapless::Vec;

/// Number of time-step banks
pub const NUM_TIME_BANKS: usize = 1000;

/// Time bank index type (0-999)
pub type TimeBankIndex = u16;

/// Maximum nodes we can store in local SRAM per bank
/// With 512KB SRAM: 512KB / 4 bytes per Fixed = ~128K elements
/// For 10K nodes, we store W matrix in compressed/formatted representation
pub const LOCAL_MATRIX_ELEMENTS: usize = 65536; // 256KB for matrix data

/// W matrix element (single entry in relational state matrix)
#[derive(Debug, Clone, Copy, Default)]
pub struct WElement {
    /// Real component (fixed-point)
    pub re: Fixed,
    /// Imaginary component (fixed-point)
    pub im: Fixed,
}

impl WElement {
    /// Create a new W matrix element
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

    /// Convert to Complex number
    pub fn to_complex(&self) -> Complex {
        Complex::new(self.re, self.im)
    }

    /// Create from Complex number
    pub fn from_complex(c: &Complex) -> Self {
        Self {
            re: c.re,
            im: c.im,
        }
    }

    /// Compute squared magnitude
    pub fn norm_sqr(&self) -> Fixed {
        let re_sq = ((self.re as i64 * self.re as i64) >> 16) as Fixed;
        let im_sq = ((self.im as i64 * self.im as i64) >> 16) as Fixed;
        re_sq.saturating_add(im_sq)
    }

    /// Add another element
    pub fn add(&self, other: &Self) -> Self {
        Self {
            re: self.re.saturating_add(other.re),
            im: self.im.saturating_add(other.im),
        }
    }

    /// Multiply by scalar (fixed-point)
    pub fn scale(&self, factor: Fixed) -> Self {
        let re_scaled = ((self.re as i64 * factor as i64) >> 16) as Fixed;
        let im_scaled = ((self.im as i64 * factor as i64) >> 16) as Fixed;
        Self {
            re: re_scaled,
            im: im_scaled,
        }
    }
}

/// Compressed W matrix representation for time-bank storage
/// Stores only non-zero elements with coordinates (sparse format)
#[derive(Debug, Clone)]
pub struct SparseWMatrix<const MAX_ELEMENTS: usize> {
    /// Number of rows/columns (always 10000 for full grid)
    pub dimension: u16,
    /// Non-zero elements as (row, col, value) tuples
    pub elements: Vec<(u16, u16, WElement), MAX_ELEMENTS>,
}

impl<const MAX_ELEMENTS: usize> SparseWMatrix<MAX_ELEMENTS> {
    /// Create a new empty sparse matrix
    pub fn new(dimension: u16) -> Self {
        Self {
            dimension,
            elements: Vec::new(),
        }
    }

    /// Insert or update an element
    pub fn set(&mut self, row: u16, col: u16, value: WElement) -> Result<(), ()> {
        if row >= self.dimension || col >= self.dimension {
            return Err(());
        }

        // Check if element already exists
        for i in 0..self.elements.len() {
            if self.elements[i].0 == row && self.elements[i].1 == col {
                self.elements[i].2 = value;
                return Ok(());
            }
        }

        // Insert new element
        self.elements.push((row, col, value)).map_err(|_| ())
    }

    /// Get an element (returns zero if not found)
    pub fn get(&self, row: u16, col: u16) -> WElement {
        if row >= self.dimension || col >= self.dimension {
            return WElement::default();
        }

        for (r, c, v) in &self.elements {
            if *r == row && *c == col {
                return *v;
            }
        }

        WElement::default()
    }

    /// Serialize matrix into a compact little-endian byte buffer.
    ///
    /// Format:
    /// - `dimension: u16`
    /// - `num_elements: u32`
    /// - repeated element records:
    ///   - `row: u16`
    ///   - `col: u16`
    ///   - `re: i32`
    ///   - `im: i32`
    pub fn serialize(&self) -> Result<StdVec<u8>, ()> {
        let element_size: usize = 2 + 2 + 4 + 4;
        let header_size: usize = 2 + 4;
        let total_size = header_size
            .checked_add(self.elements.len().checked_mul(element_size).ok_or(())?)
            .ok_or(())?;

        let mut out = StdVec::with_capacity(total_size);
        out.extend_from_slice(&self.dimension.to_le_bytes());
        out.extend_from_slice(&(self.elements.len() as u32).to_le_bytes());

        for (row, col, value) in &self.elements {
            out.extend_from_slice(&row.to_le_bytes());
            out.extend_from_slice(&col.to_le_bytes());
            out.extend_from_slice(&value.re.to_le_bytes());
            out.extend_from_slice(&value.im.to_le_bytes());
        }

        Ok(out)
    }

    /// Deserialize a matrix from bytes produced by `serialize`.
    pub fn deserialize(data: &[u8]) -> Result<Self, ()> {
        fn read_u16(data: &[u8], offset: &mut usize) -> Result<u16, ()> {
            let end = offset.checked_add(2).ok_or(())?;
            let bytes = data.get(*offset..end).ok_or(())?;
            *offset = end;
            Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
        }

        fn read_u32(data: &[u8], offset: &mut usize) -> Result<u32, ()> {
            let end = offset.checked_add(4).ok_or(())?;
            let bytes = data.get(*offset..end).ok_or(())?;
            *offset = end;
            Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        }

        fn read_i32(data: &[u8], offset: &mut usize) -> Result<i32, ()> {
            let end = offset.checked_add(4).ok_or(())?;
            let bytes = data.get(*offset..end).ok_or(())?;
            *offset = end;
            Ok(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        }

        let mut offset = 0usize;
        let dimension = read_u16(data, &mut offset)?;
        let num_elements = read_u32(data, &mut offset)? as usize;

        let mut matrix = Self::new(dimension);
        for _ in 0..num_elements {
            let row = read_u16(data, &mut offset)?;
            let col = read_u16(data, &mut offset)?;
            let re = read_i32(data, &mut offset)?;
            let im = read_i32(data, &mut offset)?;
            matrix.set(row, col, WElement::new(re, im))?;
        }

        Ok(matrix)
    }

    /// Compute Frobenius norm squared: ||W||_F^2 = sum(|w_ij|^2)
    pub fn frobenius_norm_sqr(&self) -> i64 {
        let mut sum: i64 = 0;
        for (_, _, elem) in &self.elements {
            let norm_sq = elem.norm_sqr() as i64;
            sum = sum.saturating_add(norm_sq);
        }
        sum
    }

    /// Clear all elements
    pub fn clear(&mut self) {
        self.elements.clear();
    }

    /// Get number of stored elements
    pub fn len(&self) -> usize {
        self.elements.len()
    }

    /// Check if matrix is empty
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }
}

/// Time-slicing memory manager
pub struct TimeSlicer<const MAX_ELEMENTS: usize> {
    /// Current time step (0-999)
    current_time: TimeBankIndex,
    /// Current W matrix state (active)
    current_w: SparseWMatrix<MAX_ELEMENTS>,
    /// External PSRAM interface reference (would be SPI/address)
    psram_base: u32,
    /// Maximum elements per bank in external memory
    bank_capacity: u32,
}

impl<const MAX_ELEMENTS: usize> TimeSlicer<MAX_ELEMENTS> {
    /// Create a new time slicer
    pub fn new(psram_base: u32, bank_capacity: u32) -> Self {
        let _ = psram_reset();
        let _ = psram_enable();
        Self {
            current_time: 0,
            current_w: SparseWMatrix::new(10_000),
            psram_base,
            bank_capacity,
        }
    }

    /// Get current time step
    pub fn current_time(&self) -> TimeBankIndex {
        self.current_time
    }

    /// Get current W matrix reference
    pub fn current_w(&self) -> &SparseWMatrix<MAX_ELEMENTS> {
        &self.current_w
    }

    /// Get mutable current W matrix reference
    pub fn current_w_mut(&mut self) -> &mut SparseWMatrix<MAX_ELEMENTS> {
        &mut self.current_w
    }

    /// Save current W matrix to time bank
    /// This would write to external PSRAM in practice
    pub fn save_to_bank(&mut self, bank: TimeBankIndex) -> Result<(), ()> {
        if bank >= NUM_TIME_BANKS as u16 {
            return Err(());
        }

        let address = (bank as u32) * self.bank_capacity;
        let data = self.current_w.serialize()?;
        if data.len() as u32 > self.bank_capacity {
            return Err(());
        }

        psram_write(address, &data).map_err(|_| ())?;

        Ok(())
    }

    /// Load W matrix from time bank
    pub fn load_from_bank(&mut self, bank: TimeBankIndex) -> Result<(), ()> {
        if bank >= NUM_TIME_BANKS as u16 {
            return Err(());
        }

        let address = (bank as u32) * self.bank_capacity;
        let data = psram_read(address, self.bank_capacity as usize).map_err(|_| ())?;
        self.current_w = SparseWMatrix::deserialize(&data)?;

        Ok(())
    }

    /// Step forward in time (increment time bank)
    /// Saves current state and advances
    pub fn step_forward(&mut self) -> Result<(), ()> {
        // Save current state to current time bank
        self.save_to_bank(self.current_time)?;
        
        // Advance time
        self.current_time = (self.current_time + 1) % (NUM_TIME_BANKS as u16);
        
        // Clear current W for new evolution
        self.current_w.clear();
        
        Ok(())
    }

    /// Step backward in time (decrement time bank)
    /// Uses U^dagger evolution (backward)
    pub fn step_backward(&mut self) -> Result<(), ()> {
        // Save current state
        self.save_to_bank(self.current_time)?;
        
        // Go backward in time
        self.current_time = if self.current_time == 0 {
            (NUM_TIME_BANKS - 1) as u16
        } else {
            self.current_time - 1
        };
        
        // Load the previous state
        self.load_from_bank(self.current_time)?;
        
        Ok(())
    }

    /// Jump to specific time bank
    pub fn jump_to(&mut self, bank: TimeBankIndex) -> Result<(), ()> {
        // Save current
        self.save_to_bank(self.current_time)?;
        
        // Load target
        self.load_from_bank(bank)?;
        self.current_time = bank;
        
        Ok(())
    }

    /// Reset time slicer to t=0
    pub fn reset(&mut self) {
        self.current_time = 0;
        self.current_w.clear();
    }

    /// Verify Frobenius norm conservation across time steps
    /// This is a key quantum-relational invariant
    pub fn verify_norm_conservation(&self, expected_norm_sq: i64, tolerance: i64) -> bool {
        let current_norm_sq = self.current_w.frobenius_norm_sqr();
        let diff = (current_norm_sq - expected_norm_sq).abs();
        diff <= tolerance
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::FIXED_ONE;

    #[test]
    fn test_sparse_matrix_basic() {
        let mut mat = SparseWMatrix::<100>::new(10);
        
        // Set some elements
        mat.set(0, 0, WElement::new(FIXED_ONE, 0)).unwrap();
        mat.set(5, 5, WElement::new(0, FIXED_ONE)).unwrap();
        
        assert_eq!(mat.len(), 2);
        
        // Retrieve elements
        let elem = mat.get(0, 0);
        assert_eq!(elem.re, FIXED_ONE);
        assert_eq!(elem.im, 0);
        
        // Non-existent element should be zero
        let zero = mat.get(1, 1);
        assert_eq!(zero.re, 0);
        assert_eq!(zero.im, 0);
    }

    #[test]
    fn test_frobenius_norm() {
        let mut mat = SparseWMatrix::<100>::new(10);
        
        // Identity matrix scaled by sqrt(2)
        let val = ((FIXED_ONE as i64 * 46341) / 32768) as Fixed; // sqrt(2)/2 in fixed
        mat.set(0, 0, WElement::new(val, val)).unwrap();
        
        let norm_sq = mat.frobenius_norm_sqr();
        // Should be approximately 2 * FIXED_ONE * FIXED_ONE
        let expected = ((val as i64 * val as i64) >> 16) * 2;
        
        // Allow some fixed-point error
        assert!((norm_sq - expected).abs() < 1000);
    }

    #[test]
    fn test_time_slicer_cycle() {
        let mut slicer = TimeSlicer::<100>::new(0x80000000, 65536);
        
        assert_eq!(slicer.current_time(), 0);
        
        // Step forward
        slicer.step_forward().ok(); // May fail without PSRAM, that's ok for test
        assert_eq!(slicer.current_time(), 1);
        
        // Step backward
        slicer.step_backward().ok();
        assert_eq!(slicer.current_time(), 0);
        
        // Test wraparound
        for _ in 0..NUM_TIME_BANKS {
            slicer.step_forward().ok();
        }
        // Should be back at 0
        assert_eq!(slicer.current_time(), 0);
    }

    #[test]
    fn test_w_element_arithmetic() {
        let a = WElement::new(FIXED_ONE, 0);
        let b = WElement::new(0, FIXED_ONE);
        
        let sum = a.add(&b);
        assert_eq!(sum.re, FIXED_ONE);
        assert_eq!(sum.im, FIXED_ONE);
        
        let half = FIXED_ONE / 2;
        let scaled = a.scale(half);
        assert_eq!(scaled.re, half);
        assert_eq!(scaled.im, 0);
    }
}
