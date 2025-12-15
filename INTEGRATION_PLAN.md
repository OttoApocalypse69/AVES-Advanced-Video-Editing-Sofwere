# AVES Integration Plan

**Role:** Integration Engineer  
**Date:** 2024  
**Status:** Analysis Complete

## Executive Summary

This document identifies API mismatches between subsystems, verifies timebase alignment, and proposes minimal glue code to integrate the AVES video editing software subsystems.

**Key Findings:**
- ✅ Timebase alignment: All subsystems use nanoseconds (`i64`) consistently
- ❌ **CRITICAL:** Duplicate Timeline/Clip/Track structures in `core/` and `timeline/` modules
- ❌ **CRITICAL:** Master clock type mismatch: `AtomicI64` vs `AtomicU64`
- ⚠️ Import inconsistencies across modules
- ⚠️ FFmpeg timebase conversion not implemented (placeholder code)

---

## 1. API Mismatches Identified

### 1.1 Duplicate Data Structures (CRITICAL)

**Problem:** Two separate implementations of core timeline structures exist:

| Structure | Location 1 | Location 2 | Status |
|-----------|-----------|------------|--------|
| `Timeline` | `core::timeline::Timeline` | `timeline::timeline::Timeline` | ❌ Duplicate |
| `Clip` | `core::clip::Clip` | `timeline::clip::Clip` | ❌ Duplicate |
| `Track` | `core::track::Track` | `timeline::track::Track` | ❌ Duplicate |

**Impact:**
- Type incompatibility: `core::timeline::Timeline` ≠ `timeline::timeline::Timeline`
- Modules cannot share timeline instances
- Code duplication and maintenance burden

**Current Usage:**
- `playback::engine` → uses `crate::timeline::Timeline` (from `timeline/` module)
- `audio::player` → uses `crate::timeline::Timeline` (from `timeline/` module)
- `audio::mixer` → uses `crate::timeline::Timeline` (from `timeline/` module)
- `export::exporter` → uses `crate::timeline::Timeline` (from `timeline/` module)
- `export::pipeline` → uses `crate::timeline::Timeline` (from `timeline/` module)

**Note:** All modules currently import from `crate::timeline::Timeline`, which resolves to `timeline/timeline.rs`, NOT `core/timeline.rs`. This means they're all using the duplicate `timeline/` module version.

**Resolution Strategy:**
- **Option A (Recommended):** Remove `timeline/` module, use `core/` only
- **Option B:** Remove `core/timeline.rs`, `core/clip.rs`, `core/track.rs`, use `timeline/` only
- **Option C:** Create type aliases/adapters (NOT recommended - adds complexity)

**Recommendation:** Option A - `core/` module is more fundamental and already used by all subsystems.

---

### 1.2 Master Clock Type Alignment (✅ VERIFIED)

**Status:** ✅ Both modules use `AtomicI64` consistently.

| Module | Type | Location | Status |
|--------|------|----------|--------|
| `audio::player` | `AtomicI64` | Line 36 | ✅ Correct |
| `playback::sync` | `AtomicI64` | Line 13 | ✅ Correct |

**Verification:**
- Both use `Arc<AtomicI64>` for master clock
- Types are compatible for sharing
- No conversion needed

**Note:** Master clock types are already aligned. No changes needed.

---

### 1.3 Import Inconsistencies (MEDIUM)

**Problem:** Mixed import patterns across modules:

**Current Imports:**
- `playback::engine` → `use crate::core::timeline::Timeline;`
- `audio::player` → `use crate::core::timeline::Timeline;`
- `audio::mixer` → `use crate::core::timeline::Timeline;`
- `export::exporter` → `use crate::core::timeline::Timeline;`

**Issue:** All modules correctly use `core::timeline`, but `timeline/` module also exports `Timeline`, creating confusion.

**Resolution:** After removing duplicate `timeline/` module, update `lib.rs` to only export `core::timeline`.

---

## 2. Timebase Alignment Verification

### 2.1 Time Representation (✅ ALIGNED)

**Status:** ✅ All subsystems use nanoseconds (`i64`) consistently.

| Subsystem | Time Type | Unit | Status |
|-----------|-----------|------|--------|
| Core | `Time = i64` | nanoseconds | ✅ |
| Decode | `Time` (from core) | nanoseconds | ✅ |
| Audio | `Time` (from core) | nanoseconds | ✅ |
| Playback | `Time` (from core) | nanoseconds | ✅ |
| Export | `Time` (from core) | nanoseconds | ✅ |
| Render | Uses `VideoFrame.timestamp: Time` | nanoseconds | ✅ |

**Verification:**
- `core::time::Time = i64` (nanoseconds)
- All modules import `crate::core::time::Time`
- No timebase conversions in current code

---

### 2.2 FFmpeg Timebase Conversion (⚠️ NOT IMPLEMENTED)

**Problem:** FFmpeg uses rational timebases (AVRational), but conversion to nanoseconds is not implemented.

**Current State:**
- `decode::decoder::Decoder` has placeholder TODOs for FFmpeg integration
- No timebase conversion functions exist
- Timestamp conversion from FFmpeg to nanoseconds is missing

**Required Conversions:**
```rust
// FFmpeg timestamp → nanoseconds
fn ffmpeg_ts_to_nanos(ffmpeg_ts: i64, timebase: AVRational) -> Time {
    // Convert: ffmpeg_ts * (1/timebase) * 1e9
}

// Nanoseconds → FFmpeg timestamp
fn nanos_to_ffmpeg_ts(nanos: Time, timebase: AVRational) -> i64 {
    // Convert: nanos * timebase / 1e9
}
```

**Impact:** Decoder cannot convert FFmpeg timestamps to timeline nanoseconds.

