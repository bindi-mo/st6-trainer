# SF6 Replay Frame Data Extraction Specification

This document outlines the workflow and specifications for extracting game telemetry and input data from Street Fighter 6 replay videos using automated image processing (OpenCV / Rust / Tauri).

---

## 1. Overview & Strategy

The system operates on frame-by-frame replay video ($60\text{ fps}$, $1000/60 = 16.667\text{ ms}$
per frame), targeting **Regions of Interest (ROIs)** with deterministic OpenCV (HSV colour
masking and grayscale template matching). It avoids 3D pose estimation and text popup tracking
by reading the replay's explicit on-screen displays.

**Scope:** input data and raw telemetry only — direction glyphs, button glyphs, hold-frame
counts, gauge values, character names, control type, Frame Meter values, and colour-bar cell
states. Move names, move identification, command-attempt analysis and inference are **not**
produced here.

**Design invariants**

- All processing happens in one canonical raster (§6.1). Resolution independence comes from
  that normalisation, not from ratio-based coordinates.
- Every uncertainty that could corrupt output becomes a **loud failure or an explicit flag**.
  Silent degradation is prohibited.
- HUD geometry is calibrated once per replay and never re-derived.
- Displays are read **over time**. A single snapshot cannot place a cell or a row in time.
- **Colour never determines an action boundary; the input command does** (§3.3.6).

---

## 2. Calibration

### 2.1 Scope

Calibration determines **geometry only**: the vertical displacement of the **upper HUD**
(HP / Drive / character names) and the **lower HUD** (SA gauges / Frame Meter / colour bars),
in canonical pixels.

- **Upper and lower groups calibrate independently** (`top_y_offset_px`,
  `bottom_y_offset_px`).
- **Input history ROIs are fixed and never offset** (§4.3); they belong to neither group.
- `top_y_offset_px >= 0`, `bottom_y_offset_px <= 0`. A sign reversal means the wrong anchor.
- Calibration runs **once per replay**. HUD Y cannot change mid-capture, so re-derivation only
  lets a wrong match poison the remainder.

### 2.2 Offset granularity

**Vertical position is continuous** — arbitrary integers at 1 px. No preset, no snapping, no
quantisation.

**HUD scale is not configurable.** Only position moves. Single-scale matching is correct; no
scale search, no template pyramid.

### 2.3 Anchor templates

**An anchor must contain no pixel that changes during a match.** Dynamic regions (HP fill,
gauge segments, timer digits, SA pips, portraits, the colour bar, the yellow cannot-act fill)
must never be anchors.

Purity is verified offline per pack: $\ge 200$ frames across $\ge 3$ matches, temporal
per-pixel $\sigma_t(x,y)$, require $\max_{x,y}\sigma_t(x,y)\le 6.0$ on 8-bit grayscale.

At least two anchors per group, so consensus is possible:

| Group | Anchor |
|---|---|
| upper | timer frame top edge; P1 / P2 HP bar outer frame corner |
| lower | P1 / P2 SA gauge static bracket; Frame Meter frame top edge |

Anchor dimensions are **measured from a reference screenshot**, never guessed.

### 2.4 Search

**Search Y only**; X is restricted to $\pm32\text{ px}$ of the baseline (no horizontal HUD
displacement exists). The whole band is covered by **one** `cv2.matchTemplate(TM_CCOEFF_NORMED)`
call over the padded band — no per-offset loop, no pyramid.

Bands (config data): upper $Y\in[0.00,0.25]$, lower $Y\in[0.70,1.00]$ of canonical height.

### 2.5 Acceptance and consensus

Accept an anchor only if **all** hold:

| Gate | Threshold |
|---|---|
| absolute confidence $v_{\max}$ | $\ge 0.90$ |
| peak prominence $P=v_{\max}-\max_{\lvert i-i_{\max}\rvert\ge3}R_i$ | $\ge 0.10$ |
| horizontal displacement $\lvert u\rvert$ | $\le 2\text{ px}$ |

Prominence is mandatory: a low peak on a flat surface produces a confident wrong answer.

Consensus: median $\tilde{\Delta y}$ over all accepted votes; drop votes $>2\text{ px}$ from the
median; recompute; require $\ge 0.80$ surviving. Below that, or two disjoint near-equal
clusters, emit `E_CALIB_AMBIGUOUS`. **Never pick the larger cluster** — a 4 px error is fatal
for a $10\times6$ px glyph.

### 2.6 Failure handling

