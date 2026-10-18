---
version: 1
slug: "app-lib-screens-home-dart"
primary_target: "app/lib/screens/home.dart"
related_targets: ["app/lib/main.dart","app/lib/screens/remote.dart","app/lib/screens/settings.dart","app/lib/screens/pair.dart"]
---

# Surface brief: HomeMaid app (Home, PC remote, Pairing, Settings)

Scope: the whole Android app (Flutter). Mode: **Operate**: glance and toggle at home; eyes-off trackpad sessions on the PC remote. Audience: the author daily, plus open-source users meeting it at first launch. Must stay legible in use (the user's explicit failure condition), FR + EN labels, Material 3 structure (navigation bar, system Back, insets, 48dp targets).

Wanted from the brief: the object at the center (minimal 3D models of devices and rooms, Tesla-like), motion, premium/tech feel. Not in this round: logo, app icon.

## Direction contract

THESIS: The home is a live CAD model. The room is drawn as a flat-shaded 3D maquette on a real construction grid, each device a part annotated with its true state. It refuses the category default: a grid of rounded device tiles.

OWN-WORLD: Graphite viewport ground and slab with a real construction grid; on it, studio renders of satin clay models (Blender Cycles, one iconic scale, soft contact shadows; amended at the user's request "les models sont trop primaires, je voudrais quelque chose de premium", which replaced the flat-shaded clay). One CAD-selection blue for interaction, selection and pending; one warm amber that only ever means "physically on", taken from the device's reported state, never from a wish. State grammar: solid = confirmed, blue hatch and blue text = pending, ghosted render (list: outline swatch) = offline, red = failed. Technical grotesk (Barlow) with tabular figures; leader-line callouts on shoulders; rectangular panels with hairline borders and small radii. The model is chosen by the user from a catalog of everyday objects, or inferred from capabilities. Amended at the user's request ("a realistic room layout instead of putting all models on a ground"): each room is a furnished dollhouse cut-away (graphite floor carrying the construction grid, two walls with a light cut section, dark furniture so the devices lead), one per room kind and size; devices sit on its walls, furniture tops, windows, door, floor and as pendants, at diorama scale (small devices enlarged to stay readable), the room size growing with the device count.

STORY: Open the app, see the room as an object and know at once what is on (amber), what is waiting (hatch) and what is gone (outline); touch a part or its row, act with a large control, watch the fill settle from hatch to solid.

FIRST VIEWPORT: Top: room selector (current room name large, other rooms as tabs). Upper ~55%: the room viewport, the furnished room with every device set in it, turnable by drag over a 90° arc, callouts naming each device with its state value. Below: the room's device rows (name, flipping value, primary toggle at the right edge, 48dp). Tapping a part, callout or row opens a bottom sheet with all capability controls. Navigation bar at the bottom.

FORM: CAD viewport (Fusion 360 / Onshape grammar), candidate 7 of 7 on my ordered list; seed key 7447feb3. Raises kept: fill-encoded state (Labanotation), in-place state flips in a fixed grid (split-flap), one scale/ground/light for all devices (botanical folio), the grid is the real floor (Crouwel), capped callout capacity with list overflow (j-card).

Signature interaction: turn the room with a drag over its 90° arc (inertia, stops at the ends, settles on a rendered view); state changes animate in place (amber fades up, the hatch resolves to solid, the value flips) without layout movement. On the PC remote, the trackpad is a sketch plane: dot grid, crosshair under the finger, live coordinate readout.

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance
