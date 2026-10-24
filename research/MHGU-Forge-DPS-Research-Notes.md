# MHGU DPS Meter — Research Notes

Applies to **MHGU 1.4.0 + Forge 1.5.1**. All addresses are offsets relative to the `main` module. Re-check the instruction bytes before using these offsets with any other game version.

This document records only the reverse-engineering facts used by this project, the build process, failed experiments, and verification steps. It is not a complete map of MHGU memory. The Chinese version is [MHGU-Forge-DPS-研究笔记.md](MHGU-Forge-DPS-研究笔记.md).

---

## 1. Summary

- The plugin is a Forge NRO placed at `romfs/nativeNX/plugins/00_mhgu_dps_meter.nro`.
- Damage is collected with two game-function hooks: the collision function and the damage function.
- The plugin counts hits that the local game can process. Local-player damage is the most reliable; whether teammate damage appears depends on whether the local game processes that hit, and guest-side data depends more on synchronization.
- Player manager and player-object offsets are listed in section 3.
- `player + 0x1380` is **not** a Palico type, and it is not a stable type value either. It may be a per-Palico instance, appearance, voice, or other ID; the exact semantics are unconfirmed. Version 1.1.0 removed type detection and uses icon index 15 for every Palico.
- Most icon sources are 20×20 RGBA; the old Beast icon is 22×22. The current unified Palico icon is 20×20. Rendering merges each 2×2 source block into one logical cell: normally 2 px per cell at 720p and 3 px at 1080p.
- Palico icons appear about 2 px lower than weapon icons in practice. Version 1.1.0 compensates with `PALICO_ICON_LIFT = 2.0`. This is an empirical correction, not a confirmed engine-level slot rule.
- Every generated NRO must be checked so that no dynamic relocation writes into a read-only segment. This project's NRO has no `TEXTREL`.

## 2. Build and Verification

### 2.1 Toolchain

- Rust nightly with `rust-src` (see `source/rust-toolchain.toml`).
- `cargo-mhgu-forge`, pinned to this Forge revision:
  `f2af0622a2a0aa291ffbd3d2f7628c0e2d96d220`
- Custom target: `source/armv7a-none-eabi.json`
  - `linker = "rust-lld"`
  - `-Tlink/switch32.ld`
  - `-C link-arg=--build-id=sha1`
- devkitPro/switch-tools: `build.sh` sets a devkitARM linker path and invokes `cargo-forge` to package the NRO; the target spec also declares `rust-lld`. When using `build.sh`, make sure the devkitARM path exists. When running only `cargo forge build`, the linker follows the target spec (`rust-lld`).
- Official build command:

```bash
cd source
bash build.sh
```

`build.sh` installs `cargo-mhgu-forge` into the project's own `target/forge-tools`, so it does not modify the global toolchain.

### 2.2 Offline local build

The release source depends on Forge from GitHub. When the network is unavailable, temporarily point the dependency at the local Forge checkout in this workspace:

```toml
mhgu-forge = { path = "D:/zcode/MHGU/_build/forge-rs/forge", default-features = false, features = ["imgui", "mt"] }
```

Restore the git dependency after building. Do not commit the local path to the release repository.

### 2.3 NRO relocation check

`tools/verify_nro.py` emulates the loader: it maps text/rodata/data/bss, resolves the dynamic symbol table and relocations, and checks whether any relocation writes into text or rodata. A release build should report:

```text
DT_TEXTREL present: False
reloc writes by segment: {'data': ...}
OK: every relocation targets writable memory
```

A relocation into a read-only segment crashes the plugin after loading on real Switch hardware, so run this check before every release.

## 3. Runtime Memory

`base` is the runtime base of the MHGU main module. `GAME_BASE` stores it after `forge_onLoad`.

| Item | Address/offset | Notes |
| --- | --- | --- |
| Player manager | `main + 0x01885B58` | Produces the `manager` pointer |
| Local player | call `main + 0x00276278` with `manager` | Returns the local player object |
| Player slots | `manager + 0x1C + i*4`, `i=0..3` | Four owner pointers used for de-duplication and labels. Rows are sorted by damage; P1–P4 are not party order or host order |
| HUD color number | `player + 0x100` | Mask `0xF00`; maps `0x100→0, 0x200→1, 0x400→3, 0x800→2`. This is a color number, not player identity or a slot |
| Weapon-type pointer | `player + 0x13C4` | Read u8 at target `+0x14C`; `0..14` weapon, `15` Prowler |
| Room byte / session epoch | `player + 0x1054` | Combined with the player pointer to form `Epoch`, which identifies the current quest instance for floating damage numbers. Area changes clear old number events but **do not clear DPS totals** |
| Player/Palico name | `manager + 0x300 + i*0x560` | Owner at `+0x288`, UTF-8 name at `+0x290`, up to 32 bytes. Name-block semantics may differ between normal quests and Prowler mode |
| Palico ID | `player + 0x1380` | Not a type and not a stable type value; exact semantics unconfirmed. Experiment-only |