- **Fail closed.** One group failing still allows the other to calibrate; only the failed
  group's fields are `null` + `LOW_CONF`. A zero offset is never a fallback.
- **`E_HUD_ABSENT` requires both:** no clean frame for either anchor group across the window,
  **and** no readable input glyph in the fixed input ROIs. If either exists, the HUD is present
  and the outcome is `E_CALIB_*`. The diagnosis must name the distinguishing evidence.
- Geometry gate after fusing: every ROI in-bounds and upper/lower groups non-overlapping, else
  `E_ROI_OUT_OF_BOUNDS` / `E_ROI_GROUP_OVERLAP`.

### 2.7 Manual calibration fallback

- **Offered only for geometry failures** (`E_CALIB_AMBIGUOUS`, `E_CALIB_LOW_CONF`,
  `E_ROI_OUT_OF_BOUNDS`, `E_ROI_GROUP_OVERLAP`). Malformed input (`E_ASPECT_UNSUPPORTED`,
  `E_ANISOTROPIC_SCALE`, `E_VFR_UNSUPPORTED`, decode errors) is never prompted for.
- **Two vertical offsets only**: upper $[0,270]$, lower $[-324,0]$, canonical px, independent.
  X, ROI shapes, anchors and the baseline raster are not editable.
- **Confirm, do not type.** Up to 3 candidate offsets plus any stored override, each shown as a
  still with the group's ROI rectangles overlaid; adjust with a ±1 px stepper and live
  confidence. A bare pixel number is unverifiable by eye at this glyph size.
- **On accept:** run the §2.6 geometry gate (blocking), re-match anchors. Mean confidence
  $<0.90$ requires explicit second confirmation and sets `manual_verified=false`; unverified
  groups carry `LOW_CONF`.
- **Non-interactive never blocks.** `--on-calib-failure` is `ask` for GUI, **`fail` by default**.
- **Manual offsets are part of `match_id`**, so a corrected re-run differs from the wrong one.

---

## 3. Targeted Data Elements

### 3.1 Character names (P1 / P2)

- Grayscale template matching, top-left / top-right, adjusted by `top_y_offset_px`.
- Extracted **once** during initialisation.
- **Names are plain alphabet in every locale, including Japanese UI.** No enum, no locale packs.
  P1 left-aligned, P2 right-aligned.
- No match at $\ge 0.90$, or HUD hidden $\Rightarrow$ `null`.

### 3.2 Gauges (HP / Drive / SA)

- HSV colour masking with horizontal pixel counting in offset-adjusted ROIs.
- HP ratio $0.0$–$1.0$; Drive $0.0$–$6.0$ + Burnout; SA $0$–$3$, per player.
- Failure handling: §5.

### 3.3 Frame Meter, damage and attack property

- **Arabic numerals in every locale.** Startup, Total, Advantage, Damage, Combo Damage use the
  shared digit set (§8.2). No OCR.
- **Two separate ROI groups — never one block.**
  - **Frame Meter group:** Startup, Total, Advantage, **per player**.
  - **Centre group:** below the round timer — Damage, Combo Damage, Attack Property, **per
    player**.
- **Units are frames (`F`);** Startup / Total / Advantage are integers.
- **Latched, never per-frame.** A reading is accepted after `FM_LATCH_FRAMES = 3` identical
  consecutive frames and emitted as one `move_events` row with inclusive `frame_start` /
  `frame_end`. `frame_telemetry` holds only HP / Drive / SA.
- A field below $0.90$ is `null` + `LOW_CONF`; the block is still emitted.

#### 3.3.1 Attack property

- Values: **`High` / `Mid` / `Low` / `Throw`** (JP: 上段 / 中段 / 下段 / 投げ).
- A **localized word with no colour cue**. Word templates only.
- **Every locale needs its own pack, English included.** `High` is text, not a shared glyph.
- Packs `attack_property_pack_<locale>`, exactly four crops. Word widths differ, so match against
  all four with $\pm6\text{ px}$ tolerance; argmax, require $\ge 0.92$.
- `attack_property_locale` lists loaded locales (default `["en"]`). Word from an unloaded locale
  $\Rightarrow$ `null` + `ATTACK_PROPERTY_UNSUPPORTED`. Never substitute a neighbour's value.
  Additional locale packs are added as they become available; `en` is the baseline.

#### 3.3.2 Advantage sign

- Positive renders `+` in blue, negative `-` in red. **Zero has no sign**: it renders `0F`.
  Parsing must accept an absent sign, and `fm_advantage = 0` must be reachable with no sign
  match.
