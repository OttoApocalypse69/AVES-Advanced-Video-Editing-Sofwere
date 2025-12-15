//! Seek-based frame cache for smooth scrubbing.
//! Caches frames around the current playhead position.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use crate::core::time::Time;
use crate::decode::decoder::VideoFrame;

/// Cache key: (source_path, timestamp in nanoseconds)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    source_path: PathBuf,
    timestamp: Time,
}

/// Seek-based frame cache
/// Maintains a window of frames around the current playhead
pub struct FrameCache {
    cache: HashMap<CacheKey, VideoFrame>,
    cache_window_size: Time,  // Time window to cache on each side of playhead (nanoseconds)
    max_cache_size: usize,    // Maximum total frames to cache
}

impl FrameCache {
    /// Create a new frame cache
    pub fn new(cache_window_nanos: Time, max_cache_size: usize) -> Self {
        Self {
            cache: HashMap::new(),
            cache_window_size: cache_window_nanos,
            max_cache_size,
        }
    }

    /// Create a frame cache with default settings (±1 second window, max 1000 frames)
    pub fn default() -> Self {
        Self::new(
            crate::core::time::constants::NANOS_PER_SECOND, // 1 second window
            1000
        )
    }

    /// Get a frame from the cache
    pub fn get(&self, source_path: &Path, timestamp: Time) -> Option<&VideoFrame> {
        let key = CacheKey {
            source_path: source_path.to_path_buf(),
            timestamp,
        };
        self.cache.get(&key)
    }

    /// Insert a frame into the cache
    pub fn insert(&mut self, source_path: PathBuf, frame: VideoFrame) {
        let key = CacheKey {
            source_path,
            timestamp: frame.timestamp,
        };

        // If cache is full, evict oldest entries (simple FIFO for now)
        // In a more sophisticated implementation, we'd use LRU
        if self.cache.len() >= self.max_cache_size {
            self.evict_oldest();
        }

        self.cache.insert(key, frame);
    }

    /// Evict the oldest entries from the cache
    /// Simple implementation: remove a portion of the cache
    fn evict_oldest(&mut self) {
        // Remove 25% of the cache
        let to_remove = self.cache.len() / 4;
        let keys: Vec<_> = self.cache.keys().take(to_remove).cloned().collect();
        for key in keys {
            self.cache.remove(&key);
        }
    }

    /// Get the cache window around a specific time
    /// Returns the range of timestamps that should be cached
    pub fn cache_window(&self, playhead_time: Time) -> (Time, Time) {
        let start = playhead_time.saturating_sub(self.cache_window_size);
        let end = playhead_time.saturating_add(self.cache_window_size);
        (start, end)
    }

    /// Check if a timestamp is within the cache window around a playhead position
    pub fn is_in_window(&self, timestamp: Time, playhead_time: Time) -> bool {
        let (start, end) = self.cache_window(playhead_time);
        timestamp >= start && timestamp <= end
    }

    /// Clear frames that are outside the cache window around the playhead
    pub fn trim_to_window(&mut self, source_path: &Path, playhead_time: Time) {
        let (start, end) = self.cache_window(playhead_time);
        
        self.cache.retain(|key, _| {
            if key.source_path == source_path {
                key.timestamp >= start && key.timestamp <= end
            } else {
                true  // Keep frames from other sources
            }
        });
    }

    /// Clear all cached frames
    pub fn clear(&mut self) {
        self.cache.clear();
    }

    /// Get the number of cached frames
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Check if the cache is empty
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::time;

    fn create_test_frame(timestamp: Time) -> VideoFrame {
        VideoFrame {
            data: vec![0; 100],
            width: 10,
            height: 10,
            timestamp,
        }
    }

    #[test]
    fn test_cache_insert_get() {
        let mut cache = FrameCache::new(time::from_seconds(1.0), 100);
        let path = PathBuf::from("test.mp4");
        let timestamp = time::from_seconds(5.0);
        
        let frame = create_test_frame(timestamp);

        cache.insert(path.clone(), frame.clone());
        assert!(cache.get(&path, timestamp).is_some());
        assert!(cache.get(&path, time::from_seconds(6.0)).is_none());
    }

    #[test]
    fn test_cache_window() {
        let cache = FrameCache::new(time::from_seconds(1.0), 1000);
        let playhead = time::from_seconds(10.0);
        let (start, end) = cache.cache_window(playhead);
        
        assert_eq!(start, time::from_seconds(9.0));
        assert_eq!(end, time::from_seconds(11.0));
    }

    #[test]
    fn test_is_in_window() {
        let cache = FrameCache::new(time::from_seconds(1.0), 1000);
        let playhead = time::from_seconds(10.0);
        
        assert!(cache.is_in_window(time::from_seconds(10.0), playhead));
        assert!(cache.is_in_window(time::from_seconds(10.5), playhead));
        assert!(cache.is_in_window(time::from_seconds(9.5), playhead));
        assert!(!cache.is_in_window(time::from_seconds(8.0), playhead));
        assert!(!cache.is_in_window(time::from_seconds(12.0), playhead));
    }

    #[test]
    fn test_trim_to_window() {
        let mut cache = FrameCache::new(time::from_seconds(1.0), 1000);
        let path = PathBuf::from("test.mp4");
        
        // Insert frames at various timestamps
        for i in 0..20 {
            let frame = create_test_frame(time::from_seconds(i as f64));
            cache.insert(path.clone(), frame);
        }
        
        // Trim to window around 10 seconds
        cache.trim_to_window(&path, time::from_seconds(10.0));
        
        // Only frames from 9-11 seconds should remain
        assert!(cache.get(&path, time::from_seconds(9.5)).is_some());
        assert!(cache.get(&path, time::from_seconds(10.5)).is_some());
        assert!(cache.get(&path, time::from_seconds(5.0)).is_none());
        assert!(cache.get(&path, time::from_seconds(15.0)).is_none());
    }
}
