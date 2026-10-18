---
name: HomeMaid
description: The home as a live CAD model. A furnished dollhouse room on a graphite construction grid, each device annotated with its true state.
colors:
  graphite-ground: "#15171B"
  graphite-panel: "#1C1F24"
  graphite-raised: "#252930"
  hairline: "#343A42"
  hairline-strong: "#4B525C"
  ink: "#EEF0F2"
  ink-dim: "#A2A9B3"
  selection-blue: "#7FA8FF"
  selection-blue-ink: "#0B1A3A"
  selection-blue-container: "#233656"
  physical-amber: "#FFB547"
  physical-amber-ink: "#2E1D00"
  fault-red: "#FF7468"
  swatch-off: "#8F96A0"
typography:
  display:
    fontFamily: "Barlow"
    fontSize: "34px"
    fontWeight: 600
    lineHeight: 1.1
    letterSpacing: "-0.4px"
  headline:
    fontFamily: "Barlow"
    fontSize: "24px"
    fontWeight: 600
    lineHeight: 1.2
  title-large:
    fontFamily: "Barlow"
    fontSize: "20px"
    fontWeight: 600
  title:
    fontFamily: "Barlow"
    fontSize: "16px"
    fontWeight: 600
    letterSpacing: "0.1px"
  title-small:
    fontFamily: "Barlow"
    fontSize: "14px"
    fontWeight: 600
    letterSpacing: "0.1px"
  body-large:
    fontFamily: "Barlow"
    fontSize: "16px"
    fontWeight: 400
    lineHeight: 1.35
  body:
    fontFamily: "Barlow"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: 1.35
  body-small:
    fontFamily: "Barlow"
    fontSize: "12px"
    fontWeight: 400
    lineHeight: 1.3
  label-large:
    fontFamily: "Barlow"
    fontSize: "15px"
    fontWeight: 600
    letterSpacing: "0.2px"
  label:
    fontFamily: "Barlow Semi Condensed"
    fontSize: "13px"
    fontWeight: 500
    letterSpacing: "0.2px"
  label-small:
    fontFamily: "Barlow Semi Condensed"
    fontSize: "12px"
    fontWeight: 500
    letterSpacing: "0.3px"
  figures:
    fontFamily: "Barlow"
    fontFeature: "tnum"
rounded:
  part: "6px"
  dialog: "10px"
  sheet: "12px"
  key: "9999px"
spacing:
  xs: "4px"
  sm: "8px"
  md: "12px"
  lg: "16px"
  gutter: "20px"
  xl: "24px"
components:
  button-filled:
    backgroundColor: "{colors.selection-blue}"
    textColor: "{colors.selection-blue-ink}"
    typography: "{typography.label-large}"
    rounded: "{rounded.part}"
    height: "48px"
  button-outlined:
    backgroundColor: "transparent"
    textColor: "{colors.ink}"
    typography: "{typography.label-large}"
    rounded: "{rounded.part}"
    height: "48px"
  button-text:
    textColor: "{colors.selection-blue}"
    rounded: "{rounded.part}"
    height: "48px"
  button-destructive:
    backgroundColor: "{colors.fault-red}"
    textColor: "{colors.graphite-ground}"
    rounded: "{rounded.part}"
    height: "48px"
  round-key:
    backgroundColor: "transparent"
    textColor: "{colors.ink}"
    rounded: "{rounded.key}"
    size: "56px"
  round-key-primary:
    backgroundColor: "{colors.selection-blue-container}"
    textColor: "{colors.selection-blue}"
    rounded: "{rounded.key}"
    size: "72px"
  device-row:
    textColor: "{colors.ink}"
    typography: "{typography.title}"
    padding: "8px 12px 8px 20px"
    height: "64px"
  callout-plate:
    backgroundColor: "{colors.graphite-ground}"
    textColor: "{colors.ink}"
    typography: "{typography.label}"
    rounded: "{rounded.part}"
    padding: "4px 6px"
    width: "124px"
  room-tab:
    textColor: "{colors.ink-dim}"
    typography: "{typography.title-small}"
    padding: "0 10px"
    height: "48px"
  room-tab-active:
    textColor: "{colors.ink}"
    typography: "{typography.title-small}"
    height: "48px"
  nav-bar:
    backgroundColor: "{colors.graphite-panel}"
    textColor: "{colors.ink-dim}"
    typography: "{typography.label}"
    height: "68px"
  input:
    backgroundColor: "{colors.graphite-panel}"
    textColor: "{colors.ink}"
    typography: "{typography.body}"
    rounded: "{rounded.part}"
    padding: "14px 12px"
  bottom-sheet:
    backgroundColor: "{colors.graphite-panel}"
    rounded: "{rounded.sheet}"
