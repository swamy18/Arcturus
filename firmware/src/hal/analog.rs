//! Analog I/O Driver for Arcturus OS
//!
//! Provides:
//! - Phase injection via DAC (voltage/current controlled)
//! - Conductance measurement via ADC (2-wire or 4-wire)
//! - Temperature monitoring

/// Phase injection angle type (0-65535 maps to 0-2π)
pub type PhaseAngle = u16;

/// Conductance value type (arbitrary units, calibrated)
pub type ConductanceValue = u16;

/// DAC resolution (16-bit)
pub const DAC_RESOLUTION: u8 = 16;

/// ADC resolution (12-bit)
pub const ADC_RESOLUTION: u8 = 12;

/// Minimal DAC write trait used by the phase driver.
///
/// `embedded-hal` 1.0 does not define a generic DAC trait, so the firmware
/// uses this adapter trait for concrete board implementations.
pub trait DacWrite {
    type Word;
    type Error;

    fn write(&mut self, value: Self::Word) -> Result<(), Self::Error>;
}

/// Minimal ADC conversion trait used by the conductance driver.
pub trait AdcRead<PIN> {
    type Word;
    type Error;

    fn read(&mut self, pin: &mut PIN) -> Result<Self::Word, Self::Error>;
}

/// Phase angle conversion: 0-65535 -> 0.0-2π
pub fn phase_to_radians(phase: PhaseAngle) -> f32 {
    (phase as f32 / 65535.0) * 2.0 * core::f32::consts::PI
}

/// Radians to phase angle: 0.0-2π -> 0-65535
pub fn radians_to_phase(radians: f32) -> PhaseAngle {
    let normalized = (radians / (2.0 * core::f32::consts::PI)).rem_euclid(1.0);
    (normalized * 65535.0) as PhaseAngle
}

/// DAC driver for phase injection
pub struct PhaseDac<DAC> {
    dac: DAC,
    current_value: u16,
    vref_mv: f32,
}

impl<DAC> PhaseDac<DAC> {
    /// Create a new phase DAC driver
    pub fn new(dac: DAC, vref_mv: f32) -> Self {
        Self {
            dac,
            current_value: 0,
            vref_mv,
        }
    }

    /// Get the DAC voltage reference
    pub fn vref(&self) -> f32 {
        self.vref_mv
    }

    /// Get the current DAC output value
    pub fn current_value(&self) -> u16 {
        self.current_value
    }

    /// Convert phase angle to DAC output value
    /// Phase 0-65535 maps to full voltage range
    pub fn phase_to_voltage(phase: PhaseAngle, vref_mv: f32) -> f32 {
        let fraction = phase as f32 / 65535.0;
        fraction * vref_mv / 1000.0 // Convert mV to V
    }
}

impl<DAC> PhaseDac<DAC>
where
    DAC: DacWrite<Word = u16>,
{
    /// Set phase injection value (0-65535)
    pub fn set_phase(&mut self, phase: PhaseAngle) -> Result<(), DacError> {
        let dac_value = ((phase as u32 * 65535) / 65535) as u16;
        self.dac.write(dac_value).map_err(|_| DacError::WriteError)?;
        self.current_value = dac_value;
        Ok(())
    }

    /// Set phase in radians
    pub fn set_phase_rad(&mut self, radians: f32) -> Result<(), DacError> {
        let phase = radians_to_phase(radians);
        self.set_phase(phase)
    }

    /// Disable phase injection (set to zero)
    pub fn disable(&mut self) -> Result<(), DacError> {
        self.dac.write(0).map_err(|_| DacError::WriteError)?;
        self.current_value = 0;
        Ok(())
    }
}

/// ADC driver for conductance measurement
pub struct ConductanceAdc<ADC, PIN> {
    adc: ADC,
    pin: PIN,
    vref_mv: f32,
    sense_resistor_ohms: f32,
}

impl<ADC, PIN> ConductanceAdc<ADC, PIN> {
    /// Create a new conductance ADC driver
    pub fn new(adc: ADC, pin: PIN, vref_mv: f32, sense_resistor_ohms: f32) -> Self {
        Self {
            adc,
            pin,
            vref_mv,
            sense_resistor_ohms,
        }
    }

    /// Convert ADC reading to conductance
    /// Using G = I/V = (V_adc / R_sense) / V_excitation
    pub fn adc_to_conductance(&self, adc_raw: u16, adc_max: u16) -> f32 {
        let v_adc = (adc_raw as f32 / adc_max as f32) * self.vref_mv / 1000.0; // V
        let current = v_adc / self.sense_resistor_ohms; // A
        let v_excitation = self.vref_mv / 1000.0; // V (assumed)
        
        // Conductance in Siemens
        let conductance = if v_excitation > 0.0 {
            current / v_excitation
        } else {
            0.0
        };
        
        conductance
    }

    /// Calibrated conductance to quantized value for edge cache
    pub fn quantize_conductance(&self, conductance: f32) -> u8 {
        // Map conductance to 2-bit value (0-3) for edge state cache
        let normalized = (conductance / 1e-6).min(4.0).max(0.0);
        (normalized as u8).min(3)
    }
}

impl<ADC, PIN> ConductanceAdc<ADC, PIN>
where
    ADC: AdcRead<PIN, Word = u16>,
{
    /// Measure conductance at the currently selected node
    pub fn measure(&mut self) -> Result<ConductanceValue, AdcError> {
        let adc_raw = self.adc.read(&mut self.pin).map_err(|_| AdcError::ConversionError)?;
        
        // Convert to conductance value (0-65535 range)
        let conductance_f32 = self.adc_to_conductance(adc_raw, 4095);
        let conductance_u16 = ((conductance_f32 / 1e-3).min(65535.0).max(0.0) as u16)
            .max(0)
            .min(65535);
        
        Ok(conductance_u16)
    }

    /// Measure and return raw ADC value
    pub fn measure_raw(&mut self) -> Result<u16, AdcError> {
        self.adc.read(&mut self.pin).map_err(|_| AdcError::ConversionError)
    }
}

/// DAC errors
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DacError {
    WriteError,
    InvalidValue,
}

/// ADC errors
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AdcError {
    ConversionError,
    Timeout,
    InvalidChannel,
}
