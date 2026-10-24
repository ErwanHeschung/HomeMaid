import 'dart:math';

import 'package:flutter/material.dart';

import '../clients.dart';
import '../l10n/app_localizations.dart';
import '../model.dart';
import '../theme.dart';
import '../viewport/room_viewport.dart';
import '../viewport/models.dart';
import '../viewport/rooms.dart';
import '../widgets/capability.dart';
import '../widgets/model_names.dart';
import '../widgets/state.dart';
import 'automations.dart' show actionLabel;
import 'common.dart';
import 'plugins.dart';

class HomeScreen extends StatefulWidget {
  const HomeScreen({super.key, required this.core, required this.onPair});

  final CoreClient? core;
  final VoidCallback onPair;

  @override
  State<HomeScreen> createState() => _HomeScreenState();
}

class _HomeScreenState extends State<HomeScreen> {
  /// Selected room: its name, or '' for entities without one.
  String? _room;
  String? _selected;

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final core = widget.core;
    if (core == null) {
      return EmptyState(title: l.pairHomeTitle, hint: l.pairHomeHint, onPair: widget.onPair, hero: const Logo(size: 120));
    }
    return ListenableBuilder(
      listenable: core,
      builder: (context, _) {
        final rooms = byRoom(core.entities.values);
        final current = rooms.where((r) => (r.key ?? '') == _room).firstOrNull ?? rooms.firstOrNull;
        final entities = current?.value ?? const <Entity>[];
        final theme = Theme.of(context);
        final on = entities.where((e) => onLevel(e) > 0 && e.available).length;
        final roomName = current?.key;
        final kind = roomName == null ? 'generic' : core.roomKinds[roomName] ?? inferRoomKind(roomName);
        return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(20, 20, 8, 4),
            child: Row(children: [
              // The brand mark, on the root screen only (inner screens carry their own titles).
              const Padding(padding: EdgeInsets.only(right: 12), child: Logo(size: 30)),
              Expanded(
                child: Text(
                  current == null ? l.navHome : (roomName ?? l.noRoom),
                  style: theme.textTheme.displaySmall,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                ),
              ),
              if (roomName != null)
                IconButton(
                  icon: const Icon(Icons.chair_outlined),
                  tooltip: l.roomType,
                  onPressed: () => editRoom(context, core, roomName),
                ),
              IconButton(icon: const Icon(Icons.add), tooltip: l.addDevice, onPressed: () => _add(context, core)),
            ]),
          ),
          Padding(
            padding: const EdgeInsets.fromLTRB(20, 0, 20, 8),
            child: Row(children: [
              Flexible(
                child: Text(
                  [l.deviceCount(entities.length), if (on > 0) l.onCount(on)].join(' · '),
                  style: theme.textTheme.bodyMedium?.copyWith(color: Hm.textDim, fontFeatures: Hm.tabular),
                ),
              ),
              if (core.synthetic) ...[
                const SizedBox(width: 10),
                Container(
                  padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
                  decoration: BoxDecoration(
                    border: Border.all(color: Hm.lineStrong),
                    borderRadius: BorderRadius.circular(Hm.radius),
                  ),
                  child: Text(l.demoData, style: theme.textTheme.labelSmall?.copyWith(color: Hm.textDim)),
                ),
              ],
            ]),
          ),
          if (rooms.length > 1)
            _RoomTabs(
              rooms: [for (final r in rooms) r.key],
              current: current?.key,
              noRoom: l.noRoom,
              onSelect: (room) => setState(() {
                _room = room ?? '';
                _selected = null;
              }),
            ),
          if (!core.connected)
            OfflineBanner(l.homeUnreachable, detail: core.error == null ? null : describeError(context, LinkException(core.error!))),
          Expanded(
            child: LayoutBuilder(builder: (context, constraints) {
              // About half the screen, more when the list is short: no dead band of floor below.
              final groups = groupDevices(entities);
              final rows = groups.length * 65.0 + 8;
              final viewport = (constraints.maxHeight - rows).clamp(constraints.maxHeight * 0.5, constraints.maxHeight * 0.66);
              return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                SizedBox(
                  height: viewport,
                  child: RoomViewport(kind: kind, entities: entities, selected: _selected, onSelect: (id) => _open(context, core, id)),
                ),
                const Divider(),
                Expanded(
                  child: entities.isEmpty
                      ? Padding(
                          padding: const EdgeInsets.all(24),
                          child: Text(
                            core.connected ? l.noDevices : '',
                            textAlign: TextAlign.center,
                            style: theme.textTheme.bodyMedium?.copyWith(color: Hm.textDim),
                          ),
                        )
                      : ListView.separated(
                          padding: const EdgeInsets.only(bottom: 16),
                          itemCount: groups.length,
                          separatorBuilder: (_, _) => const Divider(indent: 50),
                          itemBuilder: (context, i) {
                            final parts = groups[i].entities;
                            return DeviceRow(
                              group: groups[i],
                              selected: parts.any((e) => e.id == _selected),
                              onTap: () => _open(context, core, parts.first.id),
                              onSet: (cap, value) => _set(context, core, parts.first.id, cap, value),
                            );
                          },
                        ),
                ),
              ]);
            }),
          ),
        ]);
      },
    );
  }

  /// Adding a device: a PC by its pairing code, a device of an installed plugin by its form, or
  /// another one by installing its plugin first.
  Future<void> _add(BuildContext context, CoreClient core) async {
    final l = AppLocalizations.of(context);
    await core.refreshPlugins().catchError((Object _) {});
    if (!context.mounted) return;
    final choice = await showModalBottomSheet<Object>(
      context: context,
      useSafeArea: true,
      builder: (context) => SafeArea(
        top: false,
        child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(20, 0, 20, 8),
            child: Text(l.addDevice, style: Theme.of(context).textTheme.headlineSmall),
          ),
          for (final p in core.plugins.where((p) => p.deviceSettings.isNotEmpty))
            ListTile(
              contentPadding: const EdgeInsets.symmetric(horizontal: 20),
              leading: const Icon(Icons.extension_outlined),
              title: Text(p.name),
              subtitle: p.description == null ? null : Text(p.description!, maxLines: 2, overflow: TextOverflow.ellipsis),
              onTap: () => Navigator.pop(context, p),
            ),
          ListTile(
            contentPadding: const EdgeInsets.symmetric(horizontal: 20),
            leading: const Icon(Icons.computer_outlined),
            title: Text(l.addPc),
            subtitle: Text(l.addPcHint),
            onTap: () => Navigator.pop(context, 'pc'),
          ),
          ListTile(
            contentPadding: const EdgeInsets.symmetric(horizontal: 20),
            leading: const Icon(Icons.add_circle_outline),
            title: Text(l.addOther),
            subtitle: Text(l.addOtherHint),
            onTap: () => Navigator.pop(context, 'plugins'),
          ),
          const SizedBox(height: 8),
        ]),
      ),
    );
    if (!context.mounted) return;
    switch (choice) {
      case InstalledPlugin plugin:
        await showDeviceForm(context, core, plugin);
      case 'pc':
        widget.onPair();
      case 'plugins':
        await showPlugins(context, core);
    }
  }

  Future<void> _open(BuildContext context, CoreClient core, String id) async {
    setState(() => _selected = id);
    await showModalBottomSheet<void>(
      context: context,
      isScrollControlled: true,
      useSafeArea: true,
      builder: (_) => DeviceSheet(core: core, id: id, onSet: (part, cap, value) => _set(context, core, part, cap, value)),
    );
    if (mounted) setState(() => _selected = null);
  }

  void _set(BuildContext context, CoreClient core, String id, String cap, Object? value) {
    core.setDesired(id, cap, value).catchError((Object e) {
      if (context.mounted) showError(context, e);
    });
  }
}

