# AVES Integration Summary

**Quick Reference for Integration Work**

## Critical Issues Found

### ✅ 1. Timebase Alignment
- **Status:** ✅ ALIGNED
- All subsystems use nanoseconds (`i64`) consistently
- No timebase conversion issues

### ❌ 2. Duplicate Timeline Module (CRITICAL)
- **Status:** ❌ BLOCKING
- Two separate `Timeline`/`Clip`/`Track` implementations exist
- All modules currently use `crate::timeline::Timeline` (duplicate)
- **Action Required:** Remove `timeline/` module, update imports to `core::timeline::Timeline`

### ✅ 3. Master Clock Types
- **Status:** ✅ ALIGNED
- Both `audio::player` and `playback::sync` use `AtomicI64`
- No changes needed

### ⚠️ 4. FFmpeg Timebase Conversion
- **Status:** ⚠️ NOT IMPLEMENTED
- Placeholder code exists
- Will be needed when FFmpeg is integrated

---

## Required Actions

### Step 1: Remove Duplicate Module
```bash
# Delete these files:
src/timeline/clip.rs
src/timeline/track.rs
src/timeline/timeline.rs
src/timeline/mod.rs
```

### Step 2: Update lib.rs
```rust
// Remove this line:
pub mod timeline;
```

### Step 3: Update Imports (5 files)
Change `use crate::timeline::Timeline;` → `use crate::core::timeline::Timeline;`

**Files to update:**
- `src/playback/engine.rs`
- `src/audio/player.rs`
- `src/audio/mixer.rs`
- `src/export/exporter.rs`
- `src/export/pipeline.rs`

### Step 4: Verify Compilation
```bash
cargo check
cargo test
```

---

## Files Changed

| File | Change Type | Description |
|------|-------------|-------------|
| `src/lib.rs` | Delete | Remove `pub mod timeline;` |
| `src/timeline/*` | Delete | Remove entire `timeline/` module |
| `src/playback/engine.rs` | Update | Change import to `core::timeline` |
| `src/audio/player.rs` | Update | Change import to `core::timeline` |
| `src/audio/mixer.rs` | Update | Change import to `core::timeline` |
| `src/export/exporter.rs` | Update | Change import to `core::timeline` |
| `src/export/pipeline.rs` | Update | Change import to `core::timeline` |

---

## Testing Checklist

- [ ] All modules compile after changes
- [ ] No duplicate type definitions remain
- [ ] Timeline instances can be shared between subsystems
- [ ] All existing tests pass
- [ ] No breaking API changes

---

## Risk Level: MEDIUM

- **Breaking Changes:** Yes (import path changes)
- **Complexity:** Low (simple find/replace)
- **Testing Required:** Yes (compile + test suite)

---

## Estimated Time: 30 minutes

1. Delete files: 2 min
2. Update imports: 5 min
3. Update lib.rs: 1 min
4. Compile & test: 10 min
5. Verify: 12 min

---

**See `INTEGRATION_PLAN.md` for detailed analysis.**  
**See `RISK_LIST.md` for risk assessment.**

