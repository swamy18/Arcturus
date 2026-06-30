//! PC Communication API
//!
//! Command/response protocol for host PC communication via SPI.
//! 
//! Protocol format:
//! - Command: [CMD_ID:1][PARAM1:1][PARAM2:1][LEN:1][DATA:0-252]
//! - Response: [STATUS:1][LEN:1][DATA:0-6]

/// Command ID values
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CommandId {
    /// No operation (ping)
    Nop = 0x00,
    /// Read from memory
    Read = 0x01,
    /// Write to memory
    Write = 0x02,
    /// Apply phase to node
    ApplyPhase = 0x03,
    /// Measure conductance
    MeasureConductance = 0x04,
    /// Evolve system
    Evolve = 0x05,
    /// Synchronize time
    SyncTime = 0x06,
    /// Get system status
    GetStatus = 0x07,
    /// Reset chip
    Reset = 0x08,
    /// Set alpha parameter
    SetAlpha = 0x09,
    /// Get current time bank
    GetTimeBank = 0x0A,
    /// Jump to time bank
    JumpTimeBank = 0x0B,
    /// Read eigenmode data
    ReadEigen = 0x0C,
    /// Write eigenmode data
    WriteEigen = 0x0D,
    /// Read edge cache
    ReadEdgeCache = 0x0E,
    /// Write edge cache
    WriteEdgeCache = 0x0F,
    /// Batch read
    BatchRead = 0x10,
    /// Batch write
    BatchWrite = 0x11,
    /// Error response
    Error = 0xFF,
}

impl CommandId {
    /// Convert from raw byte
    pub fn from_u8(value: u8) -> Option<Self> {
        use CommandId::*;
        match value {
            0x00 => Some(Nop),
            0x01 => Some(Read),
            0x02 => Some(Write),
            0x03 => Some(ApplyPhase),
            0x04 => Some(MeasureConductance),
            0x05 => Some(Evolve),
            0x06 => Some(SyncTime),
            0x07 => Some(GetStatus),
            0x08 => Some(Reset),
            0x09 => Some(SetAlpha),
            0x0A => Some(GetTimeBank),
            0x0B => Some(JumpTimeBank),
            0x0C => Some(ReadEigen),
            0x0D => Some(WriteEigen),
            0x0E => Some(ReadEdgeCache),
            0x0F => Some(WriteEdgeCache),
            0x10 => Some(BatchRead),
            0x11 => Some(BatchWrite),
            0xFF => Some(Error),
            _ => None,
        }
    }
}

/// Status codes for responses
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum StatusCode {
    /// Success
    Ok = 0x00,
    /// General error
    Error = 0x01,
    /// Invalid command
    InvalidCommand = 0x02,
    /// Invalid parameter
    InvalidParameter = 0x03,
    /// Memory error
    MemoryError = 0x04,
    /// Compute error
    ComputeError = 0x05,
    /// I/O error
    IoError = 0x06,
    /// Timeout
    Timeout = 0x07,
    /// Busy (try again later)
    Busy = 0x08,
    /// Not implemented
    NotImplemented = 0x09,
}

/// Command packet structure
#[derive(Debug, Clone, Copy)]
pub struct CommandPacket {
    /// Command ID
    pub cmd: CommandId,
    /// First parameter byte
    pub param1: u8,
    /// Second parameter byte
    pub param2: u8,
    /// Data length (0-252)
    pub data_len: u8,
    /// Optional data (up to 252 bytes)
    pub data: [u8; 252],
}

impl CommandPacket {
    /// Create a new command packet
    pub fn new(cmd: CommandId, param1: u8, param2: u8) -> Self {
        Self {
            cmd,
            param1,
            param2,
            data_len: 0,
            data: [0; 252],
        }
    }

    /// Set data payload
    pub fn with_data(mut self, data: &[u8]) -> Self {
        let len = data.len().min(252);
        self.data_len = len as u8;
        self.data[..len].copy_from_slice(&data[..len]);
        self
    }