class _RoomTabs extends StatelessWidget {
  const _RoomTabs({required this.rooms, required this.current, required this.noRoom, required this.onSelect});

  final List<String?> rooms;
  final String? current;
  final String noRoom;
  final ValueChanged<String?> onSelect;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return SizedBox(
      height: 48,
      child: ListView(
        scrollDirection: Axis.horizontal,
        padding: const EdgeInsets.symmetric(horizontal: 12),
        children: [
          for (final room in rooms)
            Semantics(
              selected: room == current,
              button: true,
              child: InkWell(
                onTap: () => onSelect(room),
                borderRadius: BorderRadius.circular(Hm.radius),
                child: Container(
                  padding: const EdgeInsets.symmetric(horizontal: 10),
                  alignment: Alignment.center,
                  decoration: BoxDecoration(
                    border: Border(
                      bottom: BorderSide(color: room == current ? Hm.brand : Colors.transparent, width: 2),
                    ),
                  ),
                  child: Text(
                    room ?? noRoom,
                    style: theme.textTheme.titleSmall?.copyWith(color: room == current ? Hm.text : Hm.textDim),
                  ),
                ),
              ),
            ),
        ],
      ),
    );
  }
}

/// A device in the list: its fill swatch, name, flipping value and primary switch. A device of
/// several parts (a strip's outlets) is one row, its parts are in its sheet.
class DeviceRow extends StatelessWidget {
  const DeviceRow({super.key, required this.group, required this.selected, required this.onTap, required this.onSet});

