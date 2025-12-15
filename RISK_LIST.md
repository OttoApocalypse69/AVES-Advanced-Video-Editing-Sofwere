# AVES Integration Risk List

**Role:** Integration Engineer  
**Date:** 2024  
**Status:** Pre-Implementation

## Risk Summary

| Risk ID | Severity | Likelihood | Impact | Mitigation |
|---------|-----------|------------|--------|------------|
| R1 | HIGH | HIGH | Type incompatibility blocks integration | Remove duplicate modules |
| R2 | HIGH | MEDIUM | Clock sync failures | Standardize on AtomicI64 |
| R3 | MEDIUM | LOW | FFmpeg timestamp errors | Implement timebase conversion |
| R4 | LOW | LOW | Import confusion | Clean up module exports |
| R5 | MEDIUM | MEDIUM | Breaking changes | Careful testing required |

---

## R1: Duplicate Timeline/Clip/Track Structures

**Severity:** HIGH  
**Likelihood:** HIGH (Already occurring)  
**Impact:** Type incompatibility prevents subsystem integration

### Description
Two separate implementations of `Timeline`, `Clip`, and `Track` exist:
- `core::timeline::Timeline` vs `timeline::timeline::Timeline`
- `core::clip::Clip` vs `timeline::clip::Clip`
- `core::track::Track` vs `timeline::track::Track`

These are **different types** in Rust, causing:
- Cannot pass `core::timeline::Timeline` to code expecting `timeline::timeline::Timeline`
- Cannot share timeline instances between subsystems
- Compilation errors if wrong type is used

### Current Impact
- All subsystems currently use `core::timeline::Timeline` (good)
- `timeline/` module is unused but exported (confusing)
- Risk: Future code might accidentally use `timeline::Timeline` instead of `core::Timeline`

### Mitigation
1. **Immediate:** Remove `src/timeline/` module entirely
2. **Verification:** Ensure all imports use `core::timeline`
3. **Testing:** Compile all modules to verify no broken imports

### Residual Risk
- **LOW:** If removal is done correctly, no residual risk
- **MEDIUM:** If some code uses `timeline::Timeline`, will cause compilation errors (easy to fix)

---

## R2: Master Clock Type Mismatch (✅ RESOLVED)

**Severity:** N/A (Already resolved)  
**Status:** ✅ VERIFIED - Both modules use `AtomicI64`

### Description
**UPDATE:** Upon verification, both modules already use `AtomicI64`:
- `audio::player::AudioPlayer` uses `AtomicI64` ✅
- `playback::sync::SyncController` uses `AtomicI64` ✅

**No action needed** - master clock types are already aligned.

### Current State
- Both use `Arc<AtomicI64>` for master clock
- Types are compatible for sharing
- No conversion needed

### Residual Risk
- **NONE:** Types are already correct

---

## R3: FFmpeg Timebase Conversion Missing

**Severity:** MEDIUM  
**Likelihood:** LOW (Not yet integrated)  
**Impact:** Incorrect timestamps when FFmpeg is integrated

### Description
FFmpeg uses rational timebases (AVRational: num/den), but conversion to nanoseconds is not implemented.

**Example:**
- FFmpeg timestamp: `1000` in timebase `1/1000` = 1 second
- Should convert to: `1_000_000_000` nanoseconds
- Current code: No conversion function exists

### Current Impact
- Decoder has placeholder TODOs (lines 64-70, 114-116 in `decoder.rs`)
- Cannot decode frames with correct timestamps
- Will cause sync issues when FFmpeg is integrated

### Mitigation
1. **Add timebase conversion functions** (see Integration Plan 3.3)
2. **Test with sample FFmpeg files** when integrated
3. **Handle edge cases:** Very large timestamps, negative timestamps

### Residual Risk
- **MEDIUM:** Conversion logic must be correct (precision, overflow)
- **LOW:** Can be tested independently before full integration

---

## R4: Import Inconsistencies

**Severity:** LOW  
**Likelihood:** LOW  
**Impact:** Developer confusion, potential wrong imports

### Description
Mixed import patterns and duplicate module exports:
- `lib.rs` exports both `core` and `timeline` modules
- Both export `Timeline`, `Clip`, `Track`
- Developers might accidentally import from wrong module

### Current Impact
- All current code uses `core::timeline` (correct)
- Risk: Future code might use `timeline::Timeline` by mistake
- Confusion during code review

