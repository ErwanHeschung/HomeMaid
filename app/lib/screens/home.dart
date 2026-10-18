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
    if (core == null) return EmptyState(title: l.pairHomeTitle, hint: l.pairHomeHint, onPair: widget.onPair);
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
              final rows = entities.length * 65.0 + 8;
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
                          itemCount: entities.length,
                          separatorBuilder: (_, _) => const Divider(indent: 50),
                          itemBuilder: (context, i) => DeviceRow(
                            entity: entities[i],
                            selected: entities[i].id == _selected,
                            onTap: () => _open(context, core, entities[i].id),
                            onSet: (cap, value) => _set(context, core, entities[i].id, cap, value),
                          ),
                        ),
                ),
              ]);
            }),
          ),
        ]);
      },
    );
  }

  Future<void> _open(BuildContext context, CoreClient core, String id) async {
    setState(() => _selected = id);
    await showModalBottomSheet<void>(
      context: context,
      isScrollControlled: true,
      useSafeArea: true,
      builder: (_) => DeviceSheet(core: core, id: id, onSet: (cap, value) => _set(context, core, id, cap, value)),
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
                      bottom: BorderSide(color: room == current ? Hm.select : Colors.transparent, width: 2),
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

/// A device in the list: its fill swatch, name, flipping value and primary switch.
class DeviceRow extends StatelessWidget {
  const DeviceRow({super.key, required this.entity, required this.selected, required this.onTap, required this.onSet});

  final Entity entity;
  final bool selected;
  final VoidCallback onTap;
  final void Function(String cap, Object? value) onSet;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final power = entity.caps['on_off'];
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
                  Text(entity.name, style: theme.textTheme.titleMedium, maxLines: 1, overflow: TextOverflow.ellipsis),
                  const SizedBox(height: 2),
                  FlipText(
                    summary(context, entity),
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

/// Every control of one device, live.
class DeviceSheet extends StatelessWidget {
  const DeviceSheet({super.key, required this.core, required this.id, required this.onSet});

  final CoreClient core;
  final String id;
  final void Function(String cap, Object? value) onSet;

  @override
  Widget build(BuildContext context) => ListenableBuilder(
        listenable: core,
        builder: (context, _) {
          final entity = core.entities[id];
          if (entity == null) return const SizedBox(height: 120);
          final l = AppLocalizations.of(context);
          final theme = Theme.of(context);
          final reason = failureReason(entity);
          final status = !entity.available
              ? l.offline
              : isFailed(entity)
                  ? (reason == null ? l.failedNoReason : l.failed(reason))
                  : isPending(entity)
                      ? l.pending
                      : summary(context, entity);
          return SafeArea(
            top: false,
            child: SingleChildScrollView(
              padding: const EdgeInsets.only(bottom: 16),
              child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
                Padding(
                  padding: const EdgeInsets.fromLTRB(20, 0, 8, 4),
                  child: Row(children: [
                    StateSwatch(entity, size: 18),
                    const SizedBox(width: 14),
                    Expanded(child: Text(entity.name, style: theme.textTheme.headlineSmall)),
                    IconButton(
                      icon: const Icon(Icons.edit_outlined),
                      tooltip: l.editDevice,
                      onPressed: () => editEntity(context, core, entity),
                    ),
                  ]),
                ),
                Padding(
                  padding: const EdgeInsets.fromLTRB(52, 0, 20, 12),
                  child: FlipText(
                    status,
                    style: theme.textTheme.bodyMedium?.copyWith(color: stateColor(entity), fontFeatures: Hm.tabular),
                  ),
                ),
                const Divider(),
                const SizedBox(height: 4),
                for (final MapEntry(key: kind, value: slot) in entity.orderedCaps)
                  CapabilityTile(kind: kind, slot: slot, enabled: entity.available, onSet: (v) => onSet(kind, v)),
                if (entity.actions.any((a) => a.params == null))
                  _ActionRow(core: core, entity: entity),
              ]),
            ),
          );
        },
      );
}

Future<void> editEntity(BuildContext context, CoreClient core, Entity entity) async {
  final l = AppLocalizations.of(context);
  final name = TextEditingController(text: entity.name);
  var room = entity.room ?? '';
  var model = entity.model;
  final save = await showDialog<bool>(
    context: context,
    builder: (context) => AlertDialog(
      title: Text(l.editDevice),
      content: SizedBox(
        width: double.maxFinite,
        child: SingleChildScrollView(child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        TextField(controller: name, decoration: InputDecoration(labelText: l.name)),
        const SizedBox(height: 16),
        Autocomplete<String>(
          initialValue: TextEditingValue(text: room),
          optionsBuilder: (value) => core.rooms.where((r) => r.toLowerCase().contains(value.text.toLowerCase())),
          onSelected: (value) => room = value,
          fieldViewBuilder: (context, controller, focus, _) => TextField(
            controller: controller,
            focusNode: focus,
            onChanged: (value) => room = value,
            decoration: InputDecoration(labelText: l.room, hintText: l.roomHint),
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
  final newModel = model == entity.model ? null : (model ?? '');
  await core.update(entity.id, name: newName, room: room.trim(), model: newModel).catchError((Object e) {
    if (context.mounted) showError(context, e);
  });
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
