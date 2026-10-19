/// The automation editor: the sheet's sentences as cards to change. Saved through the core, which
/// checks it as at startup, writes it to homemaid.toml and puts it in force at once.
library;

import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:intl/intl.dart';

import '../clients.dart';
import '../l10n/app_localizations.dart';
import '../model.dart';
import '../selectors.dart';
import '../theme.dart';
import '../widgets/state.dart';
import 'automations.dart';
import 'common.dart';
import 'settings.dart';

/// Opens the editor on `automation`, or on a new one. True when it was deleted.
Future<bool> editAutomation(BuildContext context, CoreClient core, [Automation? automation]) async =>
    await Navigator.of(context).push<bool>(MaterialPageRoute(
      fullscreenDialog: true,
      builder: (_) => AutomationEditor(core: core, automation: automation),
    )) ??
    false;

/// `Le café du matin !` -> `le-cafe-du-matin`: the id of a new automation.
String slug(String text) {
  const plain = {
    'à': 'a', 'â': 'a', 'ä': 'a', 'á': 'a', 'ç': 'c', 'é': 'e', 'è': 'e', 'ê': 'e', 'ë': 'e', 'î': 'i', 'ï': 'i',
    'í': 'i', 'ô': 'o', 'ö': 'o', 'ó': 'o', 'ù': 'u', 'û': 'u', 'ü': 'u', 'ú': 'u', 'ÿ': 'y', 'ñ': 'n', 'œ': 'oe', 'æ': 'ae',
  };
  final ascii = text.toLowerCase().split('').map((c) => plain[c] ?? c).join();
  return ascii.replaceAll(RegExp('[^a-z0-9]+'), '-').replaceAll(RegExp(r'^-+|-+$'), '');
}

final _sun = RegExp(r'^(sunrise|sunset)([+-]\d+)?$');

Map<String, dynamic> _copy(Map<String, dynamic> m) => (jsonDecode(jsonEncode(m)) as Map).cast<String, dynamic>();

Map<String, dynamic> _sub(Map<String, dynamic> m, String key) => (m[key] as Map).cast<String, dynamic>();

bool _writable(Entity e, String cap) => e.caps[cap] != null && e.caps[cap]!.meta['read_only'] != true;

/// The metadata of `cap` on the first device that has it.
Map<String, dynamic> _meta(CoreClient core, String cap) =>
    core.entities.values.map((e) => e.caps[cap]?.meta).nonNulls.firstOrNull ?? const {};

bool _isBool(String cap, Map<String, dynamic> meta) =>
    cap == 'on_off' || cap == 'mute' || (meta['schema'] as Map?)?['type'] == 'boolean';

bool _isNumber(String cap, Map<String, dynamic> meta) =>
    const {'brightness', 'volume', 'power'}.contains(cap) || meta['max'] is num || (meta['schema'] as Map?)?['type'] == 'number';

Object _defaultValue(String cap, Map<String, dynamic> meta) =>
    _isBool(cap, meta) ? true : _isNumber(cap, meta) ? ((meta['max'] as num? ?? 100) / 2).round() : '';

Map<String, dynamic> _newTrigger() => {'at': '19:00'};

/// Something the home has: a switch if there is one, else a level, else any setting.
Map<String, dynamic> _newAction(CoreClient core) {
  final writable = [for (final e in core.entities.values) for (final c in e.caps.keys) if (_writable(e, c)) (e, c)];
  final pick = ['on_off', 'brightness', 'volume'].map((cap) => writable.where((w) => w.$2 == cap).firstOrNull).nonNulls.firstOrNull ??
      writable.firstOrNull;
  final cap = pick?.$2 ?? 'on_off';
  return {'set': {'select': roomSelector(pick?.$1.room, cap), 'cap': cap, 'value': _defaultValue(cap, _meta(core, cap))}};
}

class AutomationEditor extends StatefulWidget {
  const AutomationEditor({super.key, required this.core, this.automation});

  final CoreClient core;

  /// Null for a new one.
  final Automation? automation;

  @override
  State<AutomationEditor> createState() => _AutomationEditorState();
}