  final DeviceGroup group;
  final bool selected;
  final VoidCallback onTap;
  final void Function(String cap, Object? value) onSet;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final parts = group.entities;
    final entity = parts.length == 1 ? parts.first : _shown(parts);
    final power = parts.length == 1 ? entity.caps['on_off'] : null;
    return Material(
      color: selected ? Hm.select.withValues(alpha: 0.08) : Colors.transparent,
      child: InkWell(
        onTap: onTap,
        child: ConstrainedBox(
          constraints: const BoxConstraints(minHeight: 64),
          child: Padding(
            padding: const EdgeInsets.fromLTRB(20, 8, 12, 8),
            child: Row(children: [
              StateSwatch(entity),
              const SizedBox(width: 16),
              Expanded(
                child: Column(crossAxisAlignment: CrossAxisAlignment.start, mainAxisSize: MainAxisSize.min, children: [
                  Text(
                    parts.length == 1 ? entity.name : modelLabel(context, group.model),
                    style: theme.textTheme.titleMedium,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                  ),
                  const SizedBox(height: 2),
                  FlipText(
                    parts.length == 1 ? summary(context, entity) : _partsSummary(context, parts),
                    style: theme.textTheme.bodyMedium?.copyWith(color: stateColor(entity), fontFeatures: Hm.tabular),
                  ),
                ]),
              ),
              if (power != null)
                PowerSwitch(entity: entity, onChanged: (v) => onSet('on_off', v))
              else
                const Padding(padding: EdgeInsets.all(12), child: Icon(Icons.chevron_right)),
            ]),
          ),
        ),
      ),
    );
  }
}

/// The part a device of several parts is drawn as: a failed one, else a pending one, else one on.
Entity _shown(List<Entity> parts) =>
    parts.where(isFailed).firstOrNull ??
    parts.where(isPending).firstOrNull ??
    parts.where((e) => onLevel(e) > 0 && e.available).firstOrNull ??
    parts.first;

String _partsSummary(BuildContext context, List<Entity> parts) {
  final l = AppLocalizations.of(context);
  if (parts.every((e) => !e.available)) return l.offline;
  final on = parts.where((e) => onLevel(e) > 0 && e.available).length;
  return on > 0 ? l.onCount(on) : l.stateOff;
}

/// The parts of [entity]'s physical device, [entity] alone if it has none.
List<Entity> deviceParts(CoreClient core, Entity entity) => entity.device == null
    ? [entity]
    : (core.entities.values.where((e) => e.device == entity.device).toList()..sort((a, b) => a.name.compareTo(b.name)));

/// Every control of one device, live; a device of several parts gets a section per part.
class DeviceSheet extends StatelessWidget {
  const DeviceSheet({super.key, required this.core, required this.id, required this.onSet});

  final CoreClient core;

  /// Any part of the device.
  final String id;
  final void Function(String id, String cap, Object? value) onSet;

