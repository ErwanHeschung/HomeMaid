/// The scene editor: a name, where it shows, the devices it sets and the state each takes, then
/// actions run after them (open Netflix on the PC). "Take the current state" fills the states from
/// the devices as they are now. Trying it runs the draft without saving; saving goes through the
/// core, which checks it as at startup.
library;

import 'package:flutter/material.dart';

import '../clients.dart';
import '../l10n/app_localizations.dart';
import '../model.dart';
import '../theme.dart';
import '../widgets/state.dart';
import '../widgets/value_field.dart';
import '../selectors.dart';
import 'automation_editor.dart' show InvokeFields, newInvoke, slug;
import 'automations.dart' show actionText;
import 'common.dart';

/// Opens the editor on `scene`, or on a new one shown in `room`. True when it was deleted.
Future<bool> editScene(BuildContext context, CoreClient core, {Scene? scene, String? room}) async =>
    await Navigator.of(context).push<bool>(MaterialPageRoute(
      fullscreenDialog: true,
      builder: (_) => SceneEditor(core: core, scene: scene, room: room),
    )) ??
    false;

final _byId = RegExp(r'^id:(\S+)$');

/// The capabilities a scene can set on a device, the switch first.
List<String> _settable(Entity e) =>
    [for (final MapEntry(:key, :value) in e.caps.entries) if (!value.readOnly) key]..sort((a, b) => a == 'on_off' ? -1 : (b == 'on_off' ? 1 : a.compareTo(b)));

class SceneEditor extends StatefulWidget {
  const SceneEditor({super.key, required this.core, this.scene, this.room});

  final CoreClient core;

  /// Null for a new one.
  final Scene? scene;

  /// Where a new one shows.
  final String? room;

  @override
  State<SceneEditor> createState() => _SceneEditorState();
}

class _SceneEditorState extends State<SceneEditor> {
  late final _name = TextEditingController(text: widget.scene?.name ?? '');
  late String? _room = widget.scene == null ? widget.room : widget.scene!.room;

  /// What it sets, by device then capability: the sets the editor writes (`id:` selectors).
  final _values = <String, Map<String, Object?>>{};

  /// Sets written by hand with another selector: kept as they are, and removable.
  final _others = <Map<String, dynamic>>[];

  /// Its one-shot actions, run after the states.
  final _actions = <Map<String, dynamic>>[];
  String? _error;
  bool _busy = false;

  @override
  void initState() {
    super.initState();
    for (final a in (widget.scene?.config['invoke'] as List?) ?? const []) {
      _actions.add({...(a as Map).cast<String, dynamic>()});
    }
    for (final set in (widget.scene?.config['set'] as List?) ?? const []) {
      final s = (set as Map).cast<String, dynamic>();
      if (_byId.firstMatch(s['select'] as String? ?? '') case final m?) {
        (_values[m[1]!] ??= {})[s['cap'] as String] = s['value'];
      } else {
        _others.add(s);
      }
    }
  }

  @override
  void dispose() {
    _name.dispose();
    super.dispose();
  }

  /// The devices of its room (all of them for the whole home) it can set, and those it sets anyway.
  List<Entity> get _devices {
    final all = widget.core.entities.values;
    return [
      for (final e in all)
        if ((_room == null || e.room == _room || _values.containsKey(e.id)) && _settable(e).isNotEmpty) e,
    ]..sort((a, b) => a.name.compareTo(b.name));
  }

  /// A device in, at the state it is in now.
  void _include(Entity e) => _values[e.id] = {
        for (final cap in _settable(e)) cap: inner(e.caps[cap]!.observed) ?? defaultCapValue(cap, e.caps[cap]!.meta),
      };

  List<Map<String, dynamic>> get _sets => [
        for (final MapEntry(key: id, value: caps) in _values.entries)
          for (final MapEntry(key: cap, value: value) in caps.entries)
            // Off is all there is to say: a level would light some bulbs again.
            if (caps['on_off'] != false || cap == 'on_off') {'select': 'id:$id', 'cap': cap, 'value': value},
        ..._others,
      ];

  Map<String, dynamic> get _config {
    final name = _name.text.trim();
    return {
      'id': widget.scene?.id ?? _newId(name),
      if (name.isNotEmpty) 'name': name,
      'room': ?_room,
      'set': _sets,
      if (_actions.isNotEmpty) 'invoke': _actions,
    };
  }