class _AutomationEditorState extends State<AutomationEditor> {
  late final _name = TextEditingController(text: widget.automation?.name ?? '');
  late bool _enabled = widget.automation?.config['enabled'] as bool? ?? true;
  late final _triggers = [for (final t in widget.automation?.triggers ?? [_newTrigger()]) _copy(t)];
  late final _conditions = [for (final c in widget.automation?.conditions ?? const <Map<String, dynamic>>[]) _copy(c)];
  late final _actions = [for (final a in widget.automation?.actions ?? [_newAction(widget.core)]) _copy(a)];
  String? _error;
  DryRun? _dry;
  bool _busy = false;

  @override
  void dispose() {
    _name.dispose();
    super.dispose();
  }

  Map<String, dynamic> get _config {
    final name = _name.text.trim();
    return {
      'id': widget.automation?.id ?? _newId(name),
      if (name.isNotEmpty) 'name': name,
      if (!_enabled) 'enabled': false,
      'trigger': _triggers,
      if (_conditions.isNotEmpty) 'condition': _conditions,
      'action': _actions,
    };
  }

  /// From the name, unique; it never changes afterwards, so the runs stay attached.
  String _newId(String name) {
    final base = slug(name).isEmpty ? 'automation' : slug(name);
    final taken = widget.core.automations.map((a) => a.id).toSet();
    var id = base;
    for (var i = 2; taken.contains(id); i++) {
      id = '$base-$i';
    }
    return id;
  }

  void _changed() => setState(() {
        _dry = null;
        _error = null;
      });

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

  Future<void> _try() => _busyWith(() async {
        final dry = await widget.core.dryRunConfig(_config);
        if (mounted) setState(() => _dry = dry);
      });

  Future<void> _save() => _busyWith(() async {
        await widget.core.saveAutomation(_config);
        if (mounted) Navigator.pop(context, false);
      });

  Future<void> _delete() async {
    final l = AppLocalizations.of(context);
    final sure = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(l.deleteAutomationConfirm(widget.automation!.name)),
        content: Text(l.deleteAutomationBody),
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
      await widget.core.deleteAutomation(widget.automation!.id);
      if (mounted) Navigator.pop(context, true);
    });
  }

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final theme = Theme.of(context);
    final core = widget.core;
    VoidCallback? removing(List<Map<String, dynamic>> list, Map<String, dynamic> item, {bool keepOne = false}) =>
        keepOne && list.length == 1 ? null : () => setState(() => list.remove(item));
    return Scaffold(
      appBar: AppBar(
        title: Text(widget.automation == null ? l.newAutomation : l.editAutomation),
        actions: [
          if (widget.automation != null)
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
        SwitchListTile(
          contentPadding: const EdgeInsets.symmetric(horizontal: 20),
          title: Text(l.automationEnabled),
          value: _enabled,
          onChanged: (v) {
            _enabled = v;
            _changed();
          },
        ),
        SectionHeader(l.sectionWhen),
        for (final t in _triggers)
          _TriggerCard(key: ObjectKey(t), core: core, trigger: t, onChanged: _changed, onRemove: removing(_triggers, t, keepOne: true)),
        _AddButton(l.addTrigger, () => setState(() => _triggers.add(_newTrigger()))),
        SectionHeader(l.sectionIf),
        for (final c in _conditions)
          _ConditionCard(key: ObjectKey(c), core: core, condition: c, onChanged: _changed, onRemove: removing(_conditions, c)),
        _AddButton(l.addCondition, () => setState(() => _conditions.add({'between': ['18:00', '23:00']}))),
        SectionHeader(l.sectionThen),
        for (final a in _actions)
          _ActionCard(key: ObjectKey(a), core: core, action: a, onChanged: _changed, onRemove: removing(_actions, a, keepOne: true)),
        _AddButton(l.addAction, () => setState(() => _actions.add(_newAction(core)))),
        if (_dry != null) DryRunView(dry: _dry!, core: core),
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
              Expanded(child: OutlinedButton(onPressed: _busy ? null : _try, child: Text(l.tryIt))),
              const SizedBox(width: 12),
              Expanded(child: FilledButton(onPressed: _busy ? null : _save, child: Text(l.save))),
            ]),
          ]),
        ),
      ),
    );
  }
}

