# Street Fighter 6 Command Notation Specification

## 1. Purpose and Design Philosophy
This specification explicitly separates sequential lever inputs (time-series) from simultaneous button presses (spatial-series) in *Street Fighter 6*. It aims to ensure easy parsing for data processing while preventing cognitive errors in human readability.

By unifying the `+` character strictly for simultaneous button presses (same-frame inputs) and reserving square brackets `[]` exclusively for multi-button combinations, all command structures for both Classic and Modern control types are logically encapsulated without visual clutter.

---

## 2. Component Definitions

### 2.1. Directional Inputs (Lever/D-Pad)
Uses the standard 1–9 numeric numpad notation corresponding to keyboard layouts. Neutral is represented as `5`.

### 2.2. Button Definitions

#### A) Classic Type Buttons
* **`LP` / `MP` / `HP`**: Light Punch / Medium Punch / Heavy Punch
* **`LK` / `MK` / `HK`**: Light Kick / Medium Kick / Heavy Kick
* **`P` / `K`**: Any Punch / Any Kick

#### B) Modern Type Buttons
* **`A`**: Assist Button
* **`SP`**: Special Move Button
* **`DI`**: Drive Impact Button
* **`DP`**: Drive Parry Button
* **`L` / `M` / `H`**: Single Light / Medium / Heavy Attack Button

#### C) Abstract Buttons (Common to Both Types)
* **`FULL`**: Simultaneous 2-Attack Button Press
  * *Classic:* Two punch or two kick buttons of the same attribute (e.g., `[LP+MP]` for OD moves)
  * *Modern:* Any two of the three attack buttons (`L`, `M`, `H`) pressed simultaneously

---

## 3. Notation Rules (Syntax Rules)

### 3.1. Single Button vs. Simultaneous Presses
* **Single Button Inputs:** Square brackets `[]` MUST NOT be used for single button presses (e.g., `236LP`, `6SP`, `5DI`).
* **Simultaneous Presses:** Multiple buttons pressed at the exact same frame MUST be enclosed in square brackets `[]` and connected with a `+` inside the brackets (e.g., `5[A+SP]`, `5[LP+LK]`, `236[FULL]`).

### 3.2. Coupling with Directional Inputs
* Directional numbers must be placed directly outside to the left of the button or bracket block without any spacing.
* *Single Button Structure:* `[Direction Number][Button]` (e.g., `6HP`, `236LP`, `6SP`)
* *Simultaneous Press Structure:* `[Direction Number][[Button A]+[Button B]]` (e.g., `6[A+SP]`, `5[LP+LK]`)

### 3.3. Button Priority Order Within Brackets
To eliminate notation inconsistencies, the button sequence within brackets is strictly fixed from left to right based on the following priority:
1. **Aided / System Button** (`A`)
2. **Normal Attack Button** (`LP`, `LK`, `L`, etc.)
3. **Extended / Special Button** (`SP`, `FULL`, etc.)
4. **Dedicated Manual System** (`DI`, `DP`)

* *Correct Example:* `6[A+SP]` / `5[LP+LK]`
* *Incorrect Example:* `6[SP+A]` / `5[LK+LP]` (Priority Violation)

### 3.4. Time-Series (Combo / Target Combo Sequence)
* Sequential inputs that are timed rather than pressed simultaneously must be connected using the greater-than symbol `>`. Spaces or other symbols are prohibited.
* *Example:* `5L>5M>236SP`

### 3.5. Official Frame Data Special Icons Textualization
Special conditional icons found in the official web frame data are textualized immediately after the target command using parentheses `()` with an uppercase identifier to avoid namespace conflicts with button definitions:
* **`(CHG)`**: Charge (Holding the directional lever for a set time) *Placed directly after the directional number*

### 3.6. Frame Data Value Range Notation (`hit` / `block`)
Frame data values that represent a range (e.g., `hit` and `block` advantage on hit/block) use the following notation:
* **Single value:** Plain number (e.g., `"5"`, `"-2"`)
* **Range:** Two dots `..` between start and end (e.g., `"0..11"`, `"-6..5"`)
* **Multiple ranges:** Comma-separated ranges (e.g., `"-3..6,10..24"`)
* **Negative ranges:** Minus sign precedes each number (e.g., `"-4..-1"`)

The `..` symbol is used instead of full-width wave dash `～` to avoid confusion with negative numbers (e.g., `-3`).

---

## 4. Master Samples (Control Type Comparison Table)

| Category | Description | Classic Notation | Modern Notation | Logical Explanation |
| :--- | :--- | :--- | :--- | :--- |
| **Normal / Single Special** | Toward + Button | `6HP` | `6SP` | Press the designated single button while holding the lever forward (6). No brackets needed. |
| **Simple / Special Move** | Hadouken input, etc. | `236LP` | `5SP` | Single button pressed immediately after a directional motion or at neutral. |
| **OD Special Move** | Overdrive Special Move | `236[FULL]` | `5[A+SP]` | Encapsulates the simultaneous press block in brackets separately from the motion. |
| **System Simultaneous** | Normal Throw | `5[LP+LK]` | `5[L+M]` | Defines a state where multiple buttons are pressed on the exact same frame. |
| **Directional Charge** | Sonic Boom, etc. | `4(CHG)>6LP` | `4(CHG)>6SP` | Represents holding a direction before executing the forward execution. |
