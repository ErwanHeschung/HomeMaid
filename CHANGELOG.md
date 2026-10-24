# Changelog

What changes for people using HomeMaid, version by version. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/). A release's notes on GitHub are its section here.

## [Unreleased]

### Added

- **Scenes.** Put several devices in a state with one tap: "Movie night" turns the TV on and dims the lights. They sit under the room tabs on Home; long-press one to edit it, or tap **+ Scene** and *Take the current state* to save the room as it is. A scene lights up when every device is really in it, shows pink while devices are still confirming, and red if one refuses. A scene can also run actions after setting its devices, like opening Netflix on the PC or starting playback. An automation can set a scene too.

- **Add a device from the app.** Home → **+**: a device of an installed plugin by its own form (a Tapo strip by its IP address and TP-Link login), a PC by its pairing code, or another one by installing its plugin first. The plugin checks it right away: a wrong password is said in the form. A second strip on the same account needs no login again.
- **Remove a device** from its panel, with all its parts.
- **Automations as workflows.** After its triggers and conditions, an automation is now a flow of steps drawn in the app, top to bottom, as in n8n: set a device, run an action, wait a while, wait until something happens (with a time limit), if / else, repeat (a number of times, while, until, or for each device), branches side by side joined back together, stop. Tap the **+** under a step to add the next one.
- **Use what started it**: a step can act on the device that triggered the automation, or copy its new value; a condition can tell which trigger it was.
- **What happens when it starts again while still running**: ignore it, start over, run it afterwards, or run both.
- **See where a run went**: each run shows its path on the flow; a run still waiting says where, and can be stopped. *Try* lights the path it would take now.

### Changed

- **Your automations keep working as they are.** Their list of actions opens as a chain of steps, and is saved as steps once edited.
- **A power strip is one device.** The list shows it once; its outlets are sections of its panel, each with its switch and its name. Moving it to another room moves all its outlets.
- **Installing a plugin asks for nothing but its permissions** when its devices are added one by one; its first device's form opens right away. TP-Link Tapo 0.2.0 works this way: update it from Settings → Plugins, then add your strip again from Home → +.

## [0.1.0-alpha.1] - 2026-10-23

The first public alpha: the hub, the PC remote and the Android app, tried on the author's own devices. Expect rough edges, and keep your manufacturer's app at hand.

### Highlights

- **Your home as a live model.** Each room is a furnished dollhouse with your devices placed in it. What is lit is really on: the app shows the state the device reports, never what it was asked to do. A command still on its way shows as pink hatching, a device out of reach as a ghost.
- **Your phone as the PC's remote.** Trackpad, keyboard, volume, media keys and power, plus a strip of services (Netflix, YouTube, Crunchyroll…) and of your PC's own apps. The remote follows what is in front on the PC: open Crunchyroll and its buttons are Crunchyroll's, with the episode playing shown above them.
- **Automations you can try before you trust them.** Write them in the app as sentences ("At sunset, on weekdays, turn on the living room lights"), then press *Try*: it says what would happen and why, and changes nothing. Schedules that a device can keep on its own are handed to it, so they keep running when the PC is off.
- **Plugins from the app.** Settings → Plugins lists the official plugins; installing one shows the permissions it asks for, checks its signature and starts it without restarting the hub.

### Added

- **Hub**: one `homemaid.exe` on Windows, started from the notification area; it writes its own config on first start, and pairs the phone with a QR code. Optional start with Windows.
- **Devices**: TP-Link Tapo P300 power strip, each outlet on its own (local network only).
- **App**: rooms and devices, a panel per device, renaming and moving devices between rooms, a room type to pick its furniture, a demo app with a sample home.
- **Remote**: add sites from a catalogue or by address, and your PC's Start menu apps with their icons; long press to remove one.
- **Automations**: triggers at a time, at sunrise or sunset (with an offset), or when a device changes; conditions on time, days and device states; a log of the last runs, and a run-away guard that stops an automation looping.

### Known issues

- **Install over an earlier build**: this release's APKs are signed with a temporary key. Uninstall a HomeMaid built elsewhere first; a later release will install over this one only once the signing key is fixed.
- **TP-Link P300**: turn on *Third-party compatibility* in the Tapo app (Me → Third-party services), and give the strip a fixed IP in your router. Schedules handed to the P300 are not yet tested on a real one.
- **PC control** is Windows only. Website detection reads the browser's address bar, which is hidden in fullscreen: the window title is used instead.
- When the PC is off or asleep, so is the hub: only schedules handed to devices keep running.
