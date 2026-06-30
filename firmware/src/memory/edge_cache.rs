//! Edge State Cache (L1 Cache)
//!
//! Uses bismuthene edge state conductance quantization as fast local storage.
//! Each node can store 2 bits of data encoded in 4 quantized conductance states.
//!
//! Physical basis:
//! - Bismuthene exhibits topological edge states with quantized conductance
//! - Conductance values: G = (n * e²/h) where n is the mode number
//! - We encode data in the mode number: n = 0, 1, 2, 3

use super::{MemoryBank, Fixed, float_to_fixed};
use heapless::Vec;

/// Number of conductance quantization levels (2 bits per node)
pub const CONDUCTANCE_LEVELS: u8 = 4;

/// Conductance level thresholds (arbitrary units)
/// These would be calibrated to actual bismuthene quantization levels
pub const LEVEL_THRESHOLDS: [u16; 4] = [
    0,      // Level 0: 0 - 4095
    4096,   // Level 1: 4096 - 8191
    8192,   // Level 2: 8192 - 12287
    12288,  // Level 3: 12288 - 16383
];

/// Level 0: No edge state (insulating)
pub const LEVEL_0_MAX: u16 = 4095;

/// Level 1: Single edge mode (G = e²/h)
pub const LEVEL_1_MIN: u16 = 4096;
pub const LEVEL_1_MAX: u16 = 8191;

/// Level 2: Two edge modes (G = 2e²/h)
pub const LEVEL_2_MIN: u16 = 8192;
pub const LEVEL_2_MAX: u16 = 12287;

/// Level 3: Three edge modes (G = 3e²/h)
pub const LEVEL_3_MIN: u16 = 12288;
pub const LEVEL_3_MAX: u16 = 16383;

/// Cached data entry for a single node
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EdgeCacheEntry {
    /// Node ID (0-9999)
    pub node_id: u16,
    /// Cached 2-bit data value (0-3)
    pub data: u8,
    /// Last measured conductance value (raw ADC)
    pub last_conductance: u16,
    /// Timestamp of last access
    pub last_access: u32,
    /// Valid bit (entry contains data)
    pub valid: bool,
    /// Dirty bit (needs writeback)
    pub dirty: bool,
}

impl EdgeCacheEntry {
    /// Create a new empty cache entry
    pub fn new(node_id: u16) -> Self {
        Self {
            node_id,
            data: 0,
            last_conductance: 0,
            last_access: 0,
            valid: false,
            dirty: false,
        }
    }

    /// Update from measured conductance
    pub fn update_from_conductance(&mut self, conductance: u16, timestamp: u32) {
        self.last_conductance = conductance;
        self.data = Self::conductance_to_data(conductance);
        self.last_access = timestamp;
        self.valid = true;
    }

    /// Get target conductance for current data value
    pub fn target_conductance(&self) -> u16 {
        Self::data_to_conductance(self.data)
    }

    /// Convert conductance measurement to 2-bit data
    pub fn conductance_to_data(conductance: u16) -> u8 {
        match conductance {
            0..=LEVEL_0_MAX => 0,
            LEVEL_1_MIN..=LEVEL_1_MAX => 1,
            LEVEL_2_MIN..=LEVEL_2_MAX => 2,
            _ => 3,
        }
    }

    /// Convert 2-bit data to target conductance (midpoint of range)
    pub fn data_to_conductance(data: u8) -> u16 {
        match data & 0x03 {
            0 => LEVEL_0_MAX / 2,
            1 => (LEVEL_1_MIN + LEVEL_1_MAX) / 2,
            2 => (LEVEL_2_MIN + LEVEL_2_MAX) / 2,
            _ => (LEVEL_3_MIN + LEVEL_3_MAX) / 2,
        }
    }

    /// Mark entry as dirty (needs writeback)
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Mark entry as clean (written back)
    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    /// Invalidate entry
    pub fn invalidate(&mut self) {
        self.valid = false;
        self.dirty = false;
    }
}

/// L1 Edge Cache
/// Direct-mapped cache with configurable number of entries
pub struct EdgeCache<const NUM_ENTRIES: usize> {
    /// Cache entries (direct-mapped or fully associative)
    entries: Vec<EdgeCacheEntry, NUM_ENTRIES>,
    /// Global timestamp counter
    timestamp: u32,
    /// Cache statistics
    stats: CacheStats,
}

/// Cache performance statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct CacheStats {
    /// Number of cache hits
    pub hits: u32,
    /// Number of cache misses
    pub misses: u32,
    /// Number of writebacks
    pub writebacks: u32,
    /// Number of invalidations
    pub invalidations: u32,
}

impl CacheStats {
    /// Calculate hit rate as percentage
    pub fn hit_rate(&self) -> f32 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            (self.hits as f32 / total as f32) * 100.0
        }
    }

    /// Reset all statistics
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

impl<const NUM_ENTRIES: usize> EdgeCache<NUM_ENTRIES> {
    /// Create a new edge cache
    pub fn new() -> Self {
        let mut cache = Self {
            entries: Vec::new(),
            timestamp: 0,
            stats: CacheStats::default(),
        };

        // Initialize with empty entries
        for i in 0..NUM_ENTRIES.min(10000) {
            let _ = cache.entries.push(EdgeCacheEntry::new(i as u16));
        }

        cache
    }

    /// Get current timestamp
    pub fn timestamp(&self) -> u32 {
        self.timestamp
    }