Before dereferencing a pointer, reject low addresses and the top of the 4 GB range. For the weapon pointer, also verify that `+0x14C` stays in range:

```rust
if pointer < 0x10000 || pointer > 0xFFFF_0000 - 0x14C {
    return 0xFF;
}
```

If a name block has no owner pointer, the local player can fall back to the `i=0` local name. The block layout is not identical between normal quests and Prowler mode, so the code keeps both exact-owner matching and the local fallback.

### 3.1 What `Epoch` actually means

```rust
pub struct Epoch {
    pub player: u32, // current player object pointer
    pub room: u8,    // room byte at player + 0x1054
}
```

- It means “current quest instance / session generation”, not a unit of time.
- A hit records the current `Epoch`; damage accounting and projection use it to keep floating numbers from the previous area out of the new one.
- When `forge_onUpdate` sees the player or room change, it calls `STATE.reset_epoch()`. That clears floating-number/projection state only.
- DPS totals are separate state. They persist across area changes and reset only after the result/save screen, or on the first hit of a new quest.

## 4. Collection Flow

### 4.1 Hooks

| Target | Offset | Purpose |
| --- | --- | --- |
| Collision check | `main + 0x00170968` | Bind attack/owner/part/world position before the original collision call |
| Damage resolution | `main + 0x0009D6F8` | Confirm and record damage after the original damage function returns |

The collision callback reads:

- whether `attack + 0x6D` is 1;
- whether `attack + 0x10` is forwarded;
- the owner from `attack + 0x70` when forwarded, otherwise `attack + 0x0C`;
- `part + 8` as the slot and `part + 6` as the part index;
- three f32 world coordinates from `point`.

The damage callback records a hit only when `raw > 0`, `result > 0`, `mode != 2`, and `slot < 8`. This filters abnormal and duplicate results.

### 4.2 Thread binding

The collision and damage callbacks do not share a call stack. The code identifies the calling thread with `thread_key()` (AArch32 TPIDRURO) and stores the active contact in a `BINDINGS` table. Nested hits restore the previous binding.

### 4.3 Ownership and display

- Only hits whose owner equals `local_player` are marked local.
- The DPS panel displays up to four owners. Slots come from the manager, but display rows are sorted by damage.
- Local damage drives the player's own floating numbers and yellow/white classification. The DPS chart accumulates every recorded owner; teammate curves still depend on synchronization.
- Room or player-object changes invalidate `Epoch` and clear old floating-number/projection state. DPS totals do not reset on an area change; they reset on the result/save screen or on the first hit of a new quest.

## 5. DPS State and Rendering

- `HISTORY_CAP = 600`: at most 600 chart points.
- `SAMPLE_INTERVAL = 1.0`: normally one sample per second.
- When the buffer is full, adjacent points are merged while keeping the first point, every second point, and the last point. Accumulated damage is never averaged, so the chart endpoint still represents the current real total.
- The default font size is `DPS_FONT_SIZE = 19.0` at the 720p reference, scaled by `display.height / 720`.
- Curve width/height/fill, names, hit count, inline stats, and team summary are configured in the INI.
- Config path: `savedata:/mhgu_dps_meter.ini`. The default template is `source/conf/mhgu_dps_meter.ini`, embedded at compile time.
- CJK font: `romfs/nativeNX/forge/fonts/NotoSansSC-Medium.otf`, OFL licensed; the license is in `OFL.txt`.

### 5.1 Icons

Most icon sources are 20×20 RGBA; the old Beast icon is 22×22. Version 1.1.0 no longer uses type-specific icons, and the current unified Palico icon is 20×20.

`draw_pixel_icon` works as follows:

1. `logical_w = icon.width / 2`, `logical_h = icon.height / 2`;
2. sample the top-left pixel of every 2×2 source block;
3. normally use a 2 px cell at 720p and a 3 px cell at 1080p; other resolutions use `round(target / logical_w)`;
4. multiply RGB by the HUD color and keep alpha;
5. merge consecutive equal colors in a row into one rectangle to reduce draw calls.

The unified Palico icon is `ICONS[15]`. In practice, Palico icons appear about 2 px lower than weapon icons. The renderer subtracts an offset when `icon_index >= PALICO_ICON`:

```rust
const PALICO_ICON: usize = 15;
const PALICO_ICON_LIFT: f32 = 2.0;
let icon_bottom = icon_bottom - PALICO_ICON_LIFT * scale;
```

This is an **empirical correction**; it does not prove that the engine has a fixed “Palico slot is 2 px lower” rule. Old Palico icons showed the same visual offset on hardware, and the new image's two transparent top rows made it more obvious. Re-check the offset whenever the icon is replaced.

## 6. Palico Type Investigation (Failed Experiment)

### 6.1 Initial assumption

Older versions treated `player + 0x1380` as the type and mapped eight values to icons:

```text
0xAB Fighting, 0x88 Assisting, 0x6C Healing, 0xBB Gathering,
0x80 Bombing, 0x68 Protection, 0xD4 Beast, 0x26 Charisma
```

### 6.2 Measurements

Each local Palico was tested and a `[TYPE]` log line was written:

| Tested type | `+0x1380` |
| --- | --- |
| Charisma/leader | `0x26, 0x1F, 0xBA, 0xCE` |
| Healing | `0x6C` |
| Bombing | `0x80` |
| Gathering | `0xBB` |
| Protection | `0x68` |
| Fighting | `0xAB` |
| Assisting | `0x88` |
| Beast | `0xD4` |

According to the test record, four leader Palicoes produced four different values. This proves only that `+0x1380` is not a stable type value. It may be an instance, appearance, voice, model-variant, or other ID; the exact semantics are unconfirmed. The old mapping had treated the ID of one tested cat as if it were the type, so it cannot be used for other cats.

### 6.3 Areas searched without a result

- The v8 dump of the player region `0x0000..0x4000`. This is a dumped range, not proof that the player object is only 0x4000 bytes.
- The v10 dump of the `manager` region `0x0000..0x1C00`. This is not the complete manager and does not cover all five player blocks completely.
- The five full manager blocks exported by v9 at `manager + 0x300 + i*0x560`, `0x560` bytes each. The raw v10 log only keeps up to `0x1C00`, truncating about `0x1E0` bytes from the end of block 5.
- Up to thirteen candidate player-object pointer targets; some snapshots skipped targets that failed pointer validation.
- Eleven candidate manager-block pointer targets.
- 32-bit fields, 1/2-byte fields, and 1–8-bit bitfields, using the test grouping “four leaders match, seven other types differ”.

Every search returned no candidate. As a result, memory dumps were abandoned as a way to find the type. Version 1.1.0 removed type detection and uses one Palico icon.

The raw v10 diagnostic log is `research/data/palico-v10.log`. It contains eleven snapshots and the v10 `0x1C00` manager region (block 5 truncated). It is a useful starting point for a future investigation, but it is not a complete manager snapshot.

## 7. Diagnostic Method

- Button chord: **L + R + Minus**.
- Log path: `savedata:/dps_debug.log`.
- Diagnostic builds do not log every `[HIT]`; doing so fills the buffer very quickly.
- v8 dumped the player `0x4000` region; v9 dumped manager blocks and pointer targets; v10 dumped the `0x1C00` manager region.
- The log accumulates in memory. Each button press overwrites the file with the complete accumulated text; eleven Palicoes produce roughly 370 KB.
- Validate pointer ranges and alignment before reading a target. A stale pointer can crash the game, so capture only inside a quest when the player object is valid.
- Prefer local wireless for testing. Do not take unverified builds into random online multiplayer.

## 8. Release Layout

A release contains only runtime files:

```text
MHGU-DPS-Meter-v1.1.0/
  README.md
  conf/mhgu_dps_meter.ini
  docs/demo.jpg
  romfs/nativeNX/forge/fonts/NotoSansSC-Medium.otf
  romfs/nativeNX/forge/fonts/OFL.txt
  romfs/nativeNX/plugins/00_mhgu_dps_meter.nro
```

Pre-release checklist:

1. `cargo +nightly forge build --release` succeeds;
2. `verify_nro.py` reports no relocation into read-only segments;
3. the version string inside the NRO matches `Cargo.toml`;
4. the INI header version matches;
5. the ZIP NRO SHA256 matches the `成品` directory.

## 9. Known Limitations

- MHGU 1.4.0 only. A game update may change these offsets.
- Whether teammate damage appears depends on whether the local game processes that hit. Guest-side data depends more on synchronization and may be delayed, incomplete, or different from the final result.
- Palico type is not detected automatically; every Palico uses the same icon.
- If the weapon type cannot be read (`0xFF`), no icon is shown, avoiding a false Prowler classification.
- Online random-match testing has not been performed. Use local wireless or a trusted party for verification.
