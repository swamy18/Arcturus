//! GPIO Driver for Arcturus Node Addressing
//!
//! Manages the 100x100 grid addressing using row/column decoders
//! and controls phase injection / conductance measurement select lines.

use embedded_hal::digital::{OutputPin, PinState};

/// Number of row address bits (7 bits for 100 rows, ceil(log2(100)))
pub const ROW_ADDR_BITS: usize = 7;

/// Number of column address bits (7 bits for 100 columns)
pub const COL_ADDR_BITS: usize = 7;

/// Total address bits needed
pub const TOTAL_ADDR_BITS: usize = ROW_ADDR_BITS + COL_ADDR_BITS;

/// GPIO pin configuration for node addressing
pub struct NodeAddressGpio<ROW0, ROW1, ROW2, ROW3, ROW4, ROW5, ROW6,
                           COL0, COL1, COL2, COL3, COL4, COL5, COL6,
                           PHASE_SEL, COND_SEL>
where
    ROW0: OutputPin, ROW1: OutputPin, ROW2: OutputPin, ROW3: OutputPin,
    ROW4: OutputPin, ROW5: OutputPin, ROW6: OutputPin,
    COL0: OutputPin, COL1: OutputPin, COL2: OutputPin, COL3: OutputPin,
    COL4: OutputPin, COL5: OutputPin, COL6: OutputPin,
    PHASE_SEL: OutputPin, COND_SEL: OutputPin,
{
    /// Row address lines (0-99)
    pub row_pins: (ROW0, ROW1, ROW2, ROW3, ROW4, ROW5, ROW6),
    /// Column address lines (0-99)
    pub col_pins: (COL0, COL1, COL2, COL3, COL4, COL5, COL6),
    /// Phase injection select
    pub phase_sel: PHASE_SEL,
    /// Conductance measurement select
    pub cond_sel: COND_SEL,
    /// Current node ID (0-9999)
    pub current_node: u16,
}

impl<ROW0, ROW1, ROW2, ROW3, ROW4, ROW5, ROW6,
     COL0, COL1, COL2, COL3, COL4, COL5, COL6,
     PHASE_SEL, COND_SEL>
NodeAddressGpio<ROW0, ROW1, ROW2, ROW3, ROW4, ROW5, ROW6,
                COL0, COL1, COL2, COL3, COL4, COL5, COL6,
                PHASE_SEL, COND_SEL>
where
    ROW0: OutputPin, ROW1: OutputPin, ROW2: OutputPin, ROW3: OutputPin,
    ROW4: OutputPin, ROW5: OutputPin, ROW6: OutputPin,
    COL0: OutputPin, COL1: OutputPin, COL2: OutputPin, COL3: OutputPin,
    COL4: OutputPin, COL5: OutputPin, COL6: OutputPin,
    PHASE_SEL: OutputPin, COND_SEL: OutputPin,
{
    /// Create a new GPIO address driver
    pub fn new(
        row_pins: (ROW0, ROW1, ROW2, ROW3, ROW4, ROW5, ROW6),
        col_pins: (COL0, COL1, COL2, COL3, COL4, COL5, COL6),
        phase_sel: PHASE_SEL,
        cond_sel: COND_SEL,
    ) -> Self {
        Self {
            row_pins,
            col_pins,
            phase_sel,
            cond_sel,
            current_node: 0,
        }
    }

    /// Convert node ID to row and column indices
    /// Node ID = row * 100 + col
    pub fn node_to_coords(node_id: u16) -> (u8, u8) {
        let row = (node_id / 100) as u8;
        let col = (node_id % 100) as u8;
        (row, col)
    }

    /// Convert row and column to node ID
    pub fn coords_to_node(row: u8, col: u8) -> u16 {
        (row as u16) * 100 + (col as u16)
    }

    /// Set the row address lines
    fn set_row_address(&mut self, row: u8) -> Result<(), GpioError> {
        let bits = Self::to_7bit(row);
        self.set_row_bits(bits)?;
        Ok(())
    }

    /// Set the column address lines
    fn set_col_address(&mut self, col: u8) -> Result<(), GpioError> {
        let bits = Self::to_7bit(col);
        self.set_col_bits(bits)?;
        Ok(())
    }

    /// Convert value to 7-bit representation
    fn to_7bit(value: u8) -> [bool; 7] {
        [
            (value & 0x01) != 0,
            (value & 0x02) != 0,
            (value & 0x04) != 0,
            (value & 0x08) != 0,
            (value & 0x10) != 0,
            (value & 0x20) != 0,
            (value & 0x40) != 0,
        ]
    }

    /// Set row pins from bit array
    fn set_row_bits(&mut self, bits: [bool; 7]) -> Result<(), GpioError> {
        // Macro to set each pin
        macro_rules! set_pin {
            ($pin:expr, $bit:expr) => {
                $pin.set_state(if $bit { PinState::High } else { PinState::Low })
                    .map_err(|_| GpioError::PinError)?;
            };
        }

        let (ref mut p0, ref mut p1, ref mut p2, ref mut p3, ref mut p4, ref mut p5, ref mut p6) = self.row_pins;
        set_pin!(p0, bits[0]);
        set_pin!(p1, bits[1]);
        set_pin!(p2, bits[2]);
        set_pin!(p3, bits[3]);
        set_pin!(p4, bits[4]);
        set_pin!(p5, bits[5]);
        set_pin!(p6, bits[6]);

        Ok(())
    }

    /// Set column pins from bit array
    fn set_col_bits(&mut self, bits: [bool; 7]) -> Result<(), GpioError> {
        macro_rules! set_pin {
            ($pin:expr, $bit:expr) => {
                $pin.set_state(if $bit { PinState::High } else { PinState::Low })
                    .map_err(|_| GpioError::PinError)?;
            };
        }

        let (ref mut p0, ref mut p1, ref mut p2, ref mut p3, ref mut p4, ref mut p5, ref mut p6) = self.col_pins;
        set_pin!(p0, bits[0]);
        set_pin!(p1, bits[1]);
        set_pin!(p2, bits[2]);
        set_pin!(p3, bits[3]);
        set_pin!(p4, bits[4]);
        set_pin!(p5, bits[5]);
        set_pin!(p6, bits[6]);

        Ok(())
    }

    /// Select a node for operations
    pub fn select_node(&mut self, node_id: u16) -> Result<(), GpioError> {
        if node_id >= 10000 {
            return Err(GpioError::InvalidAddress);
        }

        let (row, col) = Self::node_to_coords(node_id);
        self.set_row_address(row)?;
        self.set_col_address(col)?;
        self.current_node = node_id;

        Ok(())
    }

    /// Enable phase injection for the selected node
    pub fn enable_phase_injection(&mut self) -> Result<(), GpioError> {
        self.phase_sel.set_high().map_err(|_| GpioError::PinError)
    }

    /// Disable phase injection
    pub fn disable_phase_injection(&mut self) -> Result<(), GpioError> {
        self.phase_sel.set_low().map_err(|_| GpioError::PinError)
    }

    /// Enable conductance measurement for the selected node
    pub fn enable_conductance_measurement(&mut self) -> Result<(), GpioError> {
        self.cond_sel.set_high().map_err(|_| GpioError::PinError)
    }

    /// Disable conductance measurement
    pub fn disable_conductance_measurement(&mut self) -> Result<(), GpioError> {
        self.cond_sel.set_low().map_err(|_| GpioError::PinError)
    }
}

/// GPIO operation errors
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GpioError {
    /// Pin state setting failed
    PinError,
    /// Invalid node address
    InvalidAddress,
}