    /// Increment timestamp
    pub fn tick(&mut self) {
        self.timestamp = self.timestamp.wrapping_add(1);
    }

    /// Look up a cache entry by node ID
    /// Returns (index, entry) if found and valid
    pub fn lookup(&self, node_id: u16) -> Option<(usize, &EdgeCacheEntry)> {
        // Simple direct-mapped: index = node_id % NUM_ENTRIES
        let index = (node_id as usize) % NUM_ENTRIES;
        
        if let Some(entry) = self.entries.get(index) {
            if entry.valid && entry.node_id == node_id {
                return Some((index, entry));
            }
        }
        None
    }

    /// Read data from cache
    pub fn read(&mut self, node_id: u16) -> Option<u8> {
        if let Some((_, data)) = self.lookup(node_id).map(|(index, entry)| (index, entry.data)) {
            self.stats.hits += 1;
            Some(data)
        } else {
            self.stats.misses += 1;
            None
        }
    }

    /// Write data to cache
    pub fn write(&mut self, node_id: u16, data: u8) -> Result<(), ()> {
        let index = (node_id as usize) % NUM_ENTRIES;
        
        // Check if we need to writeback existing entry
        if let Some(existing) = self.entries.get(index) {
            if existing.valid && existing.dirty && existing.node_id != node_id {
                // Would need to writeback here
                self.stats.writebacks += 1;
            }
        }

        // Update or create entry
        let entry = EdgeCacheEntry {
            node_id,
            data: data & 0x03, // Only 2 bits
            last_conductance: 0,
            last_access: self.timestamp,
            valid: true,
            dirty: true,
        };

        if index < self.entries.len() {
            self.entries[index] = entry;
        } else if self.entries.push(entry).is_err() {
            return Err(());
        }

        Ok(())
    }

    /// Invalidate an entry
    pub fn invalidate(&mut self, node_id: u16) -> bool {
        let index = (node_id as usize) % NUM_ENTRIES;
        
        if let Some(entry) = self.entries.get_mut(index) {
            if entry.node_id == node_id {
                entry.invalidate();
                self.stats.invalidations += 1;
                return true;
            }
        }
        false
    }

    /// Flush all dirty entries (writeback)
    pub fn flush(&mut self) -> u32 {
        let mut count = 0u32;
        for entry in &mut self.entries {
            if entry.valid && entry.dirty {
                // Perform writeback to physical node
                // This would trigger actual phase injection/measurement
                entry.mark_clean();
                count += 1;
            }
        }
        self.stats.writebacks += count;
        count
    }

    /// Get cache statistics
    pub fn stats(&self) -> &CacheStats {
        &self.stats
    }

    /// Reset cache statistics
    pub fn reset_stats(&mut self) {
        self.stats.reset();
    }

    /// Get number of valid entries
    pub fn valid_entries(&self) -> usize {
        self.entries.iter().filter(|e| e.valid).count()
    }

    /// Get number of dirty entries
    pub fn dirty_entries(&self) -> usize {
        self.entries.iter().filter(|e| e.valid && e.dirty).count()
    }
}

impl<const NUM_ENTRIES: usize> Default for EdgeCache<NUM_ENTRIES> {
    fn default() -> Self {
        Self::new()
    }
}

/// Bulk edge cache operations for high-throughput access
pub struct EdgeCacheBulk<const NUM_ENTRIES: usize> {
    /// Underlying cache
    cache: EdgeCache<NUM_ENTRIES>,
    /// Pending write buffer
    write_buffer: Vec<(u16, u8), 64>,
    /// Read prefetch queue
    prefetch_queue: Vec<u16, 16>,
}

impl<const NUM_ENTRIES: usize> EdgeCacheBulk<NUM_ENTRIES> {
    /// Create a new bulk cache interface
    pub fn new() -> Self {
        Self {
            cache: EdgeCache::new(),
            write_buffer: Vec::new(),
            prefetch_queue: Vec::new(),
        }
    }

    /// Queue a write operation
    pub fn queue_write(&mut self, node_id: u16, data: u8) -> Result<(), ()> {
        self.write_buffer.push((node_id, data)).map_err(|_| ())
    }

    /// Queue a prefetch operation
    pub fn queue_prefetch(&mut self, node_id: u16) -> Result<(), ()> {
        self.prefetch_queue.push(node_id).map_err(|_| ())
    }

    /// Flush all pending writes
    pub fn flush_writes(&mut self) -> usize {
        let mut count = 0;
        for (node_id, data) in &self.write_buffer {
            if self.cache.write(*node_id, *data).is_ok() {
                count += 1;
            }
        }
        self.write_buffer.clear();
        count
    }

    /// Execute prefetches
    pub fn execute_prefetches(&mut self) -> usize {
        let mut count = 0;
        for node_id in &self.prefetch_queue {
            // Trigger cache read (which may load from memory)
            if self.cache.read(*node_id).is_some() {
                count += 1;
            }
        }
        self.prefetch_queue.clear();
        count
    }

    /// Get cache reference
    pub fn cache(&self) -> &EdgeCache<NUM_ENTRIES> {
        &self.cache
    }

    /// Get mutable cache reference
    pub fn cache_mut(&mut self) -> &mut EdgeCache<NUM_ENTRIES> {
        &mut self.cache
    }
}

impl<const NUM_ENTRIES: usize> Default for EdgeCacheBulk<NUM_ENTRIES> {
    fn default() -> Self {
        Self::new()
    }
}