- **A positive advantage may render with no sign at all** (observed: `17F`, not `+17F`). The
  parser must therefore accept three forms: `+NN`, `-NN`, and bare `NN`. A bare number is a
  positive value; it is not an error and must not set any quality flag.
- Digits and sign glyphs are authoritative. Colour is a cross-check: disagreement sets
  `fm_advantage_sign_conflict` and keeps the glyph value.

#### 3.3.3 Colour meter and legend

- The meter is present in replays and is a **ring buffer of 60 cells, one cell written per
  logical frame** (§3.3.4 governs state extraction).
- **Frame counts are never derived from the meter by reading a cell position**: it wraps, so a
  60-frame and a 120-frame move can render identically. **Total is never taken from the meter**
  (recovery has no distinct terminal colour and the ring wraps). Startup may be cross-checked by
  strict temporal counting (§3.3.5); the digits remain authoritative for all values.
- Cell colours follow a **locale-independent** 11-state legend; the legend *text* is
  locale-dependent and is **never parsed**.
- The legend panel is display-only and excluded from all ROIs. If it overlaps a ROI, the §2.6
  geometry gate rejects the run rather than reading a shifted ROI.

#### 3.3.4 Cell state model and extraction

- **Two meters, one per player** (`bar_roi_p1`, `bar_roi_p2`), 60 cells, dark boundaries. Cell
  count validated as exactly 60, else `E_BAR_GEOMETRY`.
- **Ring buffer semantics.** One cell is written per logical frame; the state written at frame
  $N$ is the character's state at $N$. The head advances one cell per frame and wraps at 60.
  The timeline is continuous and never rewinds mid-round.
- **Read only the cell that changed** between frames $N-1$ and $N$; that is the state written at
  $N$. O(1) per frame and correct across the wrap. If more than one cell changes unexpectedly
  (a dropped capture frame), mark the span `LOW_CONF`, set `resync_used`, re-read the full meter.
- **Black means no state** — neutral, able to act, nothing happening. It is **not** the state of
  a character being attacked: being hit or blocking writes the corresponding defender state.
- **Green (`counter_state`) means "counter-able", not "startup".** It appears during a move's
  pre-hitbox period and again in the gaps between hitboxes of a multi-hit move.
- **Every green span is a real counter-able window, but only the first is startup.** Within a
  single-button multi-hit move the system returns to a startup-equivalent state between hitboxes:
  the gap has no hitbox and can be hit exactly like a move's opening. It is therefore a genuine
  vulnerability window, not a visual artefact.
- **Yellow (`cannot_act`) means action-impossible**: hitstun, being hit, being thrown. Being hit
  during green is a **counter hit** and moves the character green → yellow.
- **Reset rules, evaluated per meter, never globally:**
  - The winner's meter resets to black at victory; the loser's shows `cannot_act`.
  - The whole Frame Meter clears at `round_start_frame`. A per-cell bulk change there is the
    round-start clear, not many state transitions.
- **States classified against the 11-state palette in Lab or HSV**, nearest entry with a minimum
  distance margin. No clear winner $\Rightarrow$ `"?"` + `unknown_cell_state`. Legend text is
  never parsed.
- Emit one `move_state_events` record per run of identical `state`. **Spans do not cross a round
  boundary**, because the meter clears at round start. Join to `move_events` by frame overlap.
- **Record every green and yellow span, regardless of action boundaries.** Only their
  interpretation differs; nothing is discarded. `cannot_act` outside the preround window is a
  defect: emit it and set `unexpected_cannot_act`.

State values: `counter_state`, `punish_counter_state`, `non_counter_action_recovery`,
`hitbox_appearance`, `projectile_active`, `parry_counter_active`, `post_damage_block_recovery`,
`invincibility`, `strike_throw_invincibility`, `projectile_invincibility`,
`strike_projectile_invincibility`, `cannot_act`.

> Measurement note: `cannot_act` (yellow) must be checked for colour separation from the legend's
> `post_damage_block_recovery` (also yellowish). If their palette distance is below the margin,
> they must be separated by context (preround window) or the palette entries must be refined.

#### 3.3.5 Phase composition and Startup cross-check

- Counting is **1-based**: the cell written on the button-press frame is cell 1. The meter
  reflects each frame as it happens — no display lag exists.
- Phase composition is `startup + active + recovery = total`. Active is the
  `hitbox_appearance` run; **the first red cell belongs to the end of startup, not to active**.
