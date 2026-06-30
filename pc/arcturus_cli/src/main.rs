//! Arcturus CLI Tool
//!
//! Command-line interface for interacting with the Arcturus chip via USB-to-SPI bridge.
//!
//! Usage:
//!   arcturus-cli --port COM3 status
//!   arcturus-cli --port COM3 read --bank 0 --address 0x100
//!   arcturus-cli --port COM3 evolve --alpha 1.2 --steps 10

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serialport::{SerialPort, SerialPortType};
use std::io::{Read, Write};
use std::time::{Duration, Instant};

/// Default baud rate for SPI bridge communication
const DEFAULT_BAUD: u32 = 115200;

/// Command timeout
const TIMEOUT: Duration = Duration::from_secs(5);

/// Maximum payload carried by a command packet.
const MAX_COMMAND_DATA_LEN: usize = 255;

/// Maximum payload returned by the fixed-width 8-byte response packet.
const MAX_RESPONSE_DATA_LEN: usize = 6;

/// Protocol command identifiers used by the USB-to-SPI bridge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum CommandId {
    GetStatus = 0x00,
    Read = 0x01,
    Write = 0x02,
    ApplyPhase = 0x03,
    MeasureConductance = 0x04,
    Evolve = 0x05,
    SyncTime = 0x06,
    GetTimeBank = 0x07,
    JumpTimeBank = 0x08,
    Reset = 0x09,
}

/// Device response status codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StatusCode {
    Ok,
    Error,
    InvalidCommand,
    InvalidParameter,
    MemoryError,
    ComputeError,
    IoError,
    Timeout,
    Busy,
    NotImplemented,
}

/// Serialized command packet sent to the bridge.
#[derive(Debug, Clone)]
struct CommandPacket {
    command: CommandId,
    param1: u8,
    param2: u8,
    data: [u8; MAX_COMMAND_DATA_LEN],
    data_len: u8,
}

impl CommandPacket {
    fn new(command: CommandId, param1: u8, param2: u8) -> Self {
        Self {
            command,
            param1,
            param2,
            data: [0; MAX_COMMAND_DATA_LEN],
            data_len: 0,
        }
    }

    fn with_data(mut self, data: &[u8]) -> Self {
        let copy_len = data.len().min(MAX_COMMAND_DATA_LEN);
        self.data[..copy_len].copy_from_slice(&data[..copy_len]);
        self.data_len = copy_len as u8;
        self
    }

    fn serialize(&self, out: &mut [u8]) -> usize {
        let payload_len = self.data_len as usize;
        let packet_len = 4 + payload_len;
        assert!(out.len() >= packet_len);

        out[0] = self.command as u8;
        out[1] = self.param1;
        out[2] = self.param2;
        out[3] = self.data_len;
        out[4..packet_len].copy_from_slice(&self.data[..payload_len]);

        packet_len
    }
}

/// Fixed-width response packet returned by the bridge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ResponsePacket {
    status: StatusCode,
    data: [u8; MAX_RESPONSE_DATA_LEN],
    data_len: u8,
}

/// Arcturus CLI arguments
#[derive(Parser)]
#[command(name = "arcturus-cli")]
#[command(about = "Arcturus Chip Control Interface")]
#[command(version)]
struct Cli {
    /// Serial port for communication (e.g., COM3, /dev/ttyUSB0)
    #[arg(short, long)]
    port: String,

    /// Baud rate for serial communication
    #[arg(short, long, default_value_t = DEFAULT_BAUD)]
    baud: u32,

    /// Enable verbose output
    #[arg(short, long)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

/// Available commands
#[derive(Subcommand)]
enum Commands {
    /// Get system status
    Status,

    /// Read from memory
    Read {
        /// Memory bank (0-999 for time banks, 1000+ for special)
        #[arg(short, long)]
        bank: u16,

        /// Address within bank
        #[arg(short, long, value_parser = parse_hex_or_dec)]
        address: u16,
    },

    /// Write to memory
    Write {
        /// Memory bank
        #[arg(short, long)]
        bank: u16,

        /// Address within bank
        #[arg(short, long, value_parser = parse_hex_or_dec)]
        address: u16,

        /// Data to write (32-bit)
        #[arg(short, long, value_parser = parse_hex_or_dec)]
        data: u32,
    },

    /// Apply phase to node
    ApplyPhase {
        /// Node ID (0-9999)
        #[arg(short, long)]
        node: u16,

        /// Phase angle in radians (0.0-6.283)
        #[arg(short, long)]
        angle: f32,
    },