class _AddButton extends StatelessWidget {
  const _AddButton(this.label, this.onPressed);

  final String label;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) => Padding(
        padding: const EdgeInsets.symmetric(horizontal: 12),
        child: Align(
          alignment: Alignment.centerLeft,
          child: TextButton.icon(onPressed: onPressed, icon: const Icon(Icons.add), label: Text(label)),
        ),
      );
}

/// A trigger, condition or action: its kind, its fields, and a way out.
class _Card extends StatelessWidget {
  const _Card({required this.kinds, required this.kind, required this.onKind, required this.onRemove, required this.children});

  final List<(String, String)> kinds;
  final String kind;
  final ValueChanged<String> onKind;
  final VoidCallback? onRemove;
  final List<Widget> children;

  @override
  Widget build(BuildContext context) => Container(
        margin: const EdgeInsets.fromLTRB(20, 4, 20, 8),
        padding: const EdgeInsets.fromLTRB(12, 12, 4, 12),
        decoration: BoxDecoration(border: Border.all(color: Hm.line), borderRadius: BorderRadius.circular(Hm.radius)),
        child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          Row(children: [
            Expanded(
              child: SegmentedButton<String>(
                showSelectedIcon: false,
                segments: [for (final (id, label) in kinds) ButtonSegment(value: id, label: Text(label))],
                selected: {kind},
                onSelectionChanged: (s) => onKind(s.first),
              ),
            ),
            if (onRemove != null)
              IconButton(icon: const Icon(Icons.close), tooltip: AppLocalizations.of(context).removeItem, onPressed: onRemove)
            else
              const SizedBox(width: 8),
          ]),
          for (final child in children) Padding(padding: const EdgeInsets.only(top: 12, right: 8), child: child),
        ]),
      );
}

class _TriggerCard extends StatelessWidget {
  const _TriggerCard({super.key, required this.core, required this.trigger, required this.onChanged, required this.onRemove});

  final CoreClient core;
  final Map<String, dynamic> trigger;
  final VoidCallback onChanged;
  final VoidCallback? onRemove;

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final t = trigger;
    final device = t.containsKey('when');
    return _Card(
      kinds: [('moment', l.kindMoment), ('device', l.kindDevice)],
      kind: device ? 'device' : 'moment',
      onRemove: onRemove,
      onKind: (kind) {
        t.clear();
        t.addAll(kind == 'device' ? {'when': _newTest(core, trigger: true)} : _newTrigger());
        onChanged();
      },
      children: device
          ? [_TestFields(core: core, test: _sub(t, 'when'), trigger: true, onChanged: onChanged)]
          : [
              _TimePoint(core: core, value: t['at'] as String, onChanged: (v) {
                t['at'] = v;
                onChanged();
              }),
              _DayChips(days: (t['days'] as List?)?.cast<String>(), onChanged: (days) {
                days.length == 7 ? t.remove('days') : t['days'] = days;
                onChanged();
              }),
            ],
    );
  }
}

class _ConditionCard extends StatelessWidget {
  const _ConditionCard({super.key, required this.core, required this.condition, required this.onChanged, required this.onRemove});

  final CoreClient core;
  final Map<String, dynamic> condition;
  final VoidCallback onChanged;
  final VoidCallback? onRemove;

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final c = condition;
    final kind = c.containsKey('state') ? 'state' : c.containsKey('days') ? 'days' : 'between';
    return _Card(
      kinds: [('between', l.kindTimeRange), ('days', l.kindDays), ('state', l.kindState)],
      kind: kind,
      onRemove: onRemove,
      onKind: (kind) {
        c.clear();
        c.addAll(switch (kind) {
          'days' => {'days': [...allDays.take(5)]},
          'state' => {'state': _newTest(core, trigger: false)},
          _ => {'between': ['18:00', '23:00']},
        });
        onChanged();
      },
      children: switch (kind) {
        'days' => [
            _DayChips(days: (c['days'] as List).cast<String>(), onChanged: (days) {
              c['days'] = days;
              onChanged();
            }),
          ],
        'state' => [_TestFields(core: core, test: _sub(c, 'state'), trigger: false, onChanged: onChanged)],
        _ => [
            for (final (i, label) in [l.rangeFrom, l.rangeTo].indexed)
              _Labelled(
                label,
                _TimePoint(core: core, value: (c['between'] as List)[i] as String, onChanged: (v) {
                  (c['between'] as List)[i] = v;
                  onChanged();
                }),
              ),
          ],
      },
    );
  }
}