---

# Design System: HomeMaid

Language note: code and comments are in English; user documentation may be in French. This file is in English. Every UI string ships in French and English (`app/lib/l10n/`), and every label must fit both.

## Overview

**Creative North Star: "The Viewport"**

The home is a live CAD model. Each room is a furnished dollhouse cut-away (two walls, a lit cut section, dark furniture) standing on a graphite floor that is the real construction grid. The devices are studio-rendered satin clay parts set on its walls, furniture tops, windows, door, floor and ceiling, each annotated by a leader-line callout carrying its true state. The grammar is Fusion 360 / Onshape: graphite ground, hairline panels, small radii, a technical grotesk with tabular figures, one selection blue, and one amber that only ever means "physically on". It refuses the category default, a grid of rounded device tiles.

Depth lives in the renders, not in the chrome. The interface around the room is flat graphite planes separated by hairlines, so the room and its amber are the only things with volume or light. State changes happen in place: amber fades up, the blue hatch resolves to solid, the value flips inside its own box, and nothing else moves.

The PC remote belongs to the same world: round hairline keys in ink, service logos in ink, and a trackpad drawn as a sketch plane (dot grid, crosshair under the finger, live x/y readout).

**Key Characteristics:**
- Dark only: graphite grounds, built for evenings indoors.
- Amber is the device's reported state, never a wish.
- One state grammar everywhere: viewport, callouts, pick points, rows, sheet.
- Pre-rendered studio views (Blender Cycles), composited in Flutter; no real-time 3D.
- Barlow with tabular figures; condensed Barlow for annotation labels.
- Rectangular parts with a 6px radius and hairline borders; only remote keys are round.

## Colors

A graphite ramp, two inks, and three signal colours, each with exactly one meaning. All live in `Hm` (`app/lib/theme.dart`); never write a raw colour in a widget.

### Primary
- **Selection Blue** (`Hm.select`): interaction, focus, selection (active room tab underline, selected nav indicator at 16% alpha, selected row tint at 8%, selection halo in the viewport), and a command still pending (hatch, pending text, pending switch track). It also colours switches that power nothing (mute, settings) through `Hm.settingSwitch`.

### Secondary
- **Physical Amber** (`Hm.on`): a socket that powers something, a lit bulb, a running screen or PC. Light passes, floor light pools, pick points, the power switch track, summary text, and the remote's "connected" dot (a reachable agent means the PC is running).

### Tertiary
- **Fault Red** (`Hm.danger`): a failed command, an alarming sensor (smoke), an unreachable link (offline banner, remote header error), and the destructive shutdown action.

### Neutral
- **Graphite Ground** (`Hm.ground`): scaffold, viewport, callout plates (at 88% alpha), gallery tiles.
- **Graphite Panel** (`Hm.panel`): navigation bar, bottom sheet, dialog, input fill, trackpad plane.
- **Graphite Raised** (`Hm.raised`): snackbar, inactive switch and slider tracks.
- **Hairline** (`Hm.line`): dividers, callout plate borders, input borders, trackpad frame.
- **Hairline Strong** (`Hm.lineStrong`): outlined buttons, round key rings, trackpad dots, demo-data chip border.
- **Ink** (`Hm.text`) and **Ink Dim** (`Hm.textDim`): primary text; secondary text, off values, icons, offline outlines, leader lines.
- **Swatch Off**: the unlit fill of the state swatch only (`widgets/state.dart`).

### Named Rules
**The Observed Amber Rule.** Amber comes from the observed state only (`onLevel` in `app/lib/widgets/state.dart`). A pending wish never lights anything: the switch stays blue, the text is blue, the model is hatched, until the device confirms.

**The One Meaning Rule.** Blue is interaction and waiting; amber is physical power; red is failure, alarm or destruction. A toggle that powers nothing (mute, invert scroll, a boolean `custom:` capability) is blue, never amber.

**The Ink Logo Rule.** Service logos (Simple Icons, CC0, `app/assets/logos/`, see `app/tools/logos.md`) are drawn in ink like every other key, blue when active. Brand colours are refused.

## Typography

**Display Font:** Barlow (embedded, OFL, `app/assets/fonts/`, weights 400/500/600)
**Body Font:** Barlow
**Label Font:** Barlow Semi Condensed (500/600), `Hm.condensed`

**Character:** A technical grotesk read like dimension text on a drawing; the condensed cut carries annotations (callouts, nav labels, launcher names, trackpad readout) where width is scarce.