**Resolution:** Implement timebase conversion in `decode::decoder` when FFmpeg integration is added.

---

### 2.3 Master Clock Synchronization (✅ ALIGNED)

**Status:** ✅ Audio-driven timing is correctly implemented.

- `audio::player` maintains master clock (`AtomicI64`)
- `playback::sync::SyncController` reads master clock
- Video syncs to audio clock (per SPEC.md)

**Issue:** Type mismatch prevents direct sharing (see 1.2).

---

## 3. Minimal Glue Code Proposals

### 3.1 Remove Duplicate Timeline Module

**Action:** Delete `src/timeline/` module entirely.

**Files to Delete:**
- `src/timeline/clip.rs`
- `src/timeline/track.rs`
- `src/timeline/timeline.rs`
- `src/timeline/mod.rs`

**Files to Update:**
- `src/lib.rs`: Remove `pub mod timeline;`
- **CRITICAL:** Update all modules to use `core::timeline::Timeline` instead of `crate::timeline::Timeline`

**Files Requiring Import Updates:**
- `src/playback/engine.rs`: Change `use crate::timeline::Timeline;` → `use crate::core::timeline::Timeline;`
- `src/audio/player.rs`: Change `use crate::timeline::Timeline;` → `use crate::core::timeline::Timeline;`
- `src/audio/mixer.rs`: Change `use crate::timeline::Timeline;` → `use crate::core::timeline::Timeline;`
- `src/export/exporter.rs`: Change `use crate::timeline::Timeline;` → `use crate::core::timeline::Timeline;`
- `src/export/pipeline.rs`: Change `use crate::timeline::Timeline;` → `use crate::core::timeline::Timeline;`

---

### 3.2 Master Clock Type (✅ NO CHANGES NEEDED)

**Status:** Master clock types are already aligned.

- `audio::player` uses `AtomicI64` ✅
- `playback::sync` uses `AtomicI64` ✅
- Both can share the same `Arc<AtomicI64>` instance

**No changes required.**

---

### 3.3 Add FFmpeg Timebase Conversion (Placeholder)

**File:** `src/decode/decoder.rs`

**Add Helper Functions:**
```rust
/// Convert FFmpeg timestamp to nanoseconds
/// 
/// # Arguments
/// - `ffmpeg_ts`: FFmpeg timestamp in timebase units
/// - `num`: Numerator of timebase (e.g., 1 for 1/1000)
/// - `den`: Denominator of timebase (e.g., 1000 for 1/1000)
fn ffmpeg_ts_to_nanos(ffmpeg_ts: i64, num: i32, den: i32) -> Time {
    // Convert: (ffmpeg_ts * num / den) * 1e9
    // Use i128 to avoid overflow
    let seconds = (ffmpeg_ts as i128 * num as i128) / (den as i128);
    (seconds * 1_000_000_000) as i64
}

/// Convert nanoseconds to FFmpeg timestamp
fn nanos_to_ffmpeg_ts(nanos: Time, num: i32, den: i32) -> i64 {
    // Convert: (nanos / 1e9) * den / num
    let seconds = nanos as i128 / 1_000_000_000;
    ((seconds * den as i128) / num as i128) as i64
}
```

**Note:** These are placeholder implementations. Actual FFmpeg integration will use `AVRational` struct.

---

### 3.4 Update Module Exports

**File:** `src/lib.rs`

**Current:**
```rust
pub mod core;
pub mod timeline;  // ← Remove this
```

**After:**
```rust
pub mod core;
// timeline module removed - use core::timeline instead
```

---

## 4. Integration Test Points

### 4.1 Timeline Sharing Test

**Test:** Verify all subsystems can share the same `Timeline` instance.

```rust
#[test]
fn test_timeline_sharing() {
    let timeline = crate::core::Timeline::new();
    
    // All subsystems should accept the same type
    let _player = AudioPlayer::new(timeline.clone());
    let _mixer = AudioMixer::new(timeline.clone());
    let _engine = PlaybackEngine::new(timeline.clone());
}
```

---

### 4.2 Master Clock Synchronization Test

**Test:** Verify master clock is shared between audio and sync.

```rust
#[test]
fn test_master_clock_sync() {
    let sync = SyncController::new();
    let clock = sync.master_clock();
    
    // Update clock
    clock.store(1_000_000_000, Ordering::Relaxed);  // 1 second
    
    // Read back
    assert_eq!(sync.current_timeline_position(), 1_000_000_000);
}
```

---

## 5. Implementation Order

1. **Step 1:** Fix master clock type mismatch (3.2)
   - Low risk, isolated change
   - Enables clock sharing

2. **Step 2:** Remove duplicate timeline module (3.1)
   - Medium risk, requires verification
   - Simplifies codebase

3. **Step 3:** Add FFmpeg timebase conversion (3.3)
   - Low risk, placeholder code
   - Prepares for FFmpeg integration

4. **Step 4:** Update module exports (3.4)
   - Low risk, cleanup

---

## 6. Verification Checklist

- [ ] All modules compile after changes
- [ ] No duplicate type definitions remain
- [ ] Master clock type is consistent (`AtomicI64`)
- [ ] Timeline instances can be shared between subsystems
- [ ] Timebase conversions are ready for FFmpeg integration
- [ ] All tests pass
- [ ] No breaking API changes (internal only)

---

## 7. Risk Assessment

See `RISK_LIST.md` for detailed risk analysis.

---

## 8. Notes

- **No feature additions:** All changes are integration fixes only
- **No refactors:** Minimal code changes to fix mismatches
- **Backward compatibility:** Internal changes only, no public API changes
- **FFmpeg integration:** Timebase conversion is placeholder until FFmpeg is integrated

---

**Document Status:** Ready for Implementation  
**Next Steps:** Review with team, implement changes in order specified