    /// Measure node conductance
    Measure {
        /// Node ID (0-9999)
        #[arg(short, long)]
        node: u16,
    },

    /// Evolve system
    Evolve {
        /// Evolution parameter alpha
        #[arg(short, long, default_value_t = 1.2)]
        alpha: f32,

        /// Number of evolution steps
        #[arg(short, long, default_value_t = 1)]
        steps: u8,
    },

    /// Synchronize time states
    Sync,

    /// Get current time bank
    GetTimeBank,

    /// Jump to specific time bank
    JumpTimeBank {
        /// Target time bank (0-999)
        #[arg(short, long)]
        bank: u16,
    },

    /// Reset chip
    Reset,

    /// List available serial ports
    ListPorts,
}

/// Parse hex (0x prefix) or decimal number
fn parse_hex_or_dec(s: &str) -> Result<u32, String> {
    if s.starts_with("0x") || s.starts_with("0X") {
        u32::from_str_radix(&s[2..], 16).map_err(|e| e.to_string())
    } else {
        s.parse::<u32>().map_err(|e| e.to_string())
    }
}

/// Main entry point
fn main() -> Result<()> {
    let cli = Cli::parse();

    // Handle list-ports command separately (doesn't need a port)
    if let Commands::ListPorts = cli.command {
        list_ports()?;
        return Ok(());
    }

    // Open serial port
    let mut port = open_port(&cli.port, cli.baud)
        .with_context(|| format!("Failed to open port {}", cli.port))?;

    if cli.verbose {
        println!("Connected to {} @ {} baud", cli.port, cli.baud);
    }

    // Execute command
    let result = execute_command(&mut port, &cli.command, cli.verbose);

    if let Err(ref e) = result {
        eprintln!("Error: {}", e);
    }

    result
}

/// Open serial port
fn open_port(port_name: &str, baud: u32) -> Result<Box<dyn SerialPort>> {
    let port = serialport::new(port_name, baud)
        .timeout(TIMEOUT)
        .open()
        .with_context(|| format!("Failed to open {}", port_name))?;

    Ok(port)
}

/// List available serial ports
fn list_ports() -> Result<()> {
    let ports = serialport::available_ports()
        .context("Failed to list serial ports")?;

    if ports.is_empty() {
        println!("No serial ports found.");
        return Ok(());
    }

    println!("Available serial ports:");
    println!("{:<20} {:<20} {:?}", "Port", "Type", "Description");
    println!("{}", "-".repeat(70));

    for port in ports {
        let port_type = match &port.port_type {
            SerialPortType::UsbPort(info) => {
                format!("USB {:04X}:{:04X}", info.vid, info.pid)
            }
            SerialPortType::PciPort => "PCI".to_string(),
            SerialPortType::BluetoothPort => "Bluetooth".to_string(),
            _ => "Unknown".to_string(),
        };

        println!("{:<20} {:<20}", port.port_name, port_type);
    }

    Ok(())
}

/// Execute a command
fn execute_command(
    port: &mut Box<dyn SerialPort>,
    command: &Commands,
    verbose: bool,
) -> Result<()> {
    match command {
        Commands::Status => cmd_status(port, verbose),
        Commands::Read { bank, address } => cmd_read(port, *bank, *address, verbose),
        Commands::Write { bank, address, data } => cmd_write(port, *bank, *address, *data, verbose),
        Commands::ApplyPhase { node, angle } => cmd_apply_phase(port, *node, *angle, verbose),
        Commands::Measure { node } => cmd_measure(port, *node, verbose),
        Commands::Evolve { alpha, steps } => cmd_evolve(port, *alpha, *steps, verbose),
        Commands::Sync => cmd_sync(port, verbose),
        Commands::GetTimeBank => cmd_get_time_bank(port, verbose),
        Commands::JumpTimeBank { bank } => cmd_jump_time_bank(port, *bank, verbose),
        Commands::Reset => cmd_reset(port, verbose),
        Commands::ListPorts => unreachable!(), // Handled separately
    }
}

// --- Command implementations ---

fn cmd_status(port: &mut Box<dyn SerialPort>, verbose: bool) -> Result<()> {
    let cmd = CommandPacket::new(CommandId::GetStatus, 0, 0);
    
    if verbose {
        println!("Sending status request...");
    }

    let response = send_command(port, &cmd)?;

    if response.status == StatusCode::Ok {
        // Parse status data
        if response.data_len >= 4 {
            let version = ((response.data[0] as u16) << 8) | (response.data[1] as u16);
            let state = response.data[2];
            let flags = response.data[3];

            println!("Arcturus Chip Status:");
            println!("  Firmware version: {}.{}.{}", 
                (version >> 12) & 0xF, (version >> 8) & 0xF, version & 0xFF);
            println!("  State: {} (0x{:02X})", state_name(state), state);
            println!("  Flags: 0x{:02X}", flags);
        } else {
            println!("Status: OK (data format unknown)");
        }
    } else {
        println!("Error: {:?}", response.status);
    }

    Ok(())
}

fn cmd_read(port: &mut Box<dyn SerialPort>, bank: u16, address: u16, verbose: bool) -> Result<()> {
    let param1 = ((bank >> 8) & 0xFF) as u8;
    let param2 = (bank & 0xFF) as u8;
    let addr_bytes = address.to_le_bytes();

    let cmd = CommandPacket::new(CommandId::Read, param1, param2)
        .with_data(&addr_bytes);

    if verbose {
        println!("Reading bank={}, address=0x{:04X}", bank, address);
    }

    let response = send_command(port, &cmd)?;

    if response.status == StatusCode::Ok && response.data_len >= 4 {
        let data = u32::from_le_bytes([
            response.data[0], response.data[1],
            response.data[2], response.data[3]
        ]);
        println!("Read: 0x{:08X} ({})", data, data);
    } else {
        println!("Read failed: {:?}", response.status);
    }

    Ok(())
}

fn cmd_write(port: &mut Box<dyn SerialPort>, bank: u16, address: u16, data: u32, verbose: bool) -> Result<()> {
    let param1 = ((bank >> 8) & 0xFF) as u8;
    let param2 = (bank & 0xFF) as u8;
    
    let addr_bytes = address.to_le_bytes();
    let data_bytes = data.to_le_bytes();
    
    let mut payload = [0u8; 6];
    payload[0..2].copy_from_slice(&addr_bytes);
    payload[2..6].copy_from_slice(&data_bytes);

    let cmd = CommandPacket::new(CommandId::Write, param1, param2)
        .with_data(&payload);

    if verbose {
        println!("Writing 0x{:08X} to bank={}, address=0x{:04X}", data, bank, address);
    }

    let response = send_command(port, &cmd)?;

    if response.status == StatusCode::Ok {
        println!("Write successful");
    } else {
        println!("Write failed: {:?}", response.status);
    }

    Ok(())
}

fn cmd_apply_phase(port: &mut Box<dyn SerialPort>, node: u16, angle: f32, verbose: bool) -> Result<()> {
    let node_high = ((node >> 8) & 0xFF) as u8;
    let node_low = (node & 0xFF) as u8;

    // Convert angle to fixed-point (0-2π maps to 0-65535)
    let angle_fixed = ((angle / (2.0 * std::f32::consts::PI)) * 65535.0) as u16;
    let angle_bytes = angle_fixed.to_le_bytes();

    let cmd = CommandPacket::new(CommandId::ApplyPhase, node_high, node_low)
        .with_data(&angle_bytes);

    if verbose {
        println!("Applying phase {} rad to node {}", angle, node);
    }

    let response = send_command(port, &cmd)?;

    if response.status == StatusCode::Ok {
        println!("Phase applied successfully");
    } else {
        println!("Phase application failed: {:?}", response.status);
    }

    Ok(())
}

fn cmd_measure(port: &mut Box<dyn SerialPort>, node: u16, verbose: bool) -> Result<()> {
    let node_high = ((node >> 8) & 0xFF) as u8;
    let node_low = (node & 0xFF) as u8;

    let cmd = CommandPacket::new(CommandId::MeasureConductance, node_high, node_low);

    if verbose {
        println!("Measuring conductance at node {}", node);
    }

    let response = send_command(port, &cmd)?;

    if response.status == StatusCode::Ok && response.data_len >= 2 {
        let measurement = u16::from_le_bytes([response.data[0], response.data[1]]);
        println!("Conductance at node {}: {} (0x{:04X})", node, measurement, measurement);
    } else {
        println!("Measurement failed: {:?}", response.status);
    }

    Ok(())
}

fn cmd_evolve(port: &mut Box<dyn SerialPort>, alpha: f32, steps: u8, verbose: bool) -> Result<()> {
    let alpha_fixed = (alpha * 65535.0 / 2.0) as u16; // Map 0-2 to 0-65535
    let alpha_high = ((alpha_fixed >> 8) & 0xFF) as u8;
    let alpha_low = (alpha_fixed & 0xFF) as u8;

    let cmd = CommandPacket::new(CommandId::Evolve, alpha_high, alpha_low)
        .with_data(&[steps]);

    if verbose {
        println!("Evolving system: alpha={}, steps={}", alpha, steps);
    }

    let start = Instant::now();
    let response = send_command(port, &cmd)?;
    let elapsed = start.elapsed();

    if response.status == StatusCode::Ok {
        println!("Evolution complete in {:?}", elapsed);
    } else {
        println!("Evolution failed: {:?}", response.status);
    }

    Ok(())
}

fn cmd_sync(port: &mut Box<dyn SerialPort>, verbose: bool) -> Result<()> {
    let cmd = CommandPacket::new(CommandId::SyncTime, 0, 0);

    if verbose {
        println!("Synchronizing time states...");
    }

    let response = send_command(port, &cmd)?;

    if response.status == StatusCode::Ok {
        println!("Time synchronization complete");
    } else {
        println!("Synchronization failed: {:?}", response.status);
    }

    Ok(())
}

fn cmd_get_time_bank(port: &mut Box<dyn SerialPort>, _verbose: bool) -> Result<()> {
    let cmd = CommandPacket::new(CommandId::GetTimeBank, 0, 0);

    let response = send_command(port, &cmd)?;

    if response.status == StatusCode::Ok && response.data_len >= 2 {
        let bank = u16::from_le_bytes([response.data[0], response.data[1]]);
        println!("Current time bank: {}", bank);
    } else {
        println!("Failed to get time bank: {:?}", response.status);
    }

    Ok(())
}

fn cmd_jump_time_bank(port: &mut Box<dyn SerialPort>, bank: u16, verbose: bool) -> Result<()> {
    let bank_high = ((bank >> 8) & 0xFF) as u8;
    let bank_low = (bank & 0xFF) as u8;

    let cmd = CommandPacket::new(CommandId::JumpTimeBank, bank_high, bank_low);

    if verbose {
        println!("Jumping to time bank {}", bank);
    }

    let response = send_command(port, &cmd)?;

    if response.status == StatusCode::Ok {
        println!("Time jump complete");
    } else {
        println!("Time jump failed: {:?}", response.status);
    }

    Ok(())
}

fn cmd_reset(port: &mut Box<dyn SerialPort>, verbose: bool) -> Result<()> {
    let cmd = CommandPacket::new(CommandId::Reset, 0, 0);

    if verbose {
        println!("Resetting chip...");
    }

    let response = send_command(port, &cmd)?;

    if response.status == StatusCode::Ok {
        println!("Chip reset complete");
    } else {
        println!("Reset failed: {:?}", response.status);
    }

    Ok(())
}

/// Send command to device and get response
fn send_command(port: &mut Box<dyn SerialPort>, cmd: &CommandPacket) -> Result<ResponsePacket> {
    // Serialize command
    let mut cmd_buf = [0u8; 260];
    let cmd_len = cmd.serialize(&mut cmd_buf);

    // Send command
    port.write_all(&cmd_buf[..cmd_len])
        .context("Failed to write command")?;

    // Read response (8 bytes fixed)
    let mut resp_buf = [0u8; 8];
    let bytes_read = port.read(&mut resp_buf)
        .context("Failed to read response")?;

    if bytes_read < 2 {
        anyhow::bail!("Incomplete response received");
    }

    // Parse response
    let status = match resp_buf[0] {
        0x00 => StatusCode::Ok,
        0x01 => StatusCode::Error,
        0x02 => StatusCode::InvalidCommand,
        0x03 => StatusCode::InvalidParameter,
        0x04 => StatusCode::MemoryError,
        0x05 => StatusCode::ComputeError,
        0x06 => StatusCode::IoError,
        0x07 => StatusCode::Timeout,
        0x08 => StatusCode::Busy,
        0x09 => StatusCode::NotImplemented,
        _ => StatusCode::Error,
    };

    let data_len = resp_buf[1].min(MAX_RESPONSE_DATA_LEN as u8);
    let mut data = [0u8; MAX_RESPONSE_DATA_LEN];
    data[..data_len as usize].copy_from_slice(&resp_buf[2..2 + data_len as usize]);

    Ok(ResponsePacket {
        status,
        data,
        data_len,
    })
}

/// Get status code name
fn state_name(state: u8) -> &'static str {
    match state {
        0 => "Initializing",
        1 => "Idle",
        2 => "Computing",
        3 => "MemoryAccess",
        4 => "IoOperation",
        5 => "Error",
        _ => "Unknown",
    }
}