### Hierarchy
- **Display** (600, 34px, 1.1, -0.4 tracking): the screen title: current room name, "Settings".
- **Headline** (600, 24px, 1.2): sheet device name, remote PC hostname, empty-state title.
- **Title Large** (600, 20px): power sheet hostname.
- **Title** (600, 16px): device row name, settings list titles, launcher monograms (condensed).
- **Title Small** (600, 14px): room tabs, section headers (in Ink Dim), gallery section labels.
- **Body Large** (400, 16px, 1.35): capability labels and readings in the sheet, settings rows.
- **Body** (400, 14px, 1.35): row summaries, room count line, hints, field text.
- **Body Small** (400, 12px, 1.3): trackpad hint.
- **Label Large** (600, 15px, 0.2 tracking): button text.
- **Label** (condensed 500, 13px): callout name, nav labels, keyboard text keys.
- **Label Small** (condensed 500, 12px): callout value, overflow note "+N in the list", demo chip, gallery and launcher captions.
- The volume readout on the remote uses Material's default headlineMedium (28px), not a themed role.

### Named Rules
**The Tabular Figures Rule.** Every changing number (summaries, percent, watts, volume, counts, host:port, trackpad coordinates) sets `fontFeatures: Hm.tabular` so values flip without jitter.

**The Two Facts Rule.** A summary (`summary()` in `widgets/state.dart`) says at most two facts joined by " · ": "On · 70 %", "On · 410 W", "Locked · Closed". Watts keep one decimal only under 10 W.

## Layout

Single column, phone first (Android). Horizontal gutter 20px for headers, rows and sheet content; 16px on the remote; 12px for tabs and callout margins. Touch targets are at least 48dp everywhere (tabs 48, rows ≥ 64, capability tiles ≥ 56, keys ≥ 48 hit area).

**Home** (`app/lib/screens/home.dart`): room name (display) with a room-type button; count line ("8 devices · 5 on") plus the demo-data chip; scrolling room tabs; then the viewport taking half the remaining height, up to two thirds when the list is short; a divider; the device rows (dividers indented 50px to clear the swatch). Callouts sit in two columns (124 × 48 each), stacked by anchor height, never overlapping; at most 6 (`maxCallouts`), prioritised selected → failed → pending → on → list order, the rest noted "+N in the list".

**Remote** (`app/lib/screens/remote.dart`): header (hostname, status dot, keyboard and power keys), launcher strip (82px), the trackpad taking what is left, then playback (5 keys, 72px play in the middle) and volume (mute, stepper, skip-intro slot kept even when hidden so the stepper never moves). Keyboard mode swaps the bottom rows for a strip of 44px keys.

**Settings** (`app/lib/screens/settings.dart`): display title, sections under Title Small dim headers, list tiles, slider, setting switch.

## Elevation & Depth

The chrome is flat: no shadows, no surface tint (`surfaceTintColor` transparent on nav bar, sheet and dialog). Layers separate by tone (ground → panel → raised) and hairlines. All real depth comes from the pre-rendered rooms and devices: soft contact shadows baked into the device renders (edges faded by `pack.sh`), the cut walls, and amber light compositing.

### Named Rules
**The Light Is State Rule.** The only glows in the app are state: amber light passes (multiplied, plus a bloom, softer at 0.75 for screens and appliances), amber floor pools (0.13 for lamps, 0.24 for small-LED devices), a blue selection halo or red failure halo behind the device silhouette. No decorative shadow or glow on UI chrome.

## Shapes

Rectangular parts with gently softened corners (`Hm.radius`, 6px): buttons, inputs, tabs' ink, callout plates, gallery tiles, trackpad, nav indicator, snackbar. Dialogs 10px, bottom sheet top 12px. Borders are 1px hairlines; selection thickens a border to 2px in Selection Blue. Remote keys are the one round form: a hairline ring (1.5px, 2px for the primary key). State marks are geometric: square swatch, square hatch, square click mark on the trackpad, round pick points at leader ends.

## Components

### State grammar (signature, enforced in one place)
All state reads go through `app/lib/widgets/state.dart`: `onLevel`, `isPending`, `isFailed`, `alarming`, `stateColor`, `summary`. Never re-derive state in a widget.
- **On:** amber, from observed state; brightness scales it (0.35 + 0.65 × level).
- **Pending:** blue hatch (45°, every 4-5px) over the model or swatch, blue text, blue switch track, spinner in the capability tile.
- **Offline:** ghost (desaturated, 32% alpha) render; outline-only swatch and pick ring; switch at 38% opacity; summary "Offline".
- **Failed / alarming:** red halo on the model, red outline on the swatch, red pick point, red summary; the tile shows an error icon whose tooltip and snackbar carry the reason.
- **StateSwatch** (14px, 18px in the sheet): the legend of the grammar at the head of every row and the sheet header.
- **FlipText:** the new value slides up into the same box (220 ms, easeOutCubic in / easeInCubic out); zero duration under reduced motion.
- **PowerSwitch:** amber track only once confirmed, blue while pending, dimmed offline.