  @override
  Widget build(BuildContext context) => ListenableBuilder(
        listenable: core,
        builder: (context, _) {
          final entity = core.entities[id];
          if (entity == null) return const SizedBox(height: 120);
          final l = AppLocalizations.of(context);
          final theme = Theme.of(context);
          final parts = deviceParts(core, entity);
          final single = parts.length == 1;
          final shown = single ? entity : _shown(parts);
          return SafeArea(
            top: false,
            child: SingleChildScrollView(
              padding: const EdgeInsets.only(bottom: 16),
              child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
                Padding(
                  padding: const EdgeInsets.fromLTRB(20, 0, 8, 4),
                  child: Row(children: [
                    StateSwatch(shown, size: 18),
                    const SizedBox(width: 14),
                    Expanded(
                      child: Text(single ? entity.name : modelLabel(context, DeviceGroup(parts).model), style: theme.textTheme.headlineSmall),
                    ),
                    IconButton(
                      icon: const Icon(Icons.edit_outlined),
                      tooltip: l.editDevice,
                      onPressed: () => editEntity(context, core, entity, whole: !single),
                    ),
                  ]),
                ),
                Padding(
                  padding: const EdgeInsets.fromLTRB(52, 0, 20, 12),
                  child: FlipText(
                    single ? _status(context, entity) : _partsSummary(context, parts),
                    style: theme.textTheme.bodyMedium?.copyWith(color: stateColor(shown), fontFeatures: Hm.tabular),
                  ),
                ),
                const Divider(),
                const SizedBox(height: 4),
                if (single)
                  ..._controls(entity)
                else
                  for (final (i, part) in parts.indexed) ...[
                    if (i > 0) const Divider(indent: 20),
                    Padding(
                      padding: const EdgeInsets.fromLTRB(20, 8, 8, 0),
                      child: Row(children: [
                        StateSwatch(part),
                        const SizedBox(width: 14),
                        Expanded(
                          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                            Text(part.name, style: theme.textTheme.titleMedium),
                            FlipText(
                              _status(context, part),
                              style: theme.textTheme.bodyMedium?.copyWith(color: stateColor(part), fontFeatures: Hm.tabular),
                            ),
                          ]),
                        ),
                        IconButton(
                          icon: const Icon(Icons.edit_outlined, size: 20),
                          tooltip: l.editDevice,
                          onPressed: () => editEntity(context, core, part),
                        ),
                      ]),
                    ),
                    ..._controls(part),
                  ],
                if (entity.added != null) ...[
                  const SizedBox(height: 12),
                  const Divider(),
                  Padding(
                    padding: const EdgeInsets.symmetric(horizontal: 12),
                    child: Align(
                      alignment: Alignment.centerLeft,
                      child: TextButton.icon(
                        style: TextButton.styleFrom(foregroundColor: Hm.danger),
                        icon: const Icon(Icons.delete_outline),
                        label: Text(l.removeDevice),
                        onPressed: () => _remove(context, entity),
                      ),
                    ),
                  ),
                ],
              ]),
            ),
          );
        },
      );

  /// Removes the device the user added, all its parts with it, once confirmed; the sheet closes.
  Future<void> _remove(BuildContext context, Entity entity) async {
    final l = AppLocalizations.of(context);
    final sure = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(l.removeDevice),
        content: Text(l.removeDeviceBody),
        actions: [
          TextButton(onPressed: () => Navigator.pop(context, false), child: Text(l.cancel)),
          FilledButton(onPressed: () => Navigator.pop(context, true), child: Text(l.removeItem)),
        ],
      ),
    );
    if (sure != true || !context.mounted) return;
    try {
      await core.removeDevice(entity.plugin, entity.added!);
      if (context.mounted) Navigator.pop(context);
    } catch (e) {
      if (context.mounted) showError(context, e);
    }
  }

  String _status(BuildContext context, Entity entity) {
    final l = AppLocalizations.of(context);
    final reason = failureReason(entity);
    return !entity.available
        ? l.offline
        : isFailed(entity)
            ? (reason == null ? l.failedNoReason : l.failed(reason))
            : isPending(entity)
                ? l.pending
                : summary(context, entity);
  }

  List<Widget> _controls(Entity entity) => [
        for (final MapEntry(key: kind, value: slot) in entity.orderedCaps)
          CapabilityTile(kind: kind, slot: slot, enabled: entity.available, onSet: (v) => onSet(entity.id, kind, v)),
        if (entity.actions.any((a) => a.params == null)) _ActionRow(core: core, entity: entity),
      ];
}