class _ActionCard extends StatelessWidget {
  const _ActionCard({super.key, required this.core, required this.action, required this.onChanged, required this.onRemove});

  final CoreClient core;
  final Map<String, dynamic> action;
  final VoidCallback onChanged;
  final VoidCallback? onRemove;

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final a = action;
    final invoke = a.containsKey('invoke');
    return _Card(
      kinds: [('set', l.kindSet), ('invoke', l.kindInvoke)],
      kind: invoke ? 'invoke' : 'set',
      onRemove: onRemove,
      onKind: (kind) {
        a.clear();
        a.addAll(kind == 'invoke' ? {'invoke': _newInvoke(core)} : _newAction(core));
        onChanged();
      },
      children: invoke
          ? [_InvokeFields(core: core, invoke: _sub(a, 'invoke'), onChanged: onChanged)]
          : [_SetFields(core: core, set: _sub(a, 'set'), onChanged: onChanged)],
    );
  }
}

class _Labelled extends StatelessWidget {
  const _Labelled(this.label, this.child);

  final String label;
  final Widget child;

  @override
  Widget build(BuildContext context) => Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        Text(label, style: Theme.of(context).textTheme.titleSmall?.copyWith(color: Hm.textDim)),
        const SizedBox(height: 6),
        child,
      ]);
}

/// `19:30`, or sunrise/sunset shifted by minutes (`sunset-30`): the core's time points.
class _TimePoint extends StatelessWidget {
  const _TimePoint({required this.core, required this.value, required this.onChanged});

  final CoreClient core;
  final String value;
  final ValueChanged<String> onChanged;

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final theme = Theme.of(context);
    final sun = _sun.firstMatch(value);
    final mode = sun?[1] ?? 'clock';
    final offset = int.parse(sun?[2] ?? '0');
    String sunValue(String event, int offset) => offset == 0 ? event : '$event${offset > 0 ? '+' : ''}$offset';
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      SegmentedButton<String>(
        showSelectedIcon: false,
        segments: [
          ButtonSegment(value: 'clock', label: Text(l.timeClock)),
          ButtonSegment(value: 'sunrise', label: Text(l.timeSunrise)),
          ButtonSegment(value: 'sunset', label: Text(l.timeSunset)),
        ],
        selected: {mode},
        onSelectionChanged: (s) => onChanged(s.first == 'clock' ? '19:00' : s.first),
      ),
      const SizedBox(height: 8),
      if (sun == null)
        OutlinedButton.icon(
          icon: const Icon(Icons.schedule),
          label: Text(value, style: theme.textTheme.titleMedium?.copyWith(fontFeatures: Hm.tabular)),
          onPressed: () async {
            final [h, m, ...] = value.split(':').map(int.tryParse).toList();
            final picked = await showTimePicker(context: context, initialTime: TimeOfDay(hour: h ?? 19, minute: m ?? 0));
            if (picked != null) {
              onChanged('${picked.hour.toString().padLeft(2, '0')}:${picked.minute.toString().padLeft(2, '0')}');
            }
          },
        )
      else ...[
        Text(sunText(context, value, at: true)!, style: theme.textTheme.bodyLarge?.copyWith(fontFeatures: Hm.tabular)),
        Slider(
          value: offset.clamp(-360, 360).toDouble(),
          min: -360,
          max: 360,
          divisions: 144,
          semanticFormatterCallback: (v) => sunText(context, sunValue(mode, v.round()), at: true)!,
          onChanged: (v) => onChanged(sunValue(mode, v.round())),
        ),
        // Said here rather than refused at save: the hub cannot know when the sun sets.
        ListenableBuilder(
          listenable: core,
          builder: (context, _) => core.location != null
              ? const SizedBox()
              : Row(children: [
                  Expanded(child: Text(l.sunNeedsLocation, style: theme.textTheme.bodyMedium?.copyWith(color: Hm.danger))),
                  TextButton(onPressed: () => editLocation(context, core), child: Text(l.setLocation)),
                ]),
        ),
      ],
    ]);
  }
}

