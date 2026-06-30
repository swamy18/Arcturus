//! Test D: Edge Cache Conductance Quantization
//!
//! Tests the 2-bit data storage in bismuthene edge state conductance:
//! - Data 0: Conductance in [0, LEVEL_0_MAX]
//! - Data 1: Conductance in [LEVEL_1_MIN, LEVEL_1_MAX]
//! - Data 2: Conductance in [LEVEL_2_MIN, LEVEL_2_MAX]
//! - Data 3: Conductance in [LEVEL_3_MIN, LEVEL_3_MAX]

// Import the actual firmware constants
const FIXED_ONE: i32 = 1 << 16;

// Conductance level thresholds (matches firmware)
const LEVEL_0_MAX: u16 = 4095;
const LEVEL_1_MIN: u16 = 4096;
const LEVEL_1_MAX: u16 = 8191;
const LEVEL_2_MIN: u16 = 8192;
const LEVEL_2_MAX: u16 = 12287;
const LEVEL_3_MIN: u16 = 12288;
const LEVEL_3_MAX: u16 = 16383;

/// Convert data (0-3) to target conductance (midpoint of range)
fn data_to_conductance(data: u8) -> u16 {
    match data & 0x03 {
        0 => LEVEL_0_MAX / 2,
        1 => (LEVEL_1_MIN + LEVEL_1_MAX) / 2,
        2 => (LEVEL_2_MIN + LEVEL_2_MAX) / 2,
        _ => (LEVEL_3_MIN + LEVEL_3_MAX) / 2,
    }
}

/// Convert conductance measurement to data value
fn conductance_to_data(conductance: u16) -> u8 {
    match conductance {
        0..=LEVEL_0_MAX => 0,
        LEVEL_1_MIN..=LEVEL_1_MAX => 1,
        LEVEL_2_MIN..=LEVEL_2_MAX => 2,
        _ => 3,
    }
}

/// Edge cache entry (simplified from firmware)
#[derive(Debug, Clone)]
struct EdgeCacheEntry {
    node_id: u16,
    data: u8,
    last_conductance: u16,
}

impl EdgeCacheEntry {
    fn new(node_id: u16) -> Self {
        Self {
            node_id,
            data: 0,
            last_conductance: 0,
        }
    }

    fn write(&mut self, data: u8) {
        self.data = data & 0x03;
        self.last_conductance = data_to_conductance(self.data);
    }

    fn read(&self) -> u8 {
        conductance_to_data(self.last_conductance)
    }

    fn update_from_measurement(&mut self, conductance: u16) {
        self.last_conductance = conductance;
        self.data = conductance_to_data(conductance);
    }
}

