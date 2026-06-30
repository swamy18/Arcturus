//! Hardware Abstraction Layer (HAL) for Arcturus OS
//! 
//! This module provides low-level drivers for the Arcturus chip:
//! - GPIO control for node addressing
//! - SPI communication with external PSRAM and PC interface
//! - Analog I/O for phase injection and conductance measurement

pub mod gpio;
pub mod spi;
pub mod analog;

use embedded_hal::digital::OutputPin;
use embedded_hal::spi::SpiDevice;

/// Core system clock frequency (100 MHz)
pub const SYS_CLOCK_HZ: u32 = 100_000_000;

/// Number of nodes in the grid (100x100)
pub const NUM_NODES: usize = 10_000;

/// Grid dimensions
pub const GRID_SIZE: usize = 100;

/// Long-range edges count
pub const NUM_LONG_RANGE_EDGES: usize = 100;

/// Maximum SPI clock frequency for PSRAM (50 MHz)
pub const PSRAM_SPI_MAX_HZ: u32 = 50_000_000;

/// Error types for HAL operations
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HalError {
    GpioError,
    SpiError,
    AnalogError,
    InvalidNodeId,
    InvalidPhase,
    MeasurementTimeout,
    MemoryError,
}

/// Result type for HAL operations
pub type HalResult<T> = Result<T, HalError>;

/// System HAL structure containing all hardware interfaces
pub struct SystemHal<SPI, GPIO, ADC, DAC>
where
    SPI: SpiDevice,
    GPIO: OutputPin,
{
    /// SPI interface for external memory and PC communication
    pub spi: SPI,
    /// GPIO interface for node addressing
    pub gpio: GPIO,
    /// ADC for conductance measurement
    pub adc: ADC,
    /// DAC for phase injection
    pub dac: DAC,
    /// Node addressing state
    pub current_node: u16,
}

impl<SPI, GPIO, ADC, DAC> SystemHal<SPI, GPIO, ADC, DAC>
where
    SPI: SpiDevice,
    GPIO: OutputPin,
{
    /// Create a new System HAL instance
    pub fn new(spi: SPI, gpio: GPIO, adc: ADC, dac: DAC) -> Self {
        Self {
            spi,
            gpio,
            adc,
            dac,
            current_node: 0,
        }
    }

    /// Select a specific node by ID
    pub fn select_node(&mut self, node_id: u16) -> HalResult<()> {
        if node_id as usize >= NUM_NODES {
            return Err(HalError::InvalidNodeId);
        }
        self.current_node = node_id;
        // GPIO implementation would set address lines here
        Ok(())
    }
}