class _DayChips extends StatelessWidget {
  const _DayChips({required this.days, required this.onChanged});

  /// Null: every day.
  final List<String>? days;
  final ValueChanged<List<String>> onChanged;

  @override
  Widget build(BuildContext context) {
    final locale = Localizations.localeOf(context).toLanguageTag();
    final chosen = (days ?? allDays).toSet();
    return Wrap(spacing: 6, runSpacing: 6, children: [
      for (final (i, day) in allDays.indexed)
        FilterChip(
          // 2024-01-01 was a Monday.
          label: Text(DateFormat.E(locale).format(DateTime(2024, 1, 1 + i))),
          selected: chosen.contains(day),
          onSelected: (on) {
            final next = on ? {...chosen, day} : ({...chosen}..remove(day));
            // At least one day: an automation on no day is an error.
            if (next.isNotEmpty) onChanged([for (final d in allDays) if (next.contains(d)) d]);
          },
        ),
    ]);
  }
}

/// Where (a room or the whole home) and what (a capability): `room:"Salon" & cap:on_off`. A
/// selector written by hand that is not of that form stays editable as text.
class _Target extends StatelessWidget {
  const _Target({required this.core, required this.target, required this.caps, required this.onChanged});

  final CoreClient core;

  /// Holds `select` and `cap`.
  final Map<String, dynamic> target;
  final List<String> caps;

  /// With whether the capability changed (its value no longer fits).
  final void Function(bool capChanged) onChanged;

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final select = target['select'] as String;
    final cap = target['cap'] as String;
    final parsed = parseRoomSelector(select);
    final rooms = {
      ...core.rooms.where((r) => core.entities.values.any((e) => e.room == r && e.caps.containsKey(cap))),
      if (parsed?.$1 != null) parsed!.$1!,
    }.toList()
      ..sort();
    final choices = {...caps, cap}.toList()..sort((a, b) => capLabel(context, a).compareTo(capLabel(context, b)));
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      if (parsed != null)
        DropdownButtonFormField<String?>(
          isExpanded: true,
          initialValue: parsed.$1,
          decoration: InputDecoration(labelText: l.where),
          items: [
            DropdownMenuItem(value: null, child: Text(l.wholeHome)),
            for (final r in rooms) DropdownMenuItem(value: r, child: Text(r)),
          ],
          onChanged: (room) {
            target['select'] = roomSelector(room, cap);
            onChanged(false);
          },
        )
      else
        _TextValue(
          label: l.advancedSelector,
          value: select,
          onChanged: (v) {
            target['select'] = v;
            onChanged(false);
          },
        ),
      const SizedBox(height: 10),
      DropdownButtonFormField<String>(
        isExpanded: true,
        initialValue: cap,
        decoration: InputDecoration(labelText: l.what),
        items: [
          for (final c in choices)
            DropdownMenuItem(
              value: c,
              // Kept as written, though no device has it now.
              child: Text(caps.contains(c) ? capLabel(context, c) : l.capNoDevice(capLabel(context, c))),
            ),
        ],
        onChanged: (c) {
          if (c == null || c == cap) return;
          target['cap'] = c;
          if (parsed != null) target['select'] = roomSelector(parsed.$1, c);
          onChanged(true);
        },
      ),
      _Matches(core: core, select: target['select'] as String),
    ]);
  }
}

/// The devices a target matches now, or that it matches none.
class _Matches extends StatelessWidget {
  const _Matches({required this.core, required this.select});

  final CoreClient core;
  final String select;