    /// Serialize to bytes (4-byte header + data)
    pub fn serialize(&self, buffer: &mut [u8]) -> usize {
        let total_len = 4 + self.data_len as usize;
        if buffer.len() < total_len {
            return 0;
        }

        buffer[0] = self.cmd as u8;
        buffer[1] = self.param1;
        buffer[2] = self.param2;
        buffer[3] = self.data_len;

        if self.data_len > 0 {
            buffer[4..4 + self.data_len as usize]
                .copy_from_slice(&self.data[..self.data_len as usize]);
        }

        total_len
    }

    /// Deserialize from bytes
    pub fn deserialize(buffer: &[u8]) -> Option<Self> {
        if buffer.len() < 4 {
            return None;
        }

        let cmd = CommandId::from_u8(buffer[0])?;
        let param1 = buffer[1];
        let param2 = buffer[2];
        let data_len = buffer[3];

        if buffer.len() < 4 + data_len as usize {
            return None;
        }

        let mut packet = Self::new(cmd, param1, param2);
        packet.data_len = data_len;
        if data_len > 0 {
            packet.data[..data_len as usize]
                .copy_from_slice(&buffer[4..4 + data_len as usize]);
        }

        Some(packet)
    }
}

/// Response packet structure
#[derive(Debug, Clone, Copy)]
pub struct ResponsePacket {
    /// Status code
    pub status: StatusCode,
    /// Response data (up to 6 bytes)
    pub data: [u8; 6],
    /// Data length
    pub data_len: u8,
}

impl ResponsePacket {
    /// Create a success response
    pub fn ok() -> Self {
        Self {
            status: StatusCode::Ok,
            data: [0; 6],
            data_len: 0,
        }
    }

    /// Create an error response
    pub fn error(code: StatusCode) -> Self {
        Self {
            status: code,
            data: [0; 6],
            data_len: 0,
        }
    }

    /// Add data to response
    pub fn with_data(mut self, data: &[u8]) -> Self {
        let len = data.len().min(6);
        self.data_len = len as u8;
        self.data[..len].copy_from_slice(&data[..len]);
        self
    }

    /// Add 32-bit value to response
    pub fn with_u32(self, value: u32) -> Self {
        self.with_data(&value.to_le_bytes())
    }

    /// Add 16-bit value to response
    pub fn with_u16(self, value: u16) -> Self {
        self.with_data(&value.to_le_bytes())
    }

    /// Serialize to bytes
    pub fn serialize(&self) -> [u8; 8] {
        let mut buf = [0u8; 8];
        buf[0] = self.status as u8;
        buf[1] = self.data_len;
        buf[2..8].copy_from_slice(&self.data);
        buf
    }
}

/// API request/response handler
pub struct ApiHandler {
    /// Current command being processed
    current_cmd: Option<CommandId>,
    /// Buffer for incoming command data
    rx_buffer: [u8; 256],
    /// Buffer for outgoing response data
    tx_buffer: [u8; 256],
    /// Receive position
    rx_pos: usize,
    /// Transmission position
    tx_pos: usize,
    /// Command processing state
    state: HandlerState,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum HandlerState {
    Idle,
    Receiving,
    Processing,
    Transmitting,
}

impl ApiHandler {
    /// Create a new API handler
    pub fn new() -> Self {
        Self {
            current_cmd: None,
            rx_buffer: [0; 256],
            tx_buffer: [0; 256],
            rx_pos: 0,
            tx_pos: 0,
            state: HandlerState::Idle,
        }
    }

    /// Process incoming byte
    /// Returns true if a complete command was received
    pub fn rx_byte(&mut self, byte: u8) -> bool {
        if self.state == HandlerState::Idle {
            self.state = HandlerState::Receiving;
            self.rx_pos = 0;
        }

        if self.rx_pos < self.rx_buffer.len() {
            self.rx_buffer[self.rx_pos] = byte;
            self.rx_pos += 1;

            // Check if we have a complete command
            if self.rx_pos >= 4 {
                let data_len = self.rx_buffer[3] as usize;
                if self.rx_pos >= 4 + data_len {
                    return true;
                }
            }
        }

        false
    }