  /// From the name, unique; it never changes afterwards, so the automations naming it still do.
  String _newId(String name) {
    final base = slug(name).isEmpty ? 'scene' : slug(name);
    final taken = widget.core.scenes.map((s) => s.id).toSet();
    var id = base;
    for (var i = 2; taken.contains(id); i++) {
      id = '$base-$i';
    }
    return id;
  }

  void _changed() => setState(() => _error = null);

  Future<void> _busyWith(Future<void> Function() work) async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await work();
    } catch (e) {
      if (mounted) setState(() => _error = describeError(context, e));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  /// Runs the draft now, without saving it: its states, then its actions.
  Future<void> _try() => _busyWith(() async {
        final core = widget.core;
        await Future.wait([
          for (final set in _sets)
            if (_byId.firstMatch(set['select'] as String) case final m?) core.setDesired(m[1]!, set['cap'] as String, set['value']),
        ]);
        for (final a in _actions) {
          for (final e in matching(core.entities.values, a['select'] as String) ?? const <Entity>[]) {
            await core.invoke(e.id, a['action'] as String, (a['params'] as Map?)?.cast<String, Object?>());
          }
        }
      });

  Future<void> _save() => _busyWith(() async {
        await widget.core.saveScene(_config);
        if (mounted) Navigator.pop(context, false);
      });

  Future<void> _delete() async {
    final l = AppLocalizations.of(context);
    final sure = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(l.deleteAutomationConfirm(widget.scene!.name)),
        content: Text(l.deleteSceneBody),
        actions: [
          TextButton(onPressed: () => Navigator.pop(context, false), child: Text(l.cancel)),
          FilledButton(
            style: FilledButton.styleFrom(backgroundColor: Hm.danger, foregroundColor: Hm.text),
            onPressed: () => Navigator.pop(context, true),
            child: Text(l.deleteAutomation),
          ),
        ],
      ),
    );
    if (sure != true) return;
    await _busyWith(() async {
      await widget.core.deleteScene(widget.scene!.id);
      if (mounted) Navigator.pop(context, true);
    });
  }

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final theme = Theme.of(context);
    final core = widget.core;
    final devices = _devices;
    return Scaffold(
      appBar: AppBar(
        title: Text(widget.scene == null ? l.newScene : l.editScene),
        actions: [
          if (widget.scene != null)
            IconButton(icon: const Icon(Icons.delete_outline), tooltip: l.deleteAutomation, onPressed: _busy ? null : _delete),
        ],
      ),
      body: ListView(padding: const EdgeInsets.only(bottom: 24), children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(20, 8, 20, 0),
          child: TextField(
            controller: _name,
            decoration: InputDecoration(labelText: l.name),
            textCapitalization: TextCapitalization.sentences,
            onChanged: (_) => _changed(),
          ),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(20, 12, 20, 0),
          child: DropdownButtonFormField<String?>(
            isExpanded: true,
            initialValue: _room,
            decoration: InputDecoration(labelText: l.sceneRoom),
            items: [
              DropdownMenuItem(value: null, child: Text(l.wholeHome)),
              for (final r in {...core.rooms, ?_room}) DropdownMenuItem(value: r, child: Text(r)),
            ],
            onChanged: (room) {
              _room = room;
              _changed();
            },
          ),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(20, 16, 8, 4),
          child: Row(children: [
            Expanded(child: Text(l.sceneDevicesHint, style: theme.textTheme.bodyMedium?.copyWith(color: Hm.textDim))),
            TextButton.icon(
              icon: const Icon(Icons.photo_camera_outlined),
              label: Text(l.captureState),
              onPressed: devices.isEmpty
                  ? null
                  : () {
                      devices.forEach(_include);
                      _changed();
                    },
            ),
          ]),
        ),
        if (_values.isEmpty && _others.isEmpty)
          Padding(
            padding: const EdgeInsets.fromLTRB(20, 4, 20, 8),
            child: Text(l.sceneEmpty, style: theme.textTheme.bodyMedium?.copyWith(color: Hm.textDim)),
          ),
        for (final e in devices) ...[
          const Divider(indent: 20),
          _SceneDevice(
            entity: e,
            values: _values[e.id],
            onInclude: (on) {
              on ? _include(e) : _values.remove(e.id);
              _changed();
            },
            onValue: (cap, value) {
              _values[e.id]![cap] = value;
              _changed();
            },
          ),
        ],
        SectionHeader(l.sceneActions),
        Padding(
          padding: const EdgeInsets.fromLTRB(20, 0, 20, 4),
          child: Text(l.sceneActionsHint, style: theme.textTheme.bodyMedium?.copyWith(color: Hm.textDim)),
        ),
        for (final a in _actions)
          Container(
            key: ObjectKey(a),
            margin: const EdgeInsets.fromLTRB(20, 4, 20, 8),
            padding: const EdgeInsets.fromLTRB(12, 12, 4, 12),
            decoration: BoxDecoration(border: Border.all(color: Hm.line), borderRadius: BorderRadius.circular(Hm.radius)),
            // Its close key beside its first field, not on a line of its own.
            child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Expanded(child: InvokeFields(core: core, invoke: a, onChanged: _changed)),
              IconButton(
                icon: const Icon(Icons.close),
                tooltip: l.removeItem,
                onPressed: () {
                  _actions.remove(a);
                  _changed();
                },
              ),
            ]),
          ),
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 8),
          child: Align(
            alignment: Alignment.centerLeft,
            child: TextButton.icon(
              icon: const Icon(Icons.add),
              label: Text(l.addSceneAction),
              onPressed: () {
                _actions.add(newInvoke(core));
                _changed();
              },
            ),
          ),
        ),
        if (_others.isNotEmpty) ...[
          SectionHeader(l.sceneAlso),
          for (final set in [..._others])
            ListTile(
              contentPadding: const EdgeInsets.only(left: 20, right: 8),
              title: Text(actionText(context, core, {'set': set})),
              trailing: IconButton(
                icon: const Icon(Icons.close),
                tooltip: l.removeItem,
                onPressed: () {
                  _others.remove(set);
                  _changed();
                },
              ),
            ),
        ],
      ]),
      bottomNavigationBar: SafeArea(
        child: Container(
          decoration: const BoxDecoration(border: Border(top: BorderSide(color: Hm.line))),
          padding: const EdgeInsets.fromLTRB(20, 10, 20, 10),
          child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
            if (_error != null)
              Padding(
                padding: const EdgeInsets.only(bottom: 10),
                child: Text(_error!, style: theme.textTheme.bodyMedium?.copyWith(color: Hm.danger)),
              ),
            Row(children: [
              Expanded(child: OutlinedButton(onPressed: _busy || (_sets.isEmpty && _actions.isEmpty) ? null : _try, child: Text(l.tryIt))),
              const SizedBox(width: 12),
              Expanded(child: FilledButton(onPressed: _busy ? null : _save, child: Text(l.save))),
            ]),
          ]),
        ),
      ),
    );
  }
}