- **Startup = number of logical frames from the move-start frame up to but excluding the first
  `hitbox_appearance` cell.** For Startup 6F, cell 6 is the first red cell and cells 1–5 are
  green.
- Startup is cross-checked against the meter by **strict temporal counting**, not by reading a
  position, so the 60-cell wrap does not affect it. There is no lag and no rounding to absorb, so
  the cross-check is **strict**: a one-cell mismatch is a defect. A mismatch sets
  `fm_startup_bar_conflict` and keeps the digit value.
- **Total is never derived from the meter** (recovery has no terminal colour and the ring wraps).
  `fm_total` is digit-authoritative.
- One cell carries exactly one colour, so a frame's state is a single value; overlapping states
  are flattened by the display's own priority.

#### 3.3.6 Action boundaries are command-driven

- **Colour never determines an action boundary. The command does.** `action_start` = a command
  completion (button press matching a move command in the external move DB) followed by entry
  into `counter_state` (green) within `W'` frames.
- **A green run with no preceding command completion is an internal gap of the ongoing move**
  (multi-hit), not a new action. Do not emit an `action_start` for it.
- Consequently neither green alone, nor red, nor blue may be used as an action boundary.
- `startup` = length of the **first** green run following the command completion. Later green
  runs within the same move are gaps and are never counted as startup. Every green span after the
  first carries `is_gap`; the first carries `is_startup` (see `move_state_events`).
- Command attempts, move identification and counter-hit counting are **analysis-layer** work
  against the external move DB. They are not emitted by this extractor; it only guarantees that
  the inputs and states required to compute them are present and frame-aligned.

### 3.4 Control type

- **Classic and Modern render different button icons, so control type is read from the HUD, never
  assumed.** Detect once per player during initialisation, in the same pass as names.
- `control_type_p1` / `control_type_p2` $\in$ {`classic`, `modern`}; independent per player.
- **Resolve before generic glyph matching, then match only the resolved set.** `L` (modern) is a
  prefix of `LK` (classic), so cross-set matching returns confident wrong labels. If neither
  resolves at $\ge 0.92$, set `null` and emit every button glyph as `"?"` + `unknown_glyph`.
- Direction, counter and numeric templates are shared by both control types.

### 3.5 Input history

- A row carries the ordered direction glyphs and button glyphs held at that frame plus the hold
  counter. No move names or IDs. Tracking: §4. Two ROI sets, P1 / P2, **position fixed**.

---

## 4. Input Tracking Logic

### 4.1 Row semantics

- **A row is a held-key snapshot, not a motion input.** Every key held that frame is drawn in one
  row (`↓`, `→`, `LP` may coexist). A motion such as `236` spans several rows.
- **19 rows are visible** (`R = 19`, config). A new row appears **only when the held set
  changes**; the oldest is discarded.
- **Only slot 0 (top) is read per frame.** A change always materialises as a new slot 0, so
  cross-slot shift detection is unnecessary and must not be implemented.

### 4.2 Counter and segment termination

- **The counter is the number of consecutive frames this exact held set has persisted**, starting
  at 1. It **increases** while held.
- **The display saturates at a maximum that is 99 or 98 depending on the display.**
  `COUNTER_MAX ∈ {98, 99}`; a value at or above the observed maximum sets `counter_clipped` and
  `hold_frames_lower_bound`. Never hard-code 99.
- **A segment ends when slot 0's signature changes, or its counter decreases.**
- The signature is glyphs only; **the counter is excluded** (it increments every frame).
- The counter decrease is the primary delimiter and resolves the case where the held set changes
  and later returns to an earlier signature.
- **Sampling a 60 fps display at 60 Hz cannot skip a row.** If the counter becomes inconsistent
  (dropped capture frame), re-read down the 19 visible rows and set `resync_used`.
- A hold beyond the cap is not recoverable: emit `hold_frames = COUNTER_MAX`, `counter_clipped`.

### 4.3 Layout and output order

- **Display order is mirrored and used only for slot geometry:** P1 counter → direction →
  buttons; P2 buttons → direction → counter. Slot geometry is declared **per player**
  (`row_slots_p1`, `row_slots_p2`). A shared slot template is wrong.
- **Output order is fixed and identical for both players: `directions[]` then `buttons[]`.**
  Never a concatenated string.
- Within each array glyphs are in **fixed library order** (`LP, MP, HP, LK, MK, HK` / `L, M, H,
  SP, DP, DI, A`; directions likewise) — never display, press or matching-loop order. A signature
  must be identical for the same held set on P1 and P2.