### Room viewport (signature)
`app/lib/viewport/room_viewport.dart`. Paint order: room view → amber floor pools → devices far to near (pendant cord and canopy, then sprite, halo, ghost or hatch, light passes, per-outlet marks on multi-entity devices) → leader lines and pick points. Callout plates are Graphite Ground at 88% with a hairline border, sized to their text so the room shows everywhere else; name in Label, value in Label Small coloured by `stateColor`; text scaling clamped at 1.15 because the rows carry the same text at full scale. Pick points carry state for devices whose own light is too small to see. Drag turns the room horizontally (0.008 rad/px) over its 90° arc with inertia, stops at the ends and settles on the nearest rendered view; devices and callouts step with the room's views rather than sliding.

### Device rows
`DeviceRow` in `home.dart`: swatch, name (Title), `FlipText` summary (Body, `stateColor`), `PowerSwitch` at the right edge or a chevron when there is no `on_off`. Selected row tinted Selection Blue at 8%.

### Device sheet
`DeviceSheet` in `home.dart`: swatch + name (Headline) + edit, then the status line in the grammar (Offline / failed with reason / Pending / summary), a divider, one `CapabilityTile` per capability (`app/lib/widgets/capability.dart`): `on_off` switch, `mute` setting switch, `brightness`/`volume` slider (follows the finger, sends on release), `power` reading in watts, and `custom:*` rendered generically from the JSON schema (boolean → setting switch; number → reading with unit; string → reading).

### Room tabs
`_RoomTabs` in `home.dart`: Title Small text, Ink when current with a 2px Selection Blue underline, Ink Dim otherwise; horizontal scroll, 48px tall.

### Gallery (model and room-type pickers)
`Gallery` in `home.dart`: 76px Ground tiles of the actual renders (`base_22` for models, `view_3` for rooms), hairline border, 2px blue when chosen, Label Small caption; an "Automatic" tile first names what it resolves to.

### Buttons and inputs
Material 3 themed in `theme.dart`: filled (Selection Blue, 48px min height), outlined (Hairline Strong side, Ink text), text (blue), destructive filled (Fault Red, only in the shutdown confirmation). Inputs are filled Panel with a Hairline border that becomes 1.5px Selection Blue on focus.

### Round keys (remote)
`_Round` in `remote.dart`: hairline ring in Hairline Strong, icon at 42% of the key in Ink; primary (play) and selected (keyboard open, mute engaged, active service) take Selection Blue with a 12% fill; the power key is Fault Red; disabled keys fade to Hairline ring and dim ink. Launchers are 56px keys with an ink logo (24px) or a condensed monogram, captioned in Label Small.

### Sketch plane (trackpad)
`SketchPlane` in `remote.dart`: Panel fill, 1.6px Hairline Strong dots every 22px, Hairline frame; under each finger a blue crosshair (70%), a 14px square and an `x 123   y 456` readout in condensed tabular blue; a click leaves a growing blue square that fades.

### Navigation
Material `NavigationBar` (68px, Panel): outlined icons (view_in_ar, mouse, tune) in Ink Dim, filled and blue when selected, on a 16% blue indicator with the 6px radius; labels in Label.

### Banners and empty states
`OfflineBanner` (`common.dart`): red hairline at 60% over red 8%, link-off icon. `EmptyState`: an empty generic room above the title and the pairing button, so even nothing-paired is the same world.

## Render and viewport conventions (Python ↔ Dart, keep in sync)

These are invariants between `app/tools/render/render.py`, `app/tools/render/room.py` and `app/lib/viewport/` (`sprites.dart`, `geometry.dart`, `rooms.dart`, `models.dart`). Changing one side without the other breaks every room.

