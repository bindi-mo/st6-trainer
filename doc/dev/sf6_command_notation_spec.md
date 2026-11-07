# Street Fighter 6 Command Notation Specification

## 1. Purpose and Design Philosophy
This specification explicitly separates sequential lever inputs (time-series) from simultaneous button presses (spatial-series) in *Street Fighter 6*. It aims to ensure easy parsing for data processing while preventing cognitive errors in human readability.

By unifying the `+` character strictly for simultaneous button presses (same-frame inputs), all command structures for both Classic and Modern control types are logically encapsulated.

---

## 2. Component Definitions

### 2.1. Directional Inputs (Lever/D-Pad)
Uses the standard 1–9 numeric numpad notation corresponding to keyboard layouts. Neutral is represented as `5`.

### 2.2. Button Definitions

#### A) Classic Type Buttons
*   **`LP` / `MP` / `HP`**: Light Punch / Medium Punch / Heavy Punch
*   **`LK` / `MK` / `HK`**: Light Kick / Medium Kick / Heavy Kick
*   **`P` / `K`**: Any Punch / Any Kick

#### B) Modern Type Buttons
*   **`A`**: Assist Button
*   **`SP`**: Special Move Button
*   **`DI`**: Drive Impact Button
*   **`DP`**: Drive Parry Button
*   **`L` / `M` / `H`**: Single Light / Medium / Heavy Attack Button

#### C) Abstract Buttons (Common to Both Types)
*   **`FULL`**: Simultaneous 2-Attack Button Press
    *   *Classic:* Two punch or two kick buttons of any strength (e.g., `[LP+MP]` for OD moves)
    *   *Modern:* Any two of the three attack buttons (`L`, `M`, `H`) pressed simultaneously

---

## 3. Notation Rules (Syntax Rules)

### 3.1. Grouping Simultaneous Presses
*   Any single button input or multiple buttons pressed simultaneously must be enclosed in square brackets `[]`.
*   Buttons inside the brackets that must be pressed at the exact same frame are connected with a `+`.
*   *Exception (Classic Compatibility):* For single button sequential inputs with no simultaneous presses, brackets may be omitted for better human readability (e.g., `236LP`). However, strict data parsing must enforce `236[LP]`.

### 3.2. Coupling with Directional Inputs
*   Directional numbers must be placed directly outside to the left of the square brackets without any spacing.
*   *Syntax Structure:* `[Direction Number][ [Button A] + [Button B] ]`

### 3.3. Button Priority Order Within Brackets
To eliminate notation inconsistencies, the button sequence within brackets is strictly fixed from left to right based on the following priority:
1.  **Aided / System Button** (`A`)
2.  **Normal Attack Button** (`LP`, `LK`, `L`, etc.)
3.  **Extended / Special Button** (`SP`, `FULL` / `OD`, etc.)
4.  **Dedicated Manual System** (`DI`, `DP`)

*   *Correct Example:* `6[A+SP]` / `5[LP+LK]`
*   *Incorrect Example:* `6[SP+A]` / `5[LK+LP]` (Priority Violation)

### 3.4. Time-Series (Combo / Target Combo Sequence)
*   Sequential inputs that are timed rather than pressed simultaneously must be connected using the greater-than symbol `>`. Spaces or other symbols are prohibited.
*   *Example:* `5[L] > 5[M] > 236[SP]`

### 3.5. Official Frame Data Special Icons Textualization
Special conditional icons found in the official web frame data are textualized immediately after the target command using parentheses `()` with a single-letter uppercase identifier to avoid namespace conflicts with button definitions:
*   **`(H)`** : Hold (Button held for a set duration / Charged version)
*   **`(J)`** : Jump (Airborne execution / Air-only move)
*   **`(C)`** : Charge (Holding the directional lever for a set time) ※Placed directly after the directional number
*   **`(D)`** : Derivative (Follow-up attack derived from the previous move)

---

## 4. Master Samples (Control Type Comparison Table)

| **Category** | **Description** | **Classic Notation** | **Modern Notation** | **Logical Explanation** |
|---|---|---|---|---|
| **Normal / Single Special** | Toward + Button | `6[HP]` | `6[SP]` | Press the designated single button while holding the lever forward (6). |
| **Simple / Special Move** | Hadouken input, etc. | `236[LP]` | `5[SP]` | Button pressed immediately after a directional motion or at neutral. |
| **OD Special Move** | Overdrive Special Move | `236[FULL]` | `5[A+SP]` | Encapsulates the simultaneous press block separately from the motion. |
| **System Simultaneous** | Normal Throw | `5[LP+LK]` | `5[L+M]` | Defines a state where multiple buttons are clicked on the exact same frame. |
| **Hold / Charge Move** | Charged Special Move | `236[HP](H)` | `5[SP](H)` | Explicitly states that the button must be held for a set duration. |
| **Directional Charge** | Sonic Boom, etc. | `4(C)>6[LP]` | `4(C)>6[SP]` | Represents holding a direction before executing the forward execution. |
| **Derivative Attack** | Target Combo, etc. | `5[MP] > 5[HP](D)` | `5[M] > 5[H](D)` | Follow-up input permitted only after the preceding move connects. |
| **Airborne Move** | Air Special Move | `9[MK+HK](J)` | `9[DI](J)` | Defines commands that can only be executed while in the air (Jumping). |