/// Renames [entity] and moves its whole device: a strip's outlets share one room and one model.
/// [whole] edits the device itself, without a name.
Future<void> editEntity(BuildContext context, CoreClient core, Entity entity, {bool whole = false}) async {
  final l = AppLocalizations.of(context);
  final parts = deviceParts(core, entity);
  final name = TextEditingController(text: entity.name);
  var room = entity.room ?? '';
  final chosen = parts.map((e) => e.model).nonNulls.firstOrNull;
  var model = chosen;
  final save = await showDialog<bool>(
    context: context,
    builder: (context) => AlertDialog(
      title: Text(l.editDevice),
      content: SizedBox(
        width: double.maxFinite,
        child: SingleChildScrollView(child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        if (!whole) ...[
          TextField(controller: name, decoration: InputDecoration(labelText: l.name)),
          const SizedBox(height: 16),
        ],
        Autocomplete<String>(
          initialValue: TextEditingValue(text: room),
          optionsBuilder: (value) => core.rooms.where((r) => r.toLowerCase().contains(value.text.toLowerCase())),
          onSelected: (value) => room = value,
          fieldViewBuilder: (context, controller, focus, _) => TextField(
            controller: controller,
            focusNode: focus,
            onChanged: (value) => room = value,
            decoration: InputDecoration(
              labelText: l.room,
              hintText: l.roomHint,
              helperText: parts.length > 1 ? l.roomWholeDevice(parts.length) : null,
            ),
          ),
        ),
        const SizedBox(height: 20),
        Text(l.modelSection, style: Theme.of(context).textTheme.titleSmall?.copyWith(color: Hm.textDim)),
        const SizedBox(height: 8),
        StatefulBuilder(
          builder: (context, setChoice) => Gallery(
            selected: model,
            onSelect: (id) => setChoice(() => model = id),
            items: [for (final id in choosableModels) (id, 'assets/models/$id/base_22.webp', modelLabel(context, id))],
          ),
        ),
      ]))),
      actions: [
        TextButton(onPressed: () => Navigator.pop(context, false), child: Text(l.cancel)),
        FilledButton(onPressed: () => Navigator.pop(context, true), child: Text(l.save)),
      ],
    ),
  );
  final newName = name.text.trim();
  name.dispose();
  if (save != true || !context.mounted) return;
  final newRoom = room.trim();
  final newModel = model == chosen ? null : (model ?? '');
  try {
    // One after the other: each one rewrites homemaid.toml.
    for (final e in parts) {
      final rename = !whole && e.id == entity.id;
      final move = (e.room ?? '') != newRoom;
      if (!rename && !move && newModel == null) continue;
      await core.update(e.id, name: rename ? newName : null, room: move ? newRoom : null, model: newModel);
    }
  } catch (e) {
    if (context.mounted) showError(context, e);
  }
}

/// Which furnished room a room is shown as; "Automatic" goes back to the one its name suggests.
Future<void> editRoom(BuildContext context, CoreClient core, String room) async {
  final l = AppLocalizations.of(context);
  var kind = core.roomKinds[room];
  final save = await showDialog<bool>(
    context: context,
    builder: (context) => AlertDialog(
      title: Text(room),
      content: SizedBox(
        width: double.maxFinite,
        child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          Text(l.roomType, style: Theme.of(context).textTheme.titleSmall?.copyWith(color: Hm.textDim)),
          const SizedBox(height: 8),
          StatefulBuilder(
            builder: (context, setChoice) => Gallery(
              selected: kind,
              autoLabel: l.autoAs(roomKindLabel(context, inferRoomKind(room))),
              onSelect: (id) => setChoice(() => kind = id),
              items: [for (final k in roomKinds) (k, 'assets/rooms/${k}_s/view_3.webp', roomKindLabel(context, k))],
            ),
          ),
        ]),
      ),
      actions: [
        TextButton(onPressed: () => Navigator.pop(context, false), child: Text(l.cancel)),
        FilledButton(onPressed: () => Navigator.pop(context, true), child: Text(l.save)),
      ],
    ),
  );
  if (save != true || !context.mounted || kind == core.roomKinds[room]) return;
  await core.updateRoom(room, kind ?? '').catchError((Object e) {
    if (context.mounted) showError(context, e);
  });
}

/// Choices as thumbnails of their renders; "Automatic" (null) lets the app decide.
class Gallery extends StatelessWidget {
  const Gallery({super.key, required this.selected, required this.onSelect, required this.items, this.autoLabel});

  final String? selected;
  final ValueChanged<String?> onSelect;