    /// Get received command packet
    pub fn get_command(&self) -> Option<CommandPacket> {
        CommandPacket::deserialize(&self.rx_buffer[..self.rx_pos])
    }

    /// Start transmission of response
    pub fn start_tx(&mut self, response: &ResponsePacket) {
        let data = response.serialize();
        self.tx_buffer[..8].copy_from_slice(&data);
        self.tx_pos = 0;
        self.state = HandlerState::Transmitting;
    }

    /// Get next byte to transmit
    /// Returns None if transmission complete
    pub fn tx_byte(&mut self) -> Option<u8> {
        if self.state != HandlerState::Transmitting {
            return None;
        }

        if self.tx_pos < 8 {
            let byte = self.tx_buffer[self.tx_pos];
            self.tx_pos += 1;
            Some(byte)
        } else {
            self.state = HandlerState::Idle;
            None
        }
    }

    /// Check if handler is idle
    pub fn is_idle(&self) -> bool {
        self.state == HandlerState::Idle
    }

    /// Reset handler state
    pub fn reset(&mut self) {
        self.state = HandlerState::Idle;
        self.rx_pos = 0;
        self.tx_pos = 0;
        self.current_cmd = None;
    }
}

impl Default for ApiHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Command processor
/// Dispatches commands to appropriate handlers
pub struct CommandProcessor;

impl CommandProcessor {
    /// Process a command and return a response
    pub fn process(cmd: &CommandPacket) -> ResponsePacket {
        match cmd.cmd {
            CommandId::Nop => ResponsePacket::ok(),
            CommandId::GetStatus => Self::handle_get_status(),
            CommandId::Reset => Self::handle_reset(),
            CommandId::Read => Self::handle_read(cmd),
            CommandId::Write => Self::handle_write(cmd),
            CommandId::ApplyPhase => Self::handle_apply_phase(cmd),
            CommandId::MeasureConductance => Self::handle_measure_conductance(cmd),
            CommandId::Evolve => Self::handle_evolve(cmd),
            CommandId::SyncTime => Self::handle_sync_time(),
            CommandId::SetAlpha => Self::handle_set_alpha(cmd),
            CommandId::GetTimeBank => Self::handle_get_time_bank(),
            CommandId::JumpTimeBank => Self::handle_jump_time_bank(cmd),
            CommandId::ReadEigen => Self::handle_read_eigen(cmd),
            CommandId::WriteEigen => Self::handle_write_eigen(cmd),
            CommandId::ReadEdgeCache => Self::handle_read_edge_cache(cmd),
            CommandId::WriteEdgeCache => Self::handle_write_edge_cache(cmd),
            CommandId::BatchRead => Self::handle_batch_read(cmd),
            CommandId::BatchWrite => Self::handle_batch_write(cmd),
            CommandId::Error => ResponsePacket::error(StatusCode::Error),
        }
    }

    fn handle_get_status() -> ResponsePacket {
        // In real implementation, read from system status
        let version: u16 = ((crate::FIRMWARE_VERSION.0 as u16) << 8) | 
                           ((crate::FIRMWARE_VERSION.1 as u16) << 4) | 
                           (crate::FIRMWARE_VERSION.2 as u16);
        
        let data = [
            (version >> 8) as u8,
            version as u8,
            0x00, // State: Idle
            0x00, // Flags
        ];
        
        ResponsePacket::ok().with_data(&data)
    }

    fn handle_reset() -> ResponsePacket {
        // In real implementation, would trigger system reset
        ResponsePacket::ok()
    }

    fn handle_read(cmd: &CommandPacket) -> ResponsePacket {
        // Parse bank and address from parameters
        let bank = cmd.param1;
        let addr_high = cmd.param2;
        let _addr_low = if cmd.data_len > 0 { cmd.data[0] } else { 0 };
        let address = ((addr_high as u16) << 8) | (cmd.param2 as u16); // Simplified
        
        // In real implementation, read from memory
        let data = (bank as u32) << 24 | (address as u32);
        ResponsePacket::ok().with_u32(data)
    }