/// A device of the scene: in or out, and the state it takes.
class _SceneDevice extends StatelessWidget {
  const _SceneDevice({required this.entity, required this.values, required this.onInclude, required this.onValue});

  final Entity entity;

  /// Null: not in the scene.
  final Map<String, Object?>? values;
  final ValueChanged<bool> onInclude;
  final void Function(String cap, Object value) onValue;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final values = this.values;
    final off = values?['on_off'] == false;
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      Padding(
        padding: const EdgeInsets.fromLTRB(20, 4, 12, 4),
        child: Row(children: [
          StateSwatch(entity),
          const SizedBox(width: 16),
          Expanded(child: Text(entity.name, style: theme.textTheme.titleMedium, maxLines: 1, overflow: TextOverflow.ellipsis)),
          Semantics(label: entity.name, child: Switch(value: values != null, onChanged: onInclude)),
        ]),
      ),
      if (values != null)
        for (final MapEntry(key: cap, value: value) in values.entries)
          // Once off, its other settings are not sent: nothing to show.
          if (!off || cap == 'on_off')
            Padding(
              padding: const EdgeInsets.fromLTRB(50, 4, 20, 8),
              child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                Text(capLabel(context, cap), style: theme.textTheme.titleSmall?.copyWith(color: Hm.textDim)),
                const SizedBox(height: 6),
                ValueField(
                  key: ValueKey('${entity.id} $cap'),
                  cap: cap,
                  meta: entity.caps[cap]?.meta ?? const {},
                  value: value,
                  onChanged: (v) => onValue(cap, v),
                ),
              ]),
            ),
    ]);
  }
}
