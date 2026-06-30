//! SPI Driver for Arcturus OS
//!
//! Manages SPI communication with:
//! - External PSRAM (64MB) for W matrix storage
//! - FT2232H USB-to-SPI bridge for PC communication

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use embedded_hal::spi::{ErrorType, SpiBus, SpiDevice};

/// SPI error types
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpiError {
    BusError,
    DeviceError,
    Timeout,
    InvalidAddress,
    BufferTooLarge,
}

/// PSRAM bank size used by the time-slicing layer.
///
/// 64 KiB per bank yields 1000 banks within the 64 MiB external PSRAM budget.
pub const BANK_SIZE: u32 = 64 * 1024;

/// Total PSRAM capacity available to the firmware.
pub const PSRAM_CAPACITY_BYTES: u32 = 64 * 1024 * 1024;

/// PSRAM command set
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum PsramCommand {
    /// Read data
    Read = 0x03,
    /// Fast read
    FastRead = 0x0B,
    /// Page program (write)
    PageProgram = 0x02,
    /// Sector erase (4KB)
    SectorErase = 0x20,
    /// Block erase (64KB)
    BlockErase = 0xD8,
    /// Chip erase
    ChipErase = 0xC7,
    /// Read status register
    ReadStatus = 0x05,
    /// Write enable
    WriteEnable = 0x06,
    /// Write disable
    WriteDisable = 0x04,
    /// Read JEDEC ID
    ReadJedecId = 0x9F,
}

#[derive(Default)]
struct PsramState {
    enabled: bool,
    memory: BTreeMap<u32, u8>,
}