- **Input history ROIs are fixed in canonical space and never offset**; no anchor-mode option.
- **Input extraction is independent of calibration**; records are emitted even if both HUD groups
  fail.

### 4.4 Emission

An open segment finalises on a new signature at slot 0, a counter decrease, a round boundary, or
end of replay.

- `frame_start` = first logical frame at slot 0; `frame_end` = last (**inclusive**).
- `frame_span_video = frame_end - frame_start + 1`.
- `hold_frames` = last counter read at slot 0.
- `hold_frames` and `frame_span_video` are **never reconciled**. During KO slow-motion or SA
  freezes they legitimately differ; a difference $>2$ sets `time_dilation_suspected`.
- **Never drop an unread glyph:** emit `"?"` + `unknown_glyph`. Combo macros drawn as separate
  glyphs are separate array entries.
- If the input ROI never yields a glyph across calibration, set `input_history_available=false`
  and emit no input records — a first-class outcome, not an error.

### 4.5 Round phase and boundaries

- **Two distinct boundaries, never conflated:**
  - `input_accept_start_frame` — key input begins to be recorded (intro start). The colour meter
    does **not** change here.
  - `round_start_frame` — the character can act. The **whole Frame Meter and both meters clear to
    black** at this instant, and 11-state classification resumes.
- **Input is recorded from `input_accept_start_frame` to the next round boundary.** These
  observations are **never filtered or discarded** — they are frequently deliberate buffering.
  They are only classified: records with `frame_start < round_start_frame` carry `is_preround`
  and `NON_ACTIONABLE`.
- **Round boundaries come from round start logo ("Round ?" / "Final Round") / round end logo ("K.O." / "TIME OVER") / timer reset / HP reset (§7.2), never from the input history.**
  The input history persists across rounds and is displaced only by the 19-row limit. A segment
  is **never split** at a boundary; round membership is recovered by joining on frames.
- **`round_start_frame` is never cross-checked with the yellow fill.** The yellow exists only
  from round 2 onward, so using it would provide coverage that silently does not exist in round
  1, and deciding whether it carries information requires the round index being verified.

---

## 5. Gauge Detection Failure Handling

Gauge extraction may fail temporarily (SA animation, Drive Impact flash, motion blur,
compression noise).

1. **Single-frame failure:** carry the last detected value forward, tag `CARRY_FORWARD`.
2. **Consecutive failures:** after $N=10$ failed frames, record `null` for the segment, tag
   `OCCLUDED`, log a warning.
3. **Recovery:** resume from the newly detected value.
4. **Input history is unaffected**; gauge failure never resets input segments.

**Carry-forward state is per round and reset at `round_start_frame`.** A black meter after round
start yields `null`, never the previous round's value. Round 1 opens black; from round 2 the
previous winner's meter stays black until that player acts. Neither is a detection failure.

Carried values remain distinguishable from measured values via `quality_flags`.

---

## 6. Implementation Notes (normative)

### 6.1 Raster normalization

- Scale every input to a **canonical raster of `1920x1080`** before any ROI or template operation.
  All coordinates, templates and thresholds are canonical. Ratio coordinates are annotation only.
- **Crop letterbox / pillarbox first.** Content aspect differing from 16:9 by $>0.010$ rejects
  with `E_ASPECT_UNSUPPORTED`; isotropy required within $0.005$ else `E_ANISOTROPIC_SCALE`. A
  misaligned raster must never be processed.
- Standard capture is `1920x1080 / 60 fps`; other 16:9 resolutions are accepted and normalised.
- No multi-scale matching and no pyramid (§2.2).
- **HUD absent $\Rightarrow$ do not analyse.** Emit `E_HUD_ABSENT` with a diagnosis (§2.6), stop.

### 6.2 Timeline and frames

- Logical **60 Hz** timeline. `frame_index` is the 0-based ordinal; `source_frame_index` is the
  originating decoded frame.
- `timestamp_ms` is an **integer** from the source PTS mapped to its logical slot — **not**
  `frame_index / 60` (that drifts ~0.3 s per 10 min at 59.94 fps).
- `frame_end` is **inclusive**. No float time in any table.

### 6.3 Configuration and confidence

- Config (JSON/YAML) declares `coordinate_space: "canonical"`, `canonical_raster: [1920,1080]`,
  anchors, search bands, row slot geometry and glyph sets. A different declared raster is a hard
  error.
- `config_version`, `template_pack_version`, `extractor_version` are recorded in
  `match_metadata`.
