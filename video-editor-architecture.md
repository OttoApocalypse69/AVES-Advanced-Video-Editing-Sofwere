---
name: Video Editor Architecture
overview: Design a clean, modular Rust architecture for a minimal desktop video editor with frame-accurate editing, real-time preview, and proper audio/video synchronization using FFmpeg, wgpu, and cpal.
todos:
  - id: setup-project
    content: Initialize Rust project with Cargo.toml, add dependencies (ffmpeg-next, wgpu, winit, egui, cpal), create module structure
    status: completed
  - id: core-time
    content: Implement core/time.rs with Timebase and TimePoint structs, conversion methods (to_seconds, from_seconds, to_frame_index)
    status: completed
    dependencies:
      - setup-project
  - id: core-clip-track
    content: Implement core/clip.rs and core/track.rs with Clip, Track, and TrackType data structures
    status: completed
    dependencies:
      - core-time
  - id: core-timeline
    content: Implement core/timeline.rs with Timeline struct, methods for adding/removing clips, calculating duration
    status: completed
    dependencies:
      - core-clip-track
  - id: decode-wrapper
    content: Create decode/decoder.rs with safe FFmpeg wrapper, isolate all unsafe code, implement basic frame decoding
    status: completed
    dependencies:
      - core-time
  - id: decode-cache
    content: Implement decode/frame_cache.rs with seek-based caching strategy (±30 frames around playhead)
    status: completed
    dependencies:
      - decode-wrapper
  - id: audio-player
    content: Implement audio/player.rs with cpal integration, basic audio playback from timeline audio track
    status: completed
    dependencies:
      - core-timeline
  - id: render-compositor
    content: Implement render/compositor.rs with wgpu setup, texture management, basic video frame rendering
    status: completed
    dependencies:
      - decode-cache
  - id: playback-engine
    content: Implement playback/engine.rs with state machine, command handling, and coordination between audio/video threads
    status: completed
    dependencies:
      - audio-player
      - render-compositor
  - id: playback-sync
    content: Implement playback/sync.rs with audio-driven timing, atomic clock, video synchronization logic
    status: completed
    dependencies:
      - playback-engine
  - id: export-pipeline
    content: Implement export/encoder.rs and export/pipeline.rs with FFmpeg encoding for H.264 + AAC MP4 export
    status: completed
    dependencies:
      - core-timeline
      - decode-wrapper
---

# Video Editor Architecture Plan

## Module Structure

```
src/
├── lib.rs                 # Library root, re-exports
├── main.rs                # Application entry point
├── core/
│   ├── mod.rs
│   ├── time.rs            # Time representation (TimePoint, Timebase)
│   ├── timeline.rs         # Timeline data structure
│   ├── track.rs            # Track (video/audio) and clip management
│   └── clip.rs             # Clip data structure
├── decode/
│   ├── mod.rs
│   ├── decoder.rs          # FFmpeg decoder wrapper (safe interface)
│   ├── frame_cache.rs      # Seek-based frame cache
│   └── stream_info.rs      # Video/audio stream metadata
├── render/
│   ├── mod.rs
│   ├── compositor.rs       # wgpu-based compositing
│   ├── texture.rs          # GPU texture management
│   └── shader.rs           # Shader compilation (if needed)
├── audio/
│   ├── mod.rs
│   ├── player.rs           # cpal audio playback
│   ├── mixer.rs            # Audio mixing/synchronization
│   └── buffer.rs           # Audio buffer management
├── playback/
│   ├── mod.rs
│   ├── engine.rs           # Main playback coordinator
│   ├── sync.rs             # A/V synchronization logic
│   └── state.rs             # Playback state machine
├── export/
│   ├── mod.rs
│   ├── encoder.rs           # FFmpeg encoder wrapper
│   └── pipeline.rs          # Export pipeline
└── ui/
    ├── mod.rs               # UI module (placeholder for future egui integration)
    └── timeline_view.rs    # Timeline UI (future)
```

## Core Data Structures

### Time Representation (`core/time.rs`)

```rust
// Rational timebase (matches FFmpeg AVRational)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Timebase {
    pub num: i32,  // numerator
    pub den: i32,  // denominator
}

// Frame-accurate time point
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TimePoint {
    pub value: i64,        // timestamp in timebase units
    pub timebase: Timebase,
}

impl TimePoint {
    pub fn to_seconds(&self) -> f64;
    pub fn to_frame_index(&self, fps: f64) -> usize;
    pub fn from_seconds(seconds: f64, timebase: Timebase) -> Self;
}
```

### Clip (`core/clip.rs`)

```rust
#[derive(Debug, Clone)]
pub struct Clip {
    pub id: ClipId,
    pub source_path: PathBuf,
    pub in_point: TimePoint,      // Start time in source
    pub out_point: TimePoint,     // End time in source
    pub timeline_start: TimePoint, // Position on timeline
    pub timeline_end: TimePoint,   // End position on timeline
    pub stream_index: usize,      // Which stream in source file
}
```

### Track (`core/track.rs`)

```rust
#[derive(Debug, Clone)]
pub enum TrackType {
    Video,
    Audio,
}

#[derive(Debug, Clone)]
pub struct Track {
    pub id: TrackId,
    pub track_type: TrackType,
    pub clips: Vec<Clip>,  // Sorted by timeline_start
    pub muted: bool,
    pub volume: f32,       // 0.0 to 1.0
}
```

### Timeline (`core/timeline.rs`)

```rust
#[derive(Debug, Clone)]
pub struct Timeline {
    pub video_track: Track,
    pub audio_track: Track,
    pub timebase: Timebase,        // Timeline timebase (e.g., 1/1000 for ms precision)
    pub duration: TimePoint,       // Total timeline duration
    pub playhead: TimePoint,       // Current playhead position
}
```