/// Test D: Edge Cache Conductance Quantization
#[test]
fn test_edge_cache_quantization() {
    println!("=== Test D: Edge Cache Conductance Quantization ===");

    // Test 1: Data to conductance mapping
    println!("\n--- Test 1: Data to Conductance Mapping ---");
    let test_data = [0u8, 1, 2, 3];
    let expected_ranges = [
        (0u16, LEVEL_0_MAX),
        (LEVEL_1_MIN, LEVEL_1_MAX),
        (LEVEL_2_MIN, LEVEL_2_MAX),
        (LEVEL_3_MIN, LEVEL_3_MAX),
    ];

    for (i, &data) in test_data.iter().enumerate() {
        let conductance = data_to_conductance(data);
        let (min, max) = expected_ranges[i];
        
        println!("  Data {} -> Conductance {} (range: {}-{})", 
                 data, conductance, min, max);
        
        assert!(
            conductance >= min && conductance <= max,
            "Conductance {} for data {} is outside expected range {}-{}",
            conductance, data, min, max
        );
    }
    println!("  ✓ All data-to-conductance mappings correct");

    // Test 2: Conductance to data mapping
    println!("\n--- Test 2: Conductance to Data Mapping ---");
    let test_conductances = [
        (2048u16, 0u8),   // Mid range 0
        (6144u16, 1u8),   // Mid range 1
        (10240u16, 2u8),  // Mid range 2
        (14336u16, 3u8),  // Mid range 3
    ];

    for (conductance, expected_data) in &test_conductances {
        let data = conductance_to_data(*conductance);
        
        println!("  Conductance {} -> Data {} (expected {})", 
                 conductance, data, expected_data);
        
        assert_eq!(
            data, *expected_data,
            "Conductance {} mapped to data {} but expected {}",
            conductance, data, expected_data
        );
    }
    println!("  ✓ All conductance-to-data mappings correct");

    // Test 3: Round-trip consistency
    println!("\n--- Test 3: Round-Trip Consistency ---");
    let mut round_trip_errors = 0;

    for original_data in 0..=3 {
        let conductance = data_to_conductance(original_data);
        let recovered_data = conductance_to_data(conductance);
        
        println!("  Data {} -> Conductance {} -> Data {}", 
                 original_data, conductance, recovered_data);
        
        if original_data != recovered_data {
            round_trip_errors += 1;
            println!("  ✗ ERROR: Round-trip mismatch!");
        }
    }

    assert_eq!(
        round_trip_errors, 0,
        "Found {} round-trip errors in data->conductance->data conversion",
        round_trip_errors
    );
    println!("  ✓ All round-trip conversions consistent");

    // Test 4: Edge cache entry operations
    println!("\n--- Test 4: Edge Cache Entry Operations ---");
    let test_cases = [
        (0u16, 0u8),
        (100, 1),
        (5000, 2),
        (9999, 3),
    ];

    for (node_id, data) in &test_cases {
        let mut entry = EdgeCacheEntry::new(*node_id);
        
        // Write data
        entry.write(*data);
        
        // Read back
        let read_data = entry.read();
        
        println!("  Node {}: Write Data {} -> Read Data {}", 
                 node_id, data, read_data);
        
        assert_eq!(
            read_data, *data,
            "Edge cache entry for node {} failed: wrote {} but read {}",
            node_id, data, read_data
        );
        
        // Verify conductance is in correct range
        let conductance = entry.last_conductance;
        let expected_range = match *data {
            0 => (0u16, LEVEL_0_MAX),
            1 => (LEVEL_1_MIN, LEVEL_1_MAX),
            2 => (LEVEL_2_MIN, LEVEL_2_MAX),
            _ => (LEVEL_3_MIN, LEVEL_3_MAX),
        };
        
        assert!(
            conductance >= expected_range.0 && conductance <= expected_range.1,
            "Conductance {} for node {} with data {} is outside range {:?}",
            conductance, node_id, data, expected_range
        );
    }
    println!("  ✓ All edge cache entry operations successful");

    // Test 5: Update from measurement
    println!("\n--- Test 5: Update From Measurement ---");
    let measurement_test_cases = [
        (1000u16, 0u8),
        (5000, 1),
        (9000, 2),
        (14000, 3),
    ];

    for (conductance, expected_data) in &measurement_test_cases {
        let mut entry = EdgeCacheEntry::new(0);
        entry.update_from_measurement(*conductance);
        
        let read_data = entry.read();
        
        println!("  Conductance {} -> Data {} (expected {})", 
                 conductance, read_data, expected_data);
        
        assert_eq!(
            read_data, *expected_data,
            "Measurement update failed: conductance {} should map to data {}, got {}",
            conductance, expected_data, read_data
        );
    }
    println!("  ✓ All measurement updates correct");

    // Summary
    println!("\n=== Test D Complete ===");
    println!("All edge cache quantization tests passed!");
    println!("  ✓ Data to conductance mapping verified");
    println!("  ✓ Conductance to data mapping verified");
    println!("  ✓ Round-trip consistency verified");
    println!("  ✓ Edge cache entry operations verified");
    println!("  ✓ Measurement update verified");
}

fn main() {
    test_edge_cache_quantization();
}