impl PsramState {
    fn new() -> Self {
        Self {
            enabled: false,
            memory: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
fn with_psram_state<R>(f: impl FnOnce(&mut PsramState) -> R) -> R {
    use std::sync::{Mutex, OnceLock};

    static STATE: OnceLock<Mutex<PsramState>> = OnceLock::new();
    let mutex = STATE.get_or_init(|| Mutex::new(PsramState::new()));
    let mut guard = mutex.lock().expect("psram mutex poisoned");
    f(&mut guard)
}

#[cfg(not(test))]
fn with_psram_state<R>(f: impl FnOnce(&mut PsramState) -> R) -> R {
    static mut STATE: Option<PsramState> = None;

    unsafe {
        if STATE.is_none() {
            STATE = Some(PsramState::new());
        }
        f(STATE.as_mut().expect("psram state not initialized"))
    }
}

fn encode_address(addr: u32) -> [u8; 3] {
    [(addr >> 16) as u8, (addr >> 8) as u8, addr as u8]
}

fn validate_psram_range(addr: u32, len: usize) -> Result<(), SpiError> {
    let end = addr
        .checked_add(len as u32)
        .ok_or(SpiError::InvalidAddress)?;
    if end > PSRAM_CAPACITY_BYTES {
        return Err(SpiError::InvalidAddress);
    }
    Ok(())
}

/// Enable the software-backed PSRAM interface.
///
/// This mirrors the initialization step that would be issued on real hardware.
pub fn psram_enable() -> Result<(), SpiError> {
    with_psram_state(|state| {
        state.enabled = true;
    });
    Ok(())
}

/// Reset the software-backed PSRAM interface.
///
/// This clears the emulated storage and places the device back into a disabled
/// state, matching a hardware reset/initialization cycle.
pub fn psram_reset() -> Result<(), SpiError> {
    with_psram_state(|state| {
        state.enabled = false;
        state.memory.clear();
    });
    Ok(())
}

/// Write bytes to the PSRAM address space using a page-program style command.
///
/// The command frame is `0x02 | addr[23:0] | data...`.
pub fn psram_write(addr: u32, data: &[u8]) -> Result<(), SpiError> {
    validate_psram_range(addr, data.len())?;

    let mut frame = Vec::with_capacity(4 + data.len());
    frame.push(PsramCommand::PageProgram as u8);
    frame.extend_from_slice(&encode_address(addr));
    frame.extend_from_slice(data);

    with_psram_state(|state| {
        if !state.enabled {
            state.enabled = true;
        }

        for (offset, byte) in data.iter().copied().enumerate() {
            state.memory.insert(addr + offset as u32, byte);
        }
    });

    Ok(())
}

/// Read bytes from the PSRAM address space using a read command.
///
/// The command frame is `0x03 | addr[23:0]` followed by `len` data bytes.
pub fn psram_read(addr: u32, len: usize) -> Result<Vec<u8>, SpiError> {
    validate_psram_range(addr, len)?;

    let mut frame = Vec::with_capacity(4);
    frame.push(PsramCommand::Read as u8);
    frame.extend_from_slice(&encode_address(addr));

    let out = with_psram_state(|state| {
        if !state.enabled {
            state.enabled = true;
        }

        let mut data = Vec::with_capacity(len);
        for offset in 0..len {
            data.push(*state.memory.get(&(addr + offset as u32)).unwrap_or(&0));
        }
        data
    });

    Ok(out)
}

/// PSRAM memory controller for external 64MB SPI PSRAM
pub struct PsramController<BUS>
where
    BUS: SpiBus,
{
    /// SPI bus instance
    spi: BUS,
    /// Chip select pin (managed separately for shared bus)
    pub capacity_bytes: u32,
    /// Page size for writes (typically 256 bytes)
    pub page_size: u16,
    /// Sector size for erases (typically 4096 bytes)
    pub sector_size: u16,
}

impl<BUS> PsramController<BUS>
where
    BUS: SpiBus,
    BUS::Error: core::fmt::Debug,
{
    /// Create a new PSRAM controller
    pub fn new(spi: BUS) -> Self {
        Self {
            spi,
            capacity_bytes: 64 * 1024 * 1024, // 64MB
            page_size: 256,
            sector_size: 4096,
        }
    }

    /// Read JEDEC ID to verify PSRAM
    pub fn read_jedec_id(&mut self) -> Result<(u8, u8, u8), SpiError> {
        let mut id = [0u8; 3];
        let cmd = [PsramCommand::ReadJedecId as u8];
        
        self.spi.write(&cmd).map_err(|_| SpiError::BusError)?;
        self.spi.read(&mut id).map_err(|_| SpiError::BusError)?;
        
        Ok((id[0], id[1], id[2]))
    }

    /// Read status register
    pub fn read_status(&mut self) -> Result<u8, SpiError> {
        let mut status = [0u8; 1];
        let cmd = [PsramCommand::ReadStatus as u8];
        
        self.spi.write(&cmd).map_err(|_| SpiError::BusError)?;
        self.spi.read(&mut status).map_err(|_| SpiError::BusError)?;
        
        Ok(status[0])
    }

    /// Enable write operations
    pub fn write_enable(&mut self) -> Result<(), SpiError> {
        let cmd = [PsramCommand::WriteEnable as u8];
        self.spi.write(&cmd).map_err(|_| SpiError::BusError)
    }

    /// Disable write operations
    pub fn write_disable(&mut self) -> Result<(), SpiError> {
        let cmd = [PsramCommand::WriteDisable as u8];
        self.spi.write(&cmd).map_err(|_| SpiError::BusError)
    }

    /// Read data from PSRAM
    pub fn read(&mut self, address: u32, buffer: &mut [u8]) -> Result<(), SpiError> {
        if address.saturating_add(buffer.len() as u32) > self.capacity_bytes {
            return Err(SpiError::InvalidAddress);
        }

        let cmd = [
            PsramCommand::Read as u8,
            (address >> 16) as u8,
            (address >> 8) as u8,
            address as u8,
        ];

        self.spi.write(&cmd).map_err(|_| SpiError::BusError)?;
        self.spi.read(buffer).map_err(|_| SpiError::BusError)?;

        Ok(())
    }

    /// Write data to PSRAM (page program)
    pub fn write(&mut self, address: u32, data: &[u8]) -> Result<(), SpiError> {
        if data.len() > self.page_size as usize {
            return Err(SpiError::BufferTooLarge);
        }
        if address.saturating_add(data.len() as u32) > self.capacity_bytes {
            return Err(SpiError::InvalidAddress);
        }

        self.write_enable()?;

        let mut cmd = heapless::Vec::<u8, 260>::new();
        cmd.push(PsramCommand::PageProgram as u8).map_err(|_| SpiError::BufferTooLarge)?;
        cmd.push((address >> 16) as u8).map_err(|_| SpiError::BufferTooLarge)?;
        cmd.push((address >> 8) as u8).map_err(|_| SpiError::BufferTooLarge)?;
        cmd.push(address as u8).map_err(|_| SpiError::BufferTooLarge)?;
        
        for byte in data {
            cmd.push(*byte).map_err(|_| SpiError::BufferTooLarge)?;
        }

        self.spi.write(&cmd).map_err(|_| SpiError::BusError)?;

        // Wait for write completion
        while self.read_status()? & 0x01 != 0 {}

        self.write_disable()?;

        Ok(())
    }

    /// Erase a 4KB sector
    pub fn sector_erase(&mut self, address: u32) -> Result<(), SpiError> {
        if address % self.sector_size as u32 != 0 {
            return Err(SpiError::InvalidAddress);
        }

        self.write_enable()?;

        let cmd = [
            PsramCommand::SectorErase as u8,
            (address >> 16) as u8,
            (address >> 8) as u8,
            address as u8,
        ];

        self.spi.write(&cmd).map_err(|_| SpiError::BusError)?;

        // Wait for erase completion
        while self.read_status()? & 0x01 != 0 {}

        self.write_disable()?;

        Ok(())
    }
}

/// PC Communication interface via FT2232H SPI bridge
pub struct PcInterface<DEVICE>
where
    DEVICE: SpiDevice,
{
    device: DEVICE,
    command_buffer: [u8; 256],
}

impl<DEVICE> PcInterface<DEVICE>
where
    DEVICE: SpiDevice,
    DEVICE::Error: core::fmt::Debug,
{
    /// Create a new PC interface
    pub fn new(device: DEVICE) -> Self {
        Self {
            device,
            command_buffer: [0u8; 256],
        }
    }

    /// Receive a command from PC
    pub fn receive_command(&mut self) -> Result<HostCommand, SpiError> {
        // Read header (4 bytes: [cmd_id, param1, param2, length])
        let mut header = [0u8; 4];
        self.device.read(&mut header).map_err(|_| SpiError::BusError)?;

        let cmd_id = header[0];
        let param1 = header[1];
        let param2 = header[2];
        let data_len = header[3] as usize;

        // Read additional data if needed
        if data_len > 0 && data_len <= 252 {
            self.device
                .read(&mut self.command_buffer[0..data_len])
                .map_err(|_| SpiError::BusError)?;
        }

        HostCommand::decode(cmd_id, param1, param2, &self.command_buffer[0..data_len])
            .ok_or(SpiError::BusError)
    }

    /// Send a response to PC
    pub fn send_response(&mut self, response: HostResponse) -> Result<(), SpiError> {
        let data = response.encode();
        self.device.write(&data).map_err(|_| SpiError::BusError)
    }
}

/// Commands from host PC
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HostCommand {
    /// Read from memory
    Read { bank: u8, address: u16 },
    /// Write to memory
    Write { bank: u8, address: u16, data: u32 },
    /// Apply phase to node
    ApplyPhase { node_id: u16, angle: u16 },
    /// Measure conductance
    MeasureConductance { node_id: u16 },
    /// Evolve system one time step
    Evolve { alpha: u16 },
    /// Synchronize time states
    SyncTime,
    /// Get status
    GetStatus,
    /// Reset chip
    Reset,
}

impl HostCommand {
    /// Decode command from raw bytes
    pub fn decode(cmd_id: u8, param1: u8, param2: u8, data: &[u8]) -> Option<Self> {
        match cmd_id {
            0x01 => Some(HostCommand::Read {
                bank: param1,
                address: ((param2 as u16) << 8) | (data.get(0).copied().unwrap_or(0) as u16),
            }),
            0x02 => Some(HostCommand::Write {
                bank: param1,
                address: ((param2 as u16) << 8) | (data.get(0).copied().unwrap_or(0) as u16),
                data: u32::from_le_bytes([
                    data.get(1).copied().unwrap_or(0),
                    data.get(2).copied().unwrap_or(0),
                    data.get(3).copied().unwrap_or(0),
                    data.get(4).copied().unwrap_or(0),
                ]),
            }),
            0x03 => Some(HostCommand::ApplyPhase {
                node_id: ((param1 as u16) << 8) | (param2 as u16),
                angle: data.get(0).map(|&b| (b as u16) << 8).unwrap_or(0)
                    | data.get(1).copied().unwrap_or(0) as u16,
            }),
            0x04 => Some(HostCommand::MeasureConductance {
                node_id: ((param1 as u16) << 8) | (param2 as u16),
            }),
            0x05 => Some(HostCommand::Evolve {
                alpha: ((param1 as u16) << 8) | (param2 as u16),
            }),
            0x06 => Some(HostCommand::SyncTime),
            0x07 => Some(HostCommand::GetStatus),
            0x08 => Some(HostCommand::Reset),
            _ => None,
        }
    }
}

/// Responses to host PC
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HostResponse {
    /// Read data response
    ReadData { data: u32 },
    /// Write acknowledge
    WriteAck,
    /// Phase applied acknowledge
    PhaseAck,
    /// Conductance measurement result
    ConductanceResult { value: u16 },
    /// Evolution complete
    EvolutionComplete,
    /// Sync complete
    SyncComplete,
    /// Status response
    Status {
        firmware_version: u16,
        node_count: u16,
        current_time_bank: u8,
        flags: u8,
    },
    /// Reset acknowledge
    ResetAck,
    /// Error response
    Error { code: u8 },
}

impl HostResponse {
    /// Encode response to bytes
    pub fn encode(&self) -> [u8; 8] {
        let mut buf = [0u8; 8];

        match self {
            HostResponse::ReadData { data } => {
                buf[0] = 0x81;
                buf[1..5].copy_from_slice(&data.to_le_bytes());
            }
            HostResponse::WriteAck => {
                buf[0] = 0x82;
            }
            HostResponse::PhaseAck => {
                buf[0] = 0x83;
            }
            HostResponse::ConductanceResult { value } => {
                buf[0] = 0x84;
                buf[1..3].copy_from_slice(&value.to_le_bytes());
            }
            HostResponse::EvolutionComplete => {
                buf[0] = 0x85;
            }
            HostResponse::SyncComplete => {
                buf[0] = 0x86;
            }
            HostResponse::Status {
                firmware_version,
                node_count,
                current_time_bank,
                flags,
            } => {
                buf[0] = 0x87;
                buf[1..3].copy_from_slice(&firmware_version.to_le_bytes());
                buf[3..5].copy_from_slice(&node_count.to_le_bytes());
                buf[5] = *current_time_bank;
                buf[6] = *flags;
            }
            HostResponse::ResetAck => {
                buf[0] = 0x88;
            }
            HostResponse::Error { code } => {
                buf[0] = 0xFF;
                buf[1] = *code;
            }
        }

        buf
    }
}
