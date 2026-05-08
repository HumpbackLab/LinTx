# Global Keyboard Overlay Design

## Context

The current UI is driven by Rust state and `UiInputEvent` rather than LVGL input groups. SDL, fbdev touch, terminal keys, and injected inputs are normalized into events, then `UiApp` updates `UiFrame` and the LVGL backend renders the frame. This means a keyboard should follow the existing state-driven model instead of relying on LVGL's built-in focus handling.

The keyboard should visually follow LVGL's keyboard-with-text-area example: an input area above a large on-screen keyboard. It must also support LinTX's selection navigation: direction keys move a blue selection outline, `Open` activates the selected key, and touch taps select the touched key.

## Scope

Build a global keyboard overlay that future pages can call. The first integration point is ELRS `Bind Phrase`.

For `Bind Phrase`, pressing `Open` on the selected row first shows a `Click to edit` feedback prompt. Pressing `Open` again while the same field remains selected opens the keyboard overlay.

## User Experience

The overlay covers the active app page.

The top one third of the screen is a white input area:

- Left side: back button to cancel editing.
- Center: field label and text input.
- Right side: OK button to submit.
- The input box, its text, and the left/right action buttons must be large enough for landscape use. Their height should be at least comparable to the keyboard keys below.

The bottom two thirds of the screen is a white keyboard:

- Layout starts with a normal text keyboard: letter rows, shift/case toggle, numeric/symbol mode, space, cursor left/right, backspace, and done.
- Key sizes should stay close to the visual mockup: large rectangular touch targets with modest spacing.
- The selected key is outlined with a blue border.
- Tapping a key selects it and activates it.
- Direction input moves selection across the key grid. `Open` activates the selected key.

Save behavior:

- The keyboard is generic and allows broader text input.
- Field-specific validation runs only on submit.
- For `Bind Phrase`, invalid input does not close the overlay. The input box border turns red briefly, editing continues, and focus returns to the current input.
- Valid input saves the field and closes the overlay.

## Architecture

Add a reusable keyboard state module at `src/ui/keyboard.rs`.

The module owns:

- Open/closed overlay state.
- Target field metadata: label, initial value, max length, and validation kind.
- Editable buffer and cursor.
- Keyboard mode: lower, upper, numeric, symbols.
- Selected key position.
- Transient validation state for red input-box feedback.

`UiFrame` will include optional keyboard overlay state so every backend receives the same state. `UiApp` will intercept input while the overlay is open before dispatching to the active app module.

The ELRS Scripts app will request the keyboard through a generic app context action instead of owning keyboard rendering. First integration uses the local and rf-link `Bind Phrase` edit path.

## Events And Data Flow

Normal app flow:

1. User selects `Bind Phrase`.
2. First `Open` emits `Click to edit` feedback and records that the field is armed.
3. Second `Open` opens the global keyboard with the current bind phrase.

Keyboard flow:

1. `UiApp::apply_event` detects an active keyboard overlay.
2. Direction events update selected key.
3. `Open` activates the key under selection.
4. Back/cancel closes without saving.
5. OK/done validates and either closes with a committed value or shows invalid state.

Touch flow:

1. Pointer hit-testing checks keyboard overlay before app/launcher hit-testing.
2. A tap on a keyboard key updates the selected key and activates it.
3. Taps on back or OK trigger the same cancel/submit actions as button navigation.

## Validation

The keyboard itself does not enforce the bind phrase character set while typing.

`Bind Phrase` validation on submit:

- Accept only ASCII lowercase letters, digits, hyphen, and underscore.
- Reject empty input.
- Enforce the existing 32 byte maximum.
- Reject uppercase letters and other characters instead of silently normalizing them.

If rejected, the input area shows a red border briefly and remains open. The current buffer is preserved so the user can fix it.

## Rendering

LVGL backend adds overlay objects on top of the app panel:

- A white top panel sized to one third of the display height.
- Large back and OK buttons matching keyboard key height.
- A large text input box with normal and invalid border styles.
- A keyboard panel occupying the bottom two thirds.
- Fixed key rectangles for the current mode.

The overlay should be hidden when inactive and should not disturb the launcher/app transition logic.

Terminal backend will render a compact editing summary with the current label, buffer, selected key, and validation status. Full visual parity is limited to LVGL backends.

## Tests

Add unit tests for the keyboard state module:

- Moving selection with directions.
- Touch selecting a key by row/column.
- Character insertion and cursor movement.
- Backspace, space, shift, numeric/symbol mode, and done behavior.
- Invalid submit keeps overlay open and marks input invalid.
- Valid submit returns a committed value.

Add app-level tests for the ELRS Bind Phrase flow:

- First `Open` on the field emits `Click to edit` and does not open the overlay.
- Second `Open` opens the overlay.
- Invalid save preserves edit state.
- Valid save updates the same storage path currently used by Bind Phrase.

Run at least:

- `cargo test`
- `cargo check --features sdl_ui`
