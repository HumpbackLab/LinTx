# AUX Channel Mapping Design

## Goal

Replace the unused `TRAINER` app with an `AUX MAP` app that configures how physical switch/button inputs drive ELRS RC output channels `CH5` through `CH16`. Update the `CONTROL` app so its channel display reflects actual input and mixer output instead of showing the raw input channel count as `Count`.

The first version is intentionally simpler than EdgeTX's full mixer model. It supports one configured source per AUX output channel, while allowing the same source to be reused by multiple output channels.

## Non-Goals

- No EdgeTX-style stacked mixer rows, additive mixes, replace/multiply semantics, curves, or source conditions.
- No logical switches, flight modes, special functions, telemetry conditions, or scripted sources.
- No CRSF digital switch/subset frame support. The output remains the existing 16-channel CRSF RC packed frame path.
- No change to `CH1` through `CH4` stick calibration and output profile behavior.

## EdgeTX Reference Notes

EdgeTX represents mixes as independent ordered rows with a destination channel, source, weight, offset, multiplex mode, and optional switch condition. Multiple rows can target the same destination, and one source can drive multiple destinations. Physical switches used as value sources become analog values, while switch conditions are a separate signed source namespace.

LinTx will deliberately avoid row ordering and multiplex semantics in this feature. The AUX map will keep source identity separate from future condition identity, use explicit fields rather than signed sentinel IDs, and retain the current `0/5000/10000` mixer output range.

## Data Model

Add an `aux_mapping` section to `ModelConfig`.

```toml
[[aux_mapping.channels]]
channel = 5
source = "S1"
inverted = false

[[aux_mapping.channels]]
channel = 6
source = "SA"
inverted = false
```

Each entry maps one output channel number in `5..=16` to one source. Missing channels use defaults. If hand-written TOML contains duplicate entries for a channel, normalization uses the last valid entry for that channel. UI saves always write at most one entry per channel.

Default mapping preserves current behavior:

- `CH5 = S1`
- `CH6 = SA`
- `CH7 = SB`
- `CH8 = SC`
- `CH9 = SD`
- `CH10 = S2`
- `CH11..CH16 = none`

Supported source names:

- `SA`, `SB`, `SC`, `SD`: `switch_3pos[0..3]`, output `0/5000/10000`
- `S1`, `S2`: `switch_2pos[0..1]`, output `0/10000`
- `B0` through `B15`: `buttons` bit values, output `0/10000`
- `none` or empty string: output `0`

The same source may be used in multiple output channels.

`inverted = true` reverses the generated value after source scaling:

- two-position/buttons: `0 <-> 10000`
- three-position: `0 <-> 10000`, middle remains `5000`

## Mixer Behavior

The mixer continues to compute `CH1..CH4` from calibrated stick inputs and model output profiles.

For `CH5..CH16`, the mixer reads the active model's AUX mapping and latest `RcInputRawMsg`. If switch input is not present, all AUX outputs remain low/off. If a source is invalid or unavailable, it resolves to `0`.

The existing throttle-based arm safety remains mandatory: when throttle output is above `THROTTLE_ARM_MAX`, `CH5` is forced to `0` after AUX mapping is applied.

Existing model output profile handling for named roles `Arm`, `FlightMode`, `Beeper`, `Turtle`, `Prearm`, and `GpsRescue` should remain applied to `CH5..CH10` for backward compatibility. `CH11..CH16` do not need role output profiles in this feature.

## AUX MAP App

Replace `AppId::Trainer`'s visible app module with `AUX MAP` while keeping the launcher slot. The code may keep the enum variant temporarily if that is less disruptive, but user-facing strings and app module names should reflect AUX mapping.

The app follows the existing app architecture:

- 4 visible rows at a time.
- Rows represent `CH5` through `CH16`.
- Up/Down moves focus and scrolls the visible window.
- Open edits the focused channel's source using the existing global keyboard overlay.
- A successful submit writes the active model to disk and publishes `ActiveModelMsg` so the mixer updates immediately.
- Invalid source text keeps the keyboard open and marks it invalid.

The visible row should show the channel, configured source, and a compact live output value from `mixer_out.channels[channel - 1]`.

## CONTROL App

Remove the misleading `Count` display from CONTROL.

CONTROL should show:

- raw input `CH1-4` from `input_frame`
- mixer output `CH1-4`
- mixer output `CH5-8`
- mixer output `CH9-12`

This makes it clear that raw input currently carries stick axes, while AUX values are visible in mixer output.

## Error Handling

- Invalid source names submitted in the UI are rejected with error feedback and do not modify the model.
- Store write failures produce error feedback and do not publish `ActiveModelMsg`.
- Loading older model files without `aux_mapping` uses defaults.
- If the active model cannot be loaded, the app renders a usable error state rather than panicking.

## Testing

Add focused tests for:

- source parser accepts `SA..SD`, `S1..S2`, `B0..B15`, `none`, and rejects invalid text.
- default AUX mapping preserves current `CH5..CH10` behavior and leaves `CH11..CH16` low.
- the same source can drive multiple output channels.
- inversion works for two-position, three-position, and button sources.
- throttle safety still forces `CH5` low.
- older model TOML without `aux_mapping` loads with defaults.
- AUX MAP app navigation, keyboard submit, invalid submit, and active model publish behavior.
- CONTROL terminal/LVGL text no longer contains `Count`.

## Implementation Constraints

- Use existing `serde` TOML config patterns and `store::save_model_config`.
- Use the existing global keyboard overlay instead of creating app-specific text input widgets.
- Keep changes scoped to config, mixer, UI app catalog/app module, keyboard field identifiers, CONTROL display, and tests.
- Do not change CRSF packet packing or ELRS transport for this feature.