  @override
  Widget build(BuildContext context) {
    final matched = matching(core.entities.values, select);
    if (matched == null) return const SizedBox();
    final l = AppLocalizations.of(context);
    final style = Theme.of(context).textTheme.bodyMedium;
    return Padding(
      padding: const EdgeInsets.only(top: 8),
      child: matched.isEmpty
          ? Text(l.matchesNothing, style: style?.copyWith(color: Hm.danger))
          : Text(
              // A few names, then how many more.
              l.appliesTo([...matched.take(6).map((e) => e.name), if (matched.length > 6) '+${matched.length - 6}'].join(', ')),
              style: style?.copyWith(color: Hm.textDim),
            ),
    );
  }
}

/// A capability's value: on/off, a slider between its bounds, or typed.
class _Value extends StatelessWidget {
  const _Value({super.key, required this.cap, required this.meta, required this.value, required this.onChanged, this.number = false});

  final String cap;
  final Map<String, dynamic> meta;
  final Object? value;
  final ValueChanged<Object> onChanged;

  /// A threshold: always a number.
  final bool number;

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    if (!number && _isBool(cap, meta)) {
      return SegmentedButton<bool>(
        showSelectedIcon: false,
        segments: [
          ButtonSegment(value: true, label: Text(valueText(context, cap, true))),
          ButtonSegment(value: false, label: Text(valueText(context, cap, false))),
        ],
        selected: {value == true},
        onSelectionChanged: (s) => onChanged(s.first),
      );
    }
    final (min, max) = (meta['min'] as num? ?? 0, meta['max'] as num? ?? (const {'brightness', 'volume'}.contains(cap) ? 100 : null));
    if ((number || _isNumber(cap, meta)) && max != null) {
      final v = ((value as num?) ?? min).clamp(min, max).toDouble();
      return Row(children: [
        Expanded(
          child: Slider(
            value: v,
            min: min.toDouble(),
            max: max.toDouble(),
            divisions: (max - min).round().clamp(1, 1000),
            semanticFormatterCallback: (x) => valueText(context, cap, x.round()),
            onChanged: (x) => onChanged(x.round()),
          ),
        ),
        SizedBox(
          width: 64,
          child: Text(valueText(context, cap, v.round()), textAlign: TextAlign.end, style: const TextStyle(fontFeatures: Hm.tabular)),
        ),
      ]);
    }
    final numeric = number || _isNumber(cap, meta);
    return _TextValue(
      label: l.valueLabel,
      value: value == null ? '' : '$value',
      number: numeric,
      onChanged: (text) => onChanged(numeric ? (num.tryParse(text.replaceAll(',', '.')) ?? 0) : text),
    );
  }
}

class _TextValue extends StatefulWidget {
  const _TextValue({super.key, required this.label, required this.value, required this.onChanged, this.number = false});

  final String label;
  final String value;
  final ValueChanged<String> onChanged;
  final bool number;

  @override
  State<_TextValue> createState() => _TextValueState();
}

class _TextValueState extends State<_TextValue> {
  late final _text = TextEditingController(text: widget.value);

  @override
  void dispose() {
    _text.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => TextField(
        controller: _text,
        decoration: InputDecoration(labelText: widget.label),
        keyboardType: widget.number ? const TextInputType.numberWithOptions(decimal: true, signed: true) : TextInputType.text,
        onChanged: widget.onChanged,
      );
}

Map<String, dynamic> _newTest(CoreClient core, {required bool trigger}) {
  final action = _newAction(core)['set'] as Map<String, dynamic>;
  return {'select': action['select'], 'cap': action['cap'], trigger ? 'becomes' : 'is': action['value']};
}

/// A device test: `becomes`, `above`, `below`, `changes` for a trigger; `is`, `above`, `below`
/// (and `all`) for a condition.
class _TestFields extends StatelessWidget {
  const _TestFields({required this.core, required this.test, required this.trigger, required this.onChanged});