### Mitigation
1. **Remove `timeline/` module** (eliminates confusion)
2. **Update `lib.rs`** to only export `core::timeline`
3. **Document** that `core::timeline` is the canonical location

### Residual Risk
- **LOW:** After removal, no confusion possible

---

## R5: Breaking Changes During Integration

**Severity:** MEDIUM  
**Likelihood:** MEDIUM  
**Impact:** Existing code might break, requires fixes

### Description
Integration changes might break existing code:
- Removing `timeline/` module might break code that imports it
- Changing `AtomicU64` → `AtomicI64` might break code that assumes unsigned
- Type changes might require updates in multiple places

### Current Impact
- Unknown: Need to verify all code uses correct types
- Risk: Some code might be using wrong types

### Mitigation
1. **Compile all modules** after each change
2. **Run existing tests** to catch regressions
3. **Incremental changes:** Fix one issue at a time
4. **Rollback plan:** Keep git history, can revert if needed

### Residual Risk
- **LOW:** If changes are incremental and tested
- **MEDIUM:** If large changes are made at once

---

## R6: Timebase Precision Loss

**Severity:** LOW  
**Likelihood:** LOW  
**Impact:** Minor timestamp inaccuracies

### Description
Converting between FFmpeg timebases and nanoseconds might lose precision:
- Floating-point conversions
- Integer division rounding
- Very large timestamp values

### Current Impact
- Not yet relevant (FFmpeg not integrated)
- Will be relevant when decoding frames

### Mitigation
1. **Use i128 for intermediate calculations** (avoid overflow)
2. **Test with edge cases:** Very large timestamps, high frame rates
3. **Document precision limits**

### Residual Risk
- **LOW:** Precision loss should be minimal (< 1 nanosecond)

---

## R7: Thread Safety Issues

**Severity:** MEDIUM  
**Likelihood:** LOW  
**Impact:** Race conditions, data corruption

### Description
Master clock is shared between threads:
- Audio thread writes to `AtomicI64`
- Video thread reads from `AtomicI64`
- Need to ensure proper memory ordering

### Current Impact
- Code uses `Ordering::Relaxed` (may not be sufficient)
- Need to verify thread safety

### Mitigation
1. **Use `Ordering::Acquire` for reads**
2. **Use `Ordering::Release` for writes**
3. **Document thread safety guarantees**

### Residual Risk
- **LOW:** Atomic operations are inherently thread-safe
- **MEDIUM:** Memory ordering might need adjustment for strict sync

---

## Risk Matrix

```
        │ LOW  │ MEDIUM │ HIGH │
────────┼──────┼────────┼──────┤
LOW     │ R4   │ R3, R6 │      │
MEDIUM  │      │ R5, R7 │      │
HIGH    │      │        │ R1, R2│
```

**Priority Order:**
1. **R1** - Duplicate structures (blocks integration) - **CRITICAL**
2. **R2** - Master clock mismatch - ✅ **RESOLVED** (no action needed)
3. **R5** - Breaking changes (requires testing)
4. **R3** - FFmpeg conversion (future issue)
5. **R7** - Thread safety (low likelihood)
6. **R4** - Import confusion (low impact)
7. **R6** - Precision loss (low impact)

---

## Risk Mitigation Summary

### Immediate Actions (Before Integration)
1. ✅ Remove duplicate `timeline/` module
2. ✅ Update all imports to use `core::timeline::Timeline`
3. ✅ Verify all modules compile
4. ✅ Master clock types already aligned (no changes needed)

### Short-term Actions (During Integration)
1. ⚠️ Add FFmpeg timebase conversion functions
2. ⚠️ Test timeline sharing between subsystems
3. ⚠️ Verify thread safety of master clock

### Long-term Actions (After Integration)
1. 📋 Test with real FFmpeg files
2. 📋 Performance testing for thread safety
3. 📋 Documentation updates

---

## Acceptance Criteria

Integration is considered successful when:
- [ ] All subsystems can share the same `Timeline` instance
- [ ] Master clock is shared between audio and sync
- [ ] All modules compile without errors
- [ ] No duplicate type definitions exist
- [ ] Timebase conversion functions are ready for FFmpeg
- [ ] All existing tests pass
- [ ] No breaking API changes

---

**Document Status:** Pre-Implementation Review  
**Next Review:** After integration changes are implemented

