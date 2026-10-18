# Product

<!-- impeccable:product-schema 1 -->

## Platform

android

## Users

- **The author**, daily, at home: controls his devices (TP-Link Tapo P300 power strip, connected bulbs to come, his Windows PC) from his Pixel 8 Pro, and uses the phone as a trackpad, keyboard and media remote for the PC.
- **Open-source users**: people who install HomeMaid on their own hardware. They meet the product through its repository, first launch and pairing, so identity and first impression matter, not only daily use. Officially supported devices are the author's own; contributors extend the rest through plugins.

## Product Purpose

A personal, open-source, local-first alternative to Home Assistant: a light Rust core running 24/7, a modern mobile app to control the home, and a native PC remote. Success is an app the author reaches for instead of vendor apps, and that others want to install.

## Positioning

- **Typed capabilities**, not free-form state strings: the UI is generated from capabilities, so a new plugin appears in the app with no app change.
- **Desired vs observed state** with a reconciler: every command is `pending`, `confirmed` or `failed`, and an offline device receives its desired state when it returns.
- **Sandboxed WASM plugins**: official plugins use the same public API as third-party ones; the core knows no brand.
- **Local first**: no cloud dependency for normal operation; remote access through Tailscale, no port exposed.
- **The phone as a PC remote** (trackpad, keyboard, media, apps), built into the same app.

## Operating Context

- Core and PC agent run on the author's Windows PC today (`C:\HomeMaid`, core as a Windows service, agent at logon); a dedicated always-on host is the long-term target.
- The app finds hosts by QR pairing (`/pair`, opened on the PC itself) and mDNS; data is live over WebSocket.
- Rooms, names and tags are user metadata saved to the versioned `homemaid.toml`.
- Usage moments: quick glances and toggles around the house; longer, eyes-on-the-PC trackpad sessions where the phone is held without looking at it.

## Capabilities and Constraints

- Screens today: Home (devices grouped by room, one control per capability, live updates), PC remote, Settings (pairings, remote speed, scroll direction), QR pairing.
- Built-in capabilities: `on_off`, `brightness`, `volume`, `mute`, `power` (sensor); plugins add `custom:*` capabilities rendered generically from their JSON schema (e.g. CPU, memory, active window).
- Every control must show command status (pending / confirmed / failed with reason) and device availability (offline).
- Flutter, Android only for now; no iOS or desktop app planned.
- Interface in **French and English**: labels must fit both languages.
- Undecided: automations UI (roadmap step 6), history/graphs (step 8), plugin registry UI (step 7).

## Brand Commitments

- Name: **HomeMaid**.
- The project brief asks for a mobile UI that is "modern and fluid, Tesla-style: visual, animated, designed for touch", explicitly not a dashboard of tiles in a WebView.
- No logo or visual identity exists yet.

## Evidence on Hand

- Working end-to-end product on real hardware (Windows PC, Pixel 8 Pro); P300 plugin tested against a protocol-faithful fake device only.
- No users besides the author, no testimonials, metrics or press: future work must not invent any.

## Product Principles

1. **Capabilities drive the UI.** Screens are generated from typed capabilities; nothing is hard-coded per brand or device.
2. **Truth over optimism.** Show what the device actually reports; a wish is visibly a wish until confirmed.
3. **Local and private by default.** No cloud, no account, no tracking.
4. **Touch first, glanceable.** Common actions in one tap; the remote works without looking at the phone.
5. **Open to contributors.** What a plugin declares must look first-class, not second-tier.