- Confidence thresholds: $0.85$ display elements, $0.90$ calibration and Frame Meter fields,
  $0.92$ input glyphs and attack property words. Below threshold $\Rightarrow$ undetected.

---

## 7. Output Data Structure

Flat records mapping directly to SQLite. Undetected fields are `null`.

### 7.1 `match_metadata` (once)

```json
{
  "match_id": "uuid-or-hash",
  "character_name_p1": "RYU",
  "character_name_p2": "A.K.I.",
  "control_type_p1": "classic",
  "control_type_p2": "modern",
  "detected_at_frame": 1,
  "source_resolution": "1920x1080",
  "source_fps": 60.0,
  "canonical_raster": [1920, 1080],
  "top_y_offset_px": 12,
  "bottom_y_offset_px": -18,
  "offset_snapped": false,
  "manual_verified": null,
  "input_history_available": true,
  "attack_property_locale": ["en", "ja"],
  "extracted_at_utc": "2026-11-15T00:00:00Z",
  "config_version": "cfg-1.0",
  "template_pack_version": "sf6-ui-2026.11",
  "extractor_version": "0.1.0"
}
```

`match_id` material: `video_content_hash + canonical_raster + config_version +
template_pack_version + top_y_offset_px + bottom_y_offset_px + control_type_p1 + control_type_p2
+ extractor_version`. Deterministic; manual offsets change it (§2.7).

### 7.2 `frame_telemetry` (per frame)

```json
{
  "match_id": "uuid-or-hash",
  "frame_index": 1042,
  "source_frame_index": 1042,
  "timestamp_ms": 17367,
  "round_index": 1,
  "hp_reset": false,
  "hp_p1": 0.955, "drive_p1": 5.0, "in_burnout_p1": false, "sa_p1": 2,
  "hp_p2": 0.820, "drive_p2": 3.5, "in_burnout_p2": false, "sa_p2": 1,
  "quality_flags": []
}
```

`round_index` and `hp_reset` are the authoritative round boundary signal (§4.5).

### 7.3 `input_events` (one per finalised held-state segment)

A `236` + `LP` spans **four rows**.

```json
[
  { "match_id": "h", "player": "P1", "round_index": 1, "event_index": 40,
    "directions": ["2"], "buttons": [],
    "frame_start": 1020, "frame_end": 1021, "frame_span_video": 2,
    "hold_frames": 2, "counter_clipped": false, "hold_frames_lower_bound": false,
    "time_dilation_suspected": false, "resync_used": false, "unknown_glyph": false,
    "is_preround": false, "quality_flags": [] },

  { "match_id": "h", "player": "P1", "round_index": 1, "event_index": 43,
    "directions": ["6"], "buttons": ["LP"],
    "frame_start": 1026, "frame_end": 1047, "frame_span_video": 22,
    "hold_frames": 22, "counter_clipped": false, "hold_frames_lower_bound": false,
    "time_dilation_suspected": false, "resync_used": false, "unknown_glyph": false,
    "is_preround": false, "quality_flags": [] }
]
```

Dense per-frame input state is a **derivation** and must not be stored as an independent source.

### 7.4 `move_events` (one per accepted Frame Meter block)

```json
{
  "match_id": "h", "round_index": 1, "frame_start": 1500, "frame_end": 1560,
  "fm_startup_p1": 6, "fm_total_p1": 42, "fm_advantage_p1": 3,
  "fm_startup_p2": null, "fm_total_p2": null, "fm_advantage_p2": null,
  "damage_p1": 1200, "combo_damage_p1": 2400, "attack_property_p1": "Mid",
  "damage_p2": null, "combo_damage_p2": null, "attack_property_p2": null,
  "attack_property_locale": "en",
  "conf": 0.94, "quality_flags": []
}
```

### 7.5 `move_state_events` (one per observed state span)

```json
{
  "match_id": "h", "round_index": 1, "move_event_id": 7, "player": "P1",
  "frame_start": 1520, "frame_end": 1525,
  "state": "counter_state",
  "is_startup": true, "is_gap": false,
  "conf": 0.93, "quality_flags": []
}
```

- `state` is one of the 12 values (§3.3.4).
- `is_startup` / `is_gap` are meaningful **only** when `state = counter_state`; otherwise both are
  `false`. `is_startup = true` marks the first green run after a command completion; `is_gap =
  true` marks later green runs within the same action.
- `move_event_id` is `null` if the span overlaps no `move_events` record.
- `round_index` is always set, since spans never cross a round boundary.
- Dense per-frame state is a **derivation** of this table (expand `frame_start..frame_end`) and
  must not be stored as an independent source.

### 7.6 Validation invariants (before write)

1. `frame_end >= frame_start`
2. `frame_span_video == frame_end - frame_start + 1`
3. `hold_frames >= 1`
4. `counter_clipped` $\Rightarrow$ `hold_frames == COUNTER_MAX` and `hold_frames_lower_bound`
5. `event_index` strictly increasing per `(match_id, player)`
6. `time_dilation_suspected == (|hold_frames - frame_span_video| > 2)`
7. `directions` precedes `buttons`; both arrays, never strings
8. No `input_anchor_mode`; no per-frame `fm_*` in `frame_telemetry`
9. `move_state_events.state` is one of the 12 enumerated values
10. `move_state_events.round_index` equals its `move_events.round_index` when `move_event_id` is
    set
11. `is_startup` and `is_gap` are `false` unless `state = counter_state`; they are never both true

Violations: `E_INPUT_INVARIANT` / `E_SCHEMA_INVARIANT`.

`quality_flags` vocabulary: `CARRY_FORWARD`, `LOW_CONF`, `OCCLUDED`, `NON_ACTIONABLE`,
`fm_advantage_sign_conflict`, `fm_startup_bar_conflict`, `unknown_cell_state`, `bar_cleared`,
`bar_head_mismatch`, `unexpected_cannot_act`, `unknown_glyph`, `resync_used`.

---

## 8. Template Image Specifications

### 8.1 Format and preprocessing

- **Lossless PNG** (24-bit RGB or 8-bit grayscale). JPEG prohibited.
- Text/name/digit/glyph templates converted to **grayscale**.
- **1–2 px background margin** around each glyph or frame edge.
- Authored at `1920x1080`. **No runtime scaling** (§2.2).

### 8.2 Library sets

| Set | Members | Notes |
|---|---|---|
| `dir` | `1,2,3,4,6,7,8,9` | Neutral = absence of a direction glyph |
| `btn_classic` | `LP, MP, HP, LK, MK, HK`, `classic` | `control_type = classic`. `classic` is the control-type badge (§8.2.2), not a button glyph. |
| `btn_modern` | `L, M, H, SP, DP, DI, A`, `modern` | `control_type = modern`. `modern` is the control-type badge (§8.2.2), not a button glyph. |
| `num` | `0`–`9`, `+`, `-`, `F` | shared by §3.3 and counters |
| `names` | one crop per character | §3.1. Full cast coverage. |
| `anchor_*` | per §2.3 | static HUD frames only, purity-verified |
| `attack_property_<locale>` | exactly 4 crops | `en` minimum; additional locales added as packs become available |

`template_pack_version` must contain both button sets and at least one attack property pack.

#### 8.2.2 Control-type badge (member of each button set)

The HUD shows a per-player badge indicating whether that player uses Classic or Modern
controls. This badge is the authoritative source for `control_type_p1` / `control_type_p2`
(§3.4) and must be resolved **before** any button-glyph matching, because `L` (modern) is a
prefix of `LK` (classic) and cross-set matching produces confident wrong labels.

The badge lives **inside its own button set** rather than in a separate directory:

- `btn_classic/classic.png` — the badge shown when the player uses Classic controls
- `btn_modern/modern.png` — the badge shown when the player uses Modern controls

Matching the badge therefore both identifies the control type and selects the glyph set in a
single operation. It also makes an incomplete pack structurally impossible: a pack that has
button glyphs but no badge cannot exist, because the badge is a member of the same set.

- Authored at `1920x1080`, grayscale, with 1–2 px background margin (§8.1).
- Purity requirement is relaxed relative to anchors: the badge is static during a match, but
  it may sit near animated HUD elements. Crops must exclude any pixel that changes while the
  badge is visible.
- Matching threshold: `template_thresholds.input_glyph` (0.92). If neither badge reaches the
  threshold, `control_type` is `null` and every button glyph is emitted as `"?"` with
  `unknown_glyph` set (§3.4).
- The two badges must be visually distinct. If a replay shows a badge variant not covered by
  these two crops, add it as a third template and update this table.

**Implementation note.** `classic` and `modern` are set classifiers, not button glyphs. When
building the glyph list for a set, exclude these two files explicitly, and treat their
presence as the set's completeness check. Matching a badge against a small button ROI is
harmless — the badge is far larger and has a different aspect ratio, so it will not produce a
false positive — but it wastes one comparison per frame.

