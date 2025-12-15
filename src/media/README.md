# Media Decoding Subsystem

## Overview

The `media/decoder.rs` module provides a safe Rust wrapper around FFmpeg for decoding video and audio files. All unsafe FFmpeg operations are isolated within this module.

## Public API

### `MediaDecoder`

Main decoder struct that wraps FFmpeg functionality.

**Key Methods:**
- `new(path)` - Create decoder for a media file
- `get_video_stream_info()` - Get video stream metadata
- `get_audio_stream_info()` - Get audio stream metadata
- `find_video_stream()` - Get video stream index
- `find_audio_stream()` - Get audio stream index
- `seek(timestamp, stream_index)` - Seek to nearest keyframe
- `start_video_decoding(stream_index)` - Start video decoding thread, returns channel receiver
- `start_audio_decoding(stream_index)` - Start audio decoding thread, returns channel receiver

### Output Formats (per SPEC.md)

- **Video**: RGBA8 frames with nanosecond timestamps
- **Audio**: Interleaved PCM f32 samples with nanosecond timestamps

### Threading Model

- Decoding runs in separate threads
- Frames are sent via `crossbeam::channel::unbounded()` channels
- Threads automatically clean up when receiver is dropped

## FFmpeg Timebase Conversion

### Overview

FFmpeg uses rational timebases: `timestamp * (num/den) = seconds`

### Conversion to Nanoseconds

```rust
nanos = (timestamp * num * 1_000_000_000) / den
```

**Example:**
- Timebase: (1, 1000) = milliseconds
- Timestamp: 5000
- Nanoseconds: (5000 * 1 * 1_000_000_000) / 1000 = 5_000_000_000 ns = 5 seconds

### Conversion from Nanoseconds

```rust
timestamp = (nanos * den) / (num * 1_000_000_000)
```

**Example:**
- Nanoseconds: 5_000_000_000
- Timebase: (1, 1000)
- Timestamp: (5_000_000_000 * 1000) / (1 * 1_000_000_000) = 5000

### Important Notes

1. **Stream-specific timebases**: Each stream has its own timebase. Always use `stream->time_base`, not `format_ctx->time_base`.

2. **Common timebases:**
   - `(1, 1000)` - milliseconds
   - `(1, 1_000_000)` - microseconds
   - `(1, 90_000)` - MPEG-TS (common for H.264)
   - Variable per stream

3. **Overflow prevention**: Use `i128` for intermediate calculations to avoid overflow when multiplying large timestamps.

4. **Seeking**: When seeking, FFmpeg seeks to the nearest keyframe (I-frame) before the target timestamp. This is why we use `AVSEEK_FLAG_BACKWARD`.

5. **Timestamp accuracy**: After seeking, you may need to decode forward to reach the exact timestamp, as seeking goes to the nearest keyframe.

## Unsafe Code Isolation

All unsafe FFmpeg operations are contained within:
- `open_ffmpeg_context()` - FFmpeg initialization
- `decode_video_loop()` - Video decoding loop
- `decode_audio_loop()` - Audio decoding loop
- `FFmpegContext::drop()` - Resource cleanup

The public API (`MediaDecoder`) is completely safe.

## Error Handling

All FFmpeg errors are converted to `DecodeError` enum:
- `FFmpeg(String)` - Generic FFmpeg error with message
- `FileNotFound` - Input file doesn't exist
- `NoVideoStream` / `NoAudioStream` - Stream not found
- `SeekFailed` - Seek operation failed
- Others as needed

## Resource Management

- FFmpeg contexts are wrapped in `Arc` for thread safety
- All resources are automatically freed in `Drop` implementation
- Channels automatically clean up when receiver is dropped

## Usage Example

```rust
use aves::media::MediaDecoder;

// Create decoder
let decoder = MediaDecoder::new("video.mp4")?;

// Get stream info
let video_info = decoder.get_video_stream_info()?;
let video_stream_index = decoder.find_video_stream()?;

// Start decoding
let video_rx = decoder.start_video_decoding(video_stream_index)?;

// Receive frames
while let Ok(frame) = video_rx.recv() {
    // Process RGBA8 frame
    println!("Frame: {}x{} at {}ns", frame.width, frame.height, frame.timestamp);
}
```