    fn handle_write(cmd: &CommandPacket) -> ResponsePacket {
        // Parse parameters similar to read
        // Write data from command data field
        ResponsePacket::ok()
    }

    fn handle_apply_phase(cmd: &CommandPacket) -> ResponsePacket {
        let node_id = ((cmd.param1 as u16) << 8) | (cmd.param2 as u16);
        let angle = if cmd.data_len >= 2 {
            ((cmd.data[0] as u16) << 8) | (cmd.data[1] as u16)
        } else {
            0
        };
        
        // In real implementation, apply phase to node
        let _ = node_id;
        let _ = angle;
        
        ResponsePacket::ok()
    }

    fn handle_measure_conductance(cmd: &CommandPacket) -> ResponsePacket {
        let node_id = ((cmd.param1 as u16) << 8) | (cmd.param2 as u16);
        
        // In real implementation, measure conductance
        let measurement: u16 = 0x1234; // Placeholder
        
        let _ = node_id;
        ResponsePacket::ok().with_u16(measurement)
    }

    fn handle_evolve(cmd: &CommandPacket) -> ResponsePacket {
        let alpha = ((cmd.param1 as u16) << 8) | (cmd.param2 as u16);
        let steps = if cmd.data_len > 0 { cmd.data[0] } else { 1 };
        
        // In real implementation, perform evolution
        let _ = alpha;
        let _ = steps;
        
        ResponsePacket::ok()
    }

    fn handle_sync_time() -> ResponsePacket {
        // In real implementation, synchronize time states
        ResponsePacket::ok()
    }

    fn handle_set_alpha(cmd: &CommandPacket) -> ResponsePacket {
        let alpha_high = cmd.param1;
        let alpha_low = cmd.param2;
        let _alpha = ((alpha_high as u16) << 8) | (alpha_low as u16);
        
        // In real implementation, set alpha parameter
        ResponsePacket::ok()
    }

    fn handle_get_time_bank() -> ResponsePacket {
        // In real implementation, get current time bank
        let bank: u16 = 0;
        ResponsePacket::ok().with_u16(bank)
    }

    fn handle_jump_time_bank(cmd: &CommandPacket) -> ResponsePacket {
        let bank = ((cmd.param1 as u16) << 8) | (cmd.param2 as u16);
        let _ = bank;
        // In real implementation, jump to time bank
        ResponsePacket::ok()
    }

    fn handle_read_eigen(cmd: &CommandPacket) -> ResponsePacket {
        let mode = ((cmd.param1 as u16) << 8) | (cmd.param2 as u16);
        let _ = mode;
        // In real implementation, read eigenmode
        ResponsePacket::ok()
    }

    fn handle_write_eigen(cmd: &CommandPacket) -> ResponsePacket {
        let mode = ((cmd.param1 as u16) << 8) | (cmd.param2 as u16);
        let _ = mode;
        // In real implementation, write eigenmode
        ResponsePacket::ok()
    }

    fn handle_read_edge_cache(cmd: &CommandPacket) -> ResponsePacket {
        let node_id = ((cmd.param1 as u16) << 8) | (cmd.param2 as u16);
        let _ = node_id;
        // In real implementation, read edge cache
        ResponsePacket::ok()
    }

    fn handle_write_edge_cache(cmd: &CommandPacket) -> ResponsePacket {
        let node_id = ((cmd.param1 as u16) << 8) | (cmd.param2 as u16);
        let data = if cmd.data_len > 0 { cmd.data[0] & 0x03 } else { 0 };
        let _ = node_id;
        let _ = data;
        // In real implementation, write edge cache
        ResponsePacket::ok()
    }

    fn handle_batch_read(cmd: &CommandPacket) -> ResponsePacket {
        let count = cmd.param1;
        let _ = count;
        // In real implementation, batch read
        ResponsePacket::ok()
    }

    fn handle_batch_write(cmd: &CommandPacket) -> ResponsePacket {
        let count = cmd.param1;
        let _ = count;
        // In real implementation, batch write
        ResponsePacket::ok()
    }
}