  final CoreClient core;
  final Map<String, dynamic> test;
  final bool trigger;
  final VoidCallback onChanged;

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final cap = test['cap'] as String;
    final meta = _meta(core, cap);
    final equal = trigger ? 'becomes' : 'is';
    final kind = test.containsKey('above')
        ? 'above'
        : test.containsKey('below')
            ? 'below'
            : test['changes'] == true
                ? 'changes'
                : equal;
    void setKind(String kind) {
      test.removeWhere((k, _) => const {'becomes', 'is', 'above', 'below', 'changes'}.contains(k));
      test[kind] = switch (kind) {
        'changes' => true,
        'above' || 'below' => _isNumber(cap, meta) ? _defaultValue(cap, meta) : 0,
        _ => _defaultValue(cap, meta),
      };
      onChanged();
    }

    final caps = core.entities.values.expand((e) => e.caps.keys).toSet().toList();
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      _Target(core: core, target: test, caps: caps, onChanged: (capChanged) {
        if (capChanged) setKind(equal);
        onChanged();
      }),
      // Only when there is a choice: a switch can only be "is", a number can also be above or below.
      if (_isNumber(cap, meta) || trigger) ...[
        const SizedBox(height: 12),
        SegmentedButton<String>(
          showSelectedIcon: false,
          segments: [
            ButtonSegment(value: equal, label: Text(trigger ? l.testBecomes : l.testIs)),
            if (_isNumber(cap, meta)) ...[
              ButtonSegment(value: 'above', label: Text(trigger ? l.testAbove : l.testOver)),
              ButtonSegment(value: 'below', label: Text(trigger ? l.testBelow : l.testUnder)),
            ],
            if (trigger) ButtonSegment(value: 'changes', label: Text(l.testChanges)),
          ],
          selected: {kind},
          onSelectionChanged: (s) => setKind(s.first),
        ),
      ],
      if (kind != 'changes') ...[
        const SizedBox(height: 10),
        _Value(
          key: ValueKey('$cap $kind'),
          cap: cap,
          meta: meta,
          value: test[kind],
          number: kind == 'above' || kind == 'below',
          onChanged: (v) {
            test[kind] = v;
            onChanged();
          },
        ),
      ],
      if (!trigger)
        SwitchListTile(
          contentPadding: EdgeInsets.zero,
          title: Text(l.allMustPass),
          value: test['all'] == true,
          onChanged: (v) {
            v ? test['all'] = true : test.remove('all');
            onChanged();
          },
        ),
    ]);
  }
}

class _SetFields extends StatelessWidget {
  const _SetFields({required this.core, required this.set, required this.onChanged});

  final CoreClient core;
  final Map<String, dynamic> set;
  final VoidCallback onChanged;

  @override
  Widget build(BuildContext context) {
    final cap = set['cap'] as String;
    final meta = _meta(core, cap);
    final caps = core.entities.values.expand((e) => e.caps.keys.where((c) => _writable(e, c))).toSet().toList();
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      _Target(core: core, target: set, caps: caps, onChanged: (capChanged) {
        if (capChanged) set['value'] = _defaultValue(set['cap'] as String, _meta(core, set['cap'] as String));
        onChanged();
      }),
      const SizedBox(height: 12),
      _Value(
        key: ValueKey(cap),
        cap: cap,
        meta: meta,
        value: set['value'],
        onChanged: (v) {
          set['value'] = v;
          onChanged();
        },
      ),
    ]);
  }
}

/// Where an action can be invoked: rooms, then the devices of a plugin that has actions (`plugin:pc`).
List<(String, String)> _invokeScopes(BuildContext context, CoreClient core) {
  final withActions = core.entities.values.where((e) => e.actions.isNotEmpty);
  return [
    for (final r in withActions.map((e) => e.room).nonNulls.toSet().toList()..sort()) ('room:"$r"', r),
    for (final p in withActions.map((e) => e.plugin).toSet().toList()..sort()) ('plugin:$p', targetText(context, 'plugin:$p')),
  ];
}

Map<String, dynamic> _newInvoke(CoreClient core) {
  final device = core.entities.values.where((e) => e.actions.any((a) => a.params == null)).firstOrNull;
  return {
    'select': device == null ? 'plugin:pc' : 'plugin:${device.plugin}',
    'action': device?.actions.firstWhere((a) => a.params == null).name ?? 'play_pause',
  };
}