  /// The "Automatic" tile's label, when it can say what it resolves to.
  final String? autoLabel;

  /// Id, thumbnail asset, label.
  final List<(String, String, String)> items;

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final theme = Theme.of(context);
    Widget tile(String? id, Widget picture, String label) {
      final chosen = id == selected;
      return Semantics(
        selected: chosen,
        button: true,
        label: label,
        child: InkWell(
          onTap: () => onSelect(id),
          borderRadius: BorderRadius.circular(Hm.radius),
          child: SizedBox(
            width: 84,
            child: Column(children: [
              Container(
                width: 76,
                height: 76,
                decoration: BoxDecoration(
                  color: Hm.ground,
                  borderRadius: BorderRadius.circular(Hm.radius),
                  border: Border.all(color: chosen ? Hm.select : Hm.line, width: chosen ? 2 : 1),
                ),
                child: picture,
              ),
              const SizedBox(height: 4),
              ExcludeSemantics(
                child: Text(
                  label,
                  maxLines: 2,
                  textAlign: TextAlign.center,
                  overflow: TextOverflow.ellipsis,
                  style: theme.textTheme.labelSmall?.copyWith(color: chosen ? Hm.text : Hm.textDim),
                ),
              ),
            ]),
          ),
        ),
      );
    }

    // Opens on the current choice, one tile of context before it.
    final at = items.indexWhere((i) => i.$1 == selected) + 1;
    return SizedBox(
      height: 124,
      child: ListView(controller: ScrollController(initialScrollOffset: max(0, at - 1) * 84.0), scrollDirection: Axis.horizontal, children: [
        tile(null, const Icon(Icons.auto_awesome_outlined, color: Hm.textDim), autoLabel ?? l.modelAuto),
        for (final (id, image, label) in items)
          tile(
            id,
            Image.asset(
              image,
              fit: BoxFit.contain,
              errorBuilder: (_, _, _) => const Icon(Icons.view_in_ar_outlined, color: Hm.textDim),
            ),
            label,
          ),
      ]),
    );
  }
}

/// An entity's one-shot actions (those without parameters) as keys. They are not states: no
/// switch, nothing pending; shutting down asks first.
class _ActionRow extends StatelessWidget {
  const _ActionRow({required this.core, required this.entity});

  final CoreClient core;
  final Entity entity;

  static const _icons = {
    'play_pause': Icons.play_arrow,
    'next': Icons.skip_next,
    'previous': Icons.skip_previous,
    'lock': Icons.lock_outline,
    'sleep': Icons.bedtime_outlined,
    'shutdown': Icons.power_settings_new,
  };

  Future<void> _invoke(BuildContext context, String action) async {
    final l = AppLocalizations.of(context);
    if (action == 'shutdown') {
      final sure = await showDialog<bool>(
        context: context,
        builder: (context) => AlertDialog(
          icon: const Icon(Icons.power_settings_new, color: Hm.danger),
          title: Text(l.shutdownConfirm(entity.name)),
          content: Text(l.shutdownBody),
          actions: [
            TextButton(onPressed: () => Navigator.pop(context, false), child: Text(l.cancel)),
            FilledButton(
              style: FilledButton.styleFrom(backgroundColor: Hm.danger, foregroundColor: Hm.ground),
              onPressed: () => Navigator.pop(context, true),
              child: Text(l.powerShutdown),
            ),
          ],
        ),
      );
      if (sure != true || !context.mounted) return;
    }
    await core.invoke(entity.id, action).catchError((Object e) {
      if (context.mounted) showError(context, e);
    });
  }

  @override
  Widget build(BuildContext context) => Padding(
        padding: const EdgeInsets.fromLTRB(20, 12, 20, 4),
        child: Wrap(spacing: 8, runSpacing: 8, children: [
          for (final a in entity.actions.where((a) => a.params == null))
            OutlinedButton.icon(
              style: a.name == 'shutdown' ? OutlinedButton.styleFrom(foregroundColor: Hm.danger, side: const BorderSide(color: Hm.danger)) : null,
              onPressed: entity.available ? () => _invoke(context, a.name) : null,
              icon: Icon(_icons[a.name] ?? Icons.bolt_outlined, size: 18),
              label: Text(actionLabel(context, a.name)),
            ),
        ]),
      );
}