- **Camera:** orthographic, pitch 0.62 rad (`PITCH` / `renderPitch`), `ORTHO_SCALE` 3.2 units across every device image (`orthoScale`). Axes: our (x, y, z) → Blender (x, −z, y); x along the back wall, y up, z towards the open side.
- **Device turntables:** 24 frames (`FRAMES` / `frames`); frame k is yaw k·2π/24 (15°), applied in Blender as an object rotation of −yaw about Z. The app picks `frameAt(cameraYaw − facing)`. Files: `assets/models/<name>/base_XX.webp` (lights off, contact shadow, 320px) and `glow<j>_XX.webp` per light part (white, 192px, tinted amber in the app by `BlendMode.modulate`). Pendants have a `<name>_hung` variant.
- **Rooms:** 7 kinds (`living, kitchen, bedroom, office, bathroom, entrance, generic`) × 3 sizes (`s` 4.5×3.6, `m` 6.0×4.5, `l` 7.5×5.4; walls 2.2 high), 7 views 15° apart over 90° (`VIEWS, STEP = 7, 15`, `views`/`step` in `rooms.json`). View i is the room seen at app yaw −i·STEP, so the camera yaw lives in [−90°, 0]; the default yaw −π/4 is view 3 (used for room thumbnails). A room's devices go in the smallest size that holds them all. Files: `assets/rooms/<kind>_<size>/view_<i>.webp` (1000px wide) plus `rooms.json` (framing, mounts, model bounds).
- **Facing:** 0 looks towards +z, 90° towards +x. Back wall at z = −D/2 facing +z, right wall at x = +W/2 facing −x; the open sides (+z, −x) face the camera along the whole arc. Floor devices turn to −45°, the open corner.
- **Mounts:** `wall`, `surface`, `window`, `door` runs and `floor`/`pendant` spots, declared in `room.py`, read by `rooms.dart`. Each model lists its preferred mounts and heights in `catalog` (`models.dart`), with a diorama scale (devices enlarged to stay readable, furniture life size).
- **Model choice:** by the user (`entity.model` in `homemaid.toml`) or inferred from capabilities (`inferredModel`), never from a brand. Strips are rendered with 2 to 6 outlets; more show as 6.
- **Floor grid:** brick texture, hairlines every half unit, baked into the room renders: the grid is the floor.
- **Pipeline:** from `app/`, `~/.local/opt/blender-5.2.2-linux-x64/blender -b -P tools/render/render.py`, the same for `room.py`, then `IMPECCABLE=<path> ./tools/render/pack.sh` (WebP conversion, shadow edge fade, provenance embedded in every file). Judge motion on a release build only.

## Do's and Don'ts

### Do:
- **Do** route every state colour through `stateColor` / `onLevel` / `isPending` in `app/lib/widgets/state.dart`.
- **Do** add a new capability by extending `CapabilityTile`, `capLabel` and, if it has a summary fact, `summary`; unknown `custom:*` capabilities must still render generically.
- **Do** add a new model in three places together: geometry in `render.py`, an entry in `catalog` (`models.dart`) with anchor, scale and mounts, and a label in `widgets/model_names.dart`; then re-render and re-pack.
- **Do** add a new room kind in `room.py` (all three sizes, its mounts), `roomKinds` and `inferRoomKind` in `rooms.dart`, and its FR/EN label.
- **Do** add a remote key as a `_Round`: hairline ring, ink icon, blue only when primary or selected, red only for destructive power.
- **Do** keep values in place: changing numbers use `FlipText` and tabular figures, and layout never moves on a state change.
- **Do** honour reduced motion (`MediaQuery.disableAnimations`): flips, easing and spin snap instantly.

### Don't:
- **Don't** light anything amber from a desired state, or use amber for a switch that powers nothing.
- **Don't** colour service logos in their brand colours.
- **Don't** add real-time 3D; extend the pre-rendered views and keep camera constants identical on both sides.
- **Don't** choose a model or room from a brand or plugin name.
- **Don't** add shadows, gradients or glows to UI chrome; light in this world is state.
- **Don't** put device tiles in a grid; devices live in the room and in rows.
- **Don't** expect light on walls: lit devices pool amber on the floor only; light spilling onto walls is not done yet.

## Automations screen (Scénarios)

Read-only: automations are written in the core's `homemaid.toml` (`app/lib/screens/automations.dart`).

- **Row**: the last run's mark in the state grammar (amber square = acted, red = failed, outline =
  skipped by a condition, dim = never ran), the name, the first trigger as a sentence, red "N
  problems" when a selector matches nothing, and "Next: Today 22:00" in tabular figures.
- **Sheet**: When / Only if / Then as sentences built from the core's JSON (`triggerText`,
  `conditionText`, `actionText`; selectors humanized by `targetText`). Then "Try it" (outlined)
  and "Run now" (filled). The dry-run result sits in a hairline panel, blue-edged when it would
  run. Last runs follow, newest first.
- **Device sheet actions** (plugin-api 1.1): one-shot actions without parameters show as outlined
  buttons under the capabilities. Shut down is red and asks first, as on the remote. Actions never
  take amber or the pending hatch: they are not states.