class _InvokeFields extends StatelessWidget {
  const _InvokeFields({required this.core, required this.invoke, required this.onChanged});

  final CoreClient core;
  final Map<String, dynamic> invoke;
  final VoidCallback onChanged;

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final select = invoke['select'] as String;
    final scopes = _invokeScopes(context, core);
    final known = scopes.any((s) => s.$1 == select);
    final decls = {
      for (final e in matching(core.entities.values, select) ?? const <Entity>[]) for (final a in e.actions) a.name: a,
    };
    final action = invoke['action'] as String;
    final properties = ((decls[action]?.params?['properties'] as Map?) ?? const {}).cast<String, dynamic>();
    final params = ((invoke['params'] as Map?) ?? const {}).cast<String, dynamic>();
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      if (known)
        DropdownButtonFormField<String>(
          isExpanded: true,
          initialValue: select,
          decoration: InputDecoration(labelText: l.where),
          items: [for (final (value, label) in scopes) DropdownMenuItem(value: value, child: Text(label))],
          onChanged: (v) {
            invoke['select'] = v;
            onChanged();
          },
        )
      else
        _TextValue(label: l.advancedSelector, value: select, onChanged: (v) {
          invoke['select'] = v;
          onChanged();
        }),
      const SizedBox(height: 10),
      if (decls.isEmpty && known)
        Text(l.noActionHere, style: Theme.of(context).textTheme.bodyMedium?.copyWith(color: Hm.danger))
      else
        DropdownButtonFormField<String>(
          isExpanded: true,
          initialValue: action,
          decoration: InputDecoration(labelText: l.actionToRun),
          items: [
            for (final name in {...decls.keys, action}) DropdownMenuItem(value: name, child: Text(actionLabel(context, name))),
          ],
          onChanged: (v) {
            invoke['action'] = v;
            invoke.remove('params');
            // A choice to make (an app, a service): the first, so it is never left empty.
            final fields = ((decls[v]?.params?['properties'] as Map?) ?? const {}).cast<String, dynamic>();
            final firsts = {
              for (final MapEntry(:key, :value) in fields.entries)
                if ((value as Map?)?['enum'] case [final first, ...]) key: first,
            };
            if (firsts.isNotEmpty) invoke['params'] = firsts;
            onChanged();
          },
        ),
      for (final MapEntry(key: name, value: schema) in properties.entries)
        Padding(
          padding: const EdgeInsets.only(top: 10),
          child: (schema as Map?)?['enum'] is List
              // The plugin lists what it accepts (the PC's apps, its services): a menu.
              ? DropdownButtonFormField<String>(
                  key: ValueKey('$action $name'),
                  isExpanded: true,
                  initialValue: (schema!['enum'] as List).contains(params[name]) ? params[name] as String : null,
                  decoration: InputDecoration(labelText: _paramLabel(context, name)),
                  items: [for (final v in (schema['enum'] as List).cast<String>()) DropdownMenuItem(value: v, child: Text(v))],
                  onChanged: (v) {
                    invoke['params'] = {...params, name: v};
                    onChanged();
                  },
                )
              : _TextValue(
            key: ValueKey('$action $name'),
            label: _paramLabel(context, name),
            value: '${params[name] ?? ''}',
            number: schema?['type'] == 'integer' || schema?['type'] == 'number',
            onChanged: (text) {
              final next = {...params, name: schema?['type'] == 'string' ? text : (num.tryParse(text) ?? text)};
              invoke['params'] = next;
              onChanged();
            },
          ),
        ),
      _Matches(core: core, select: invoke['select'] as String),
    ]);
  }
}

/// The words of the common action parameters; others humanized.
String _paramLabel(BuildContext context, String name) {
  final l = AppLocalizations.of(context);
  return switch (name) {
    'app' => l.paramApp,
    'shortcut' => l.paramShortcut,
    _ => name.isEmpty ? name : name[0].toUpperCase() + name.substring(1).replaceAll('_', ' '),
  };
}