---

## Change Log (3.0 → 4.0)

| Section | Change |
|---|---|
| §2 | Calibration replaces top/bottom detection: continuous offsets, no snapping, no scale search, anchor purity, Y-only search, prominence gate, consensus, per-group fail-closed |
| §2.7 | Manual offset fallback; non-interactive defaults to `fail`; offsets enter `match_id` |
| §3.1 | Names alphabet in all locales; `character_id` enum dropped |
| §3.3 | Frame Meter latched; two ROI groups; damage/property below timer; attack property = 4 localized words; advantage sign colour; meter present but not a frame-count source |
| §3.3.3 | Total never taken from the meter; Startup cross-checked by strict temporal counting only |
| §3.3.4 | Cell state model: 60-cell ring, 1 cell/frame, black = no state, green = counter-able, yellow = cannot-act, per-meter reset, 12 states |
| §3.3.5 | Phase composition; Startup = cells before first red (1-based, strict check); Total digit-authoritative |
| §3.3.6 | Action boundaries command-driven; green is never an action boundary; gaps vs startup |
| §3.4 | Control type read from the HUD (new) |
| §3.3.2 | Advantage accepts a bare positive number (`17F`) |
| §4 | Row = held-key snapshot; 19 rows; slot 0 only; counter is the delimiter and increases; output `directions`→`buttons`; per-player slot geometry; input ROIs fixed |
| §4.5 | Two boundaries (`input_accept_start_frame` vs `round_start_frame`); preround inputs classified, never dropped; round boundaries from HP/timer |
| §5 | Carry-forward reset per round |
| §6 | Canonical raster + aspect gate; integer PTS; inclusive `frame_end` |
| §7 | `match_id` material, `round_index`, `quality_flags`, per-player `move_events`, `move_state_events`, invariants |
| §8 | Both button sets and per-locale attack property packs |

### Confirmed UI facts

| Item | Confirmed |
|---|---|
| Input row semantics | All keys held that frame, one row |
| Visible rows | 19, oldest discarded |
| Row creation | Only when the held set changes |
| Counter | Increases with the held set; caps at 98/99 |
| History reset | Persists across rounds; displaced only by the 19-row limit |
| Input record window | Starts at intro (`input_accept_start_frame`), before the character can act |
| Display order | P1: counter/dir/buttons — P2: buttons/dir/counter |
| Output order | `directions` → `buttons`, both players |
| Input history position | Fixed; never offset; independent of calibration |
| Control type | Readable from button icons; both sets shipped |
| Frame Meter | Startup / Total / Advantage per player, unit `F` |
| Centre group | Damage / Combo Damage / Attack Property per player, below timer |
| Attack property | 4 localized words; no colour cue; per-locale templates |
| Advantage sign | `+` blue / `-` red; zero renders `0F`; positive may render bare (`17F`) |
| Colour meter | 60-cell ring, 1 cell/frame, wraps, continuous, never rewinds mid-round |
| Meter reset | Winner → black at victory; whole meter clears at round start |
| Green cell | Counter-able window (pre-hitbox and inter-hitbox gaps); not inherently startup |
| Yellow cell | Action-impossible (hitstun / hit / thrown) |
| Multi-hit | Inter-hitbox green gaps are real counter-able windows; only the first green is startup |
| Counter hit | Green interrupted → yellow; counted in analysis, not by the extractor |
| Numerals | Arabic in every locale |
| HUD position | Continuous, upper/lower independent |
| HUD scale | Not configurable |
| Character names | Alphabet in every locale; P1 left, P2 right |
| Capture | 1920x1080 / 60 fps; HUD absent ⇒ `E_HUD_ABSENT` + diagnosis |

---

## Not normative (implementation notes)

Counter re-sync heuristics, median/mean choice, colour-space choice and margin for cell
classification, caching strategies, calibration window and stride tuning, capture-profile
persistence, drift monitoring, VFR handling, OpenCV distribution, performance targets and
acceptance datasets.

Analysis-layer inputs measured at runtime, not fixed here but required to be measured, never
guessed: `W` (input buffer window), `W'` (command completion → green entry tolerance). A wrong
value makes attempt detection visibly inconsistent (`command_unmatched` / boundaries landing wrong),
so it fails loudly and does not block release.

Out of scope by design (require a move DB or additional extraction, not this document):
move identification, command-attempt classification, counter-hit counting, spacing/distance
analysis.