## Playback Pipeline Architecture

### Threading Model

The playback system uses three main threads:

1. **Audio Thread** (cpal callback) - Drives timing

   - Runs at fixed sample rate (e.g., 48kHz)
   - Requests audio samples for current playback time
   - Signals video thread via atomic timestamp

2. **Video Thread** (dedicated render thread)

   - Polls audio timestamp atomically
   - Decodes video frames as needed
   - Renders to wgpu surface
   - Syncs to audio (may drop/repeat frames if behind/ahead)

3. **Main Thread** (winit event loop)

   - Handles UI (egui) and user input
   - Updates timeline state
   - Sends commands to playback engine

### Data Flow

```
┌─────────────┐
│  Main Thread│
│  (winit)    │
└──────┬──────┘
       │ Commands (play, seek, etc.)
       ▼
┌─────────────────┐
│ Playback Engine │
│  (playback/)    │
└────┬───────┬────┘
     │       │
     │       └──────────────┐
     │                      │
     ▼                      ▼
┌──────────┐         ┌──────────┐
│  Decoder │         │  Audio   │
│  Thread  │         │  Thread  │
│          │         │  (cpal)  │
└────┬─────┘         └────┬─────┘
     │                    │
     │ Frames             │ Audio samples
     │                    │
     ▼                    ▼
┌──────────┐         ┌──────────┐
│  Frame   │         │  Audio   │
│  Cache   │         │  Mixer   │
└────┬─────┘         └────┬─────┘
     │                    │
     │                    │
     ▼                    ▼
┌──────────┐         ┌──────────┐
│ Compositor│         │  cpal   │
│  (wgpu)  │         │  output │
└──────────┘         └──────────┘
```

### Synchronization Strategy

- **Audio-driven timing**: Audio thread maintains master clock via `AtomicU64` (nanoseconds since start)
- **Video sync**: Video thread reads audio clock, decodes frame for that timestamp, renders
- **Seek coordination**: Main thread sets seek target atomically; audio/video threads detect and reset
- **Frame cache**: Seek-based cache stores frames around current playhead (±N frames) for smooth scrubbing

## Unsafe Code Isolation

### FFmpeg Wrapper (`decode/decoder.rs`)

All FFmpeg interactions isolated in `decode/decoder.rs`:

```rust
// Safe public API
pub struct Decoder {
    // Opaque pointer to FFmpeg context (unsafe internally)
    inner: *mut FFmpegContext,
}

impl Decoder {
    pub fn new(path: &Path) -> Result<Self, DecodeError> { /* unsafe inside */ }
    pub fn decode_frame(&mut self, timestamp: TimePoint) -> Result<Frame, DecodeError> { /* unsafe */ }
}

// Unsafe implementation details hidden
struct FFmpegContext {
    // FFmpeg structures accessed only via unsafe blocks
}
```

**Unsafe usage:**

- FFmpeg C API calls (all in `decode/decoder.rs`)
- Raw pointer dereferences for FFmpeg contexts
- Memory management (FFmpeg allocators)

**Safety guarantees:**

- Decoder struct ensures single-threaded access (Send + Sync only if properly synchronized)
- All FFmpeg errors converted to Rust Result types
- Resource cleanup in Drop impl

### wgpu Integration

wgpu is safe Rust, but GPU operations are inherently unsafe at hardware level. Isolate in `render/compositor.rs`:

- All wgpu device/queue access through safe wrapper
- Texture uploads validated before GPU operations
- Shader compilation errors handled gracefully

## Implementation Details

### Frame Cache (`decode/frame_cache.rs`)

Seek-based cache strategy:

- Cache window: ±30 frames around current playhead
- On seek, decode frames in new window
- LRU eviction when cache exceeds size limit
- Cache key: `(source_path, frame_index)`

### Audio Playback (`audio/player.rs`)

- cpal stream callback requests samples for current time
- Audio mixer reads from timeline audio track
- Resamples if needed (source sample rate ≠ output rate)
- Handles gaps between clips (silence)

### Playback Engine (`playback/engine.rs`)

State machine:

```rust
pub enum PlaybackState {
    Stopped,
    Playing { start_time: Instant, timeline_start: TimePoint },
    Paused { timeline_position: TimePoint },
    Seeking { target: TimePoint },
}
```

Commands from main thread:

- `Play`
- `Pause`
- `Seek(TimePoint)`
- `Stop`

## Dependencies (Cargo.toml)

```toml
[dependencies]
ffmpeg-next = "6.1"
wgpu = "0.20"
winit = "0.30"
egui = "0.27"
egui-wgpu = "0.27"
cpal = "0.15"
parking_lot = "0.12"  # For efficient mutexes
```

## Development Phases

1. **Phase 1: Core data structures** - Implement `core/` module with Timeline, Track, Clip, TimePoint
2. **Phase 2: Decoding** - FFmpeg decoder wrapper and frame cache
3. **Phase 3: Audio playback** - cpal integration and basic mixing
4. **Phase 4: Video rendering** - wgpu compositor for single video track
5. **Phase 5: Synchronization** - A/V sync and playback engine
6. **Phase 6: Timeline editing** - Cut, trim, move operations
7. **Phase 7: Export** - FFmpeg encoding pipeline

## Key Design Decisions

- **Rational timebase**: Matches FFmpeg, avoids floating-point precision issues
- **Audio-driven timing**: Standard practice, ensures smooth playback
- **Seek-based cache**: Balances memory usage with scrubbing performance
- **Unsafe isolation**: All FFmpeg unsafe code in single module with safe API
- **Thread-safe communication**: Atomic types for timing, channels for commands
- **Modular structure**: Each component (decode, render, audio, playback) is independent and testable