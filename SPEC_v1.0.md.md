# Rust Video Editor – Shared Technical Spec (MVP)

# 🔒 SPEC LOCK NOTICE

This document is AUTHORITATIVE.

Any change to this file requires:
1. A version bump
2. A written rationale
3. Explicit approval by the Integrator (human)

Agents are NOT allowed to:
- Change time units
- Add dependencies
- Modify thread model
- Expand scope

Violations invalidate the submission.

## Project Scope
- Minimal desktop video editor
- Personal use 
- MVP only (no plugins, no effects beyond transforms)

---

## Language & Platform
- Language: Rust (stable)
- Platforms: Windows, Linux

---

## Core Libraries (Locked)
- ffmpeg-next → decoding / encoding
- wgpu → GPU rendering
- winit → windowing/input
- egui + egui-wgpu → UI
- cpal → audio playback
- crossbeam → inter-thread channels
- tokio → background jobs (non-real-time)

---

## Time & Sync (CRITICAL)
- Time unit: **nanoseconds (`i64`)**
- Master clock: **audio playback**
- Video frames sync to audio clock
- No frame-based logic outside decoding

---

## Thread Model
- UI Thread: input & egui
- Decode Thread(s): FFmpeg audio/video
- Audio Thread: cpal callback (master clock)
- Render Thread: GPU submission

Threads communicate only via channels.

---

## Media Formats
- Video decode output: RGBA8
- Audio decode output: interleaved PCM f32
- Export: MP4 (H.264 + AAC)

---

## Timeline Model
- Timeline → Tracks → Clips
- Track types: Video, Audio
- Clips have in/out points (source time)
- Timeline time ≠ source time

---

## Safety Rules
- `unsafe` only allowed in:
  - ffmpeg bindings
  - GPU buffer mapping
- Unsafe code MUST be isolated in modules
- No unsafe in UI or timeline logic

---

## Performance Constraints
- Real-time preview @ 30fps minimum
- Frame dropping allowed during seek
- Correct sync > perfect smoothness

---

## Non-Goals (DO NOT IMPLEMENT)
- Effects beyond transforms
- Plugins
- Cloud features
- DRM / licensing
- Mobile support

---

## Deliverable Expectations
- Clean Rust modules
- Public APIs documented
- Unit tests where applicable
