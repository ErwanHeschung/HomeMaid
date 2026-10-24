/// The automation editor: a flow, as in n8n. What starts it on top (its triggers and conditions),
/// then its steps as cards linked top to bottom; the "+" under a step adds one after it, a tap opens
/// its settings. Saved through the core, which checks it as at startup, writes it to
/// homemaid.toml and puts it in force at once.
library;

import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:intl/intl.dart';

import '../clients.dart';
import '../flow.dart';
import '../l10n/app_localizations.dart';
import '../model.dart';
import '../selectors.dart';
import '../theme.dart';
import '../widgets/flow_canvas.dart';
import '../widgets/state.dart';
import '../widgets/value_field.dart';
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

List<Map<String, dynamic>> _list(Map<String, dynamic> m, String key) => (m[key] as List).cast<Map<String, dynamic>>();

bool _writable(Entity e, String cap) => e.caps[cap] != null && e.caps[cap]!.meta['read_only'] != true;

/// The metadata of `cap` on the first device that has it.
Map<String, dynamic> _meta(CoreClient core, String cap) =>
    core.entities.values.map((e) => e.caps[cap]?.meta).nonNulls.firstOrNull ?? const {};

Map<String, dynamic> _newTrigger() => {'at': '19:00'};

/// Something the home has: a switch if there is one, else a level, else any setting.
Map<String, dynamic> _newAction(CoreClient core) {
  final writable = [for (final e in core.entities.values) for (final c in e.caps.keys) if (_writable(e, c)) (e, c)];
  final pick = ['on_off', 'brightness', 'volume'].map((cap) => writable.where((w) => w.$2 == cap).firstOrNull).nonNulls.firstOrNull ??
      writable.firstOrNull;
  final cap = pick?.$2 ?? 'on_off';
  return {'set': {'select': roomSelector(pick?.$1.room, cap), 'cap': cap, 'value': defaultCapValue(cap, _meta(core, cap))}};
}

Map<String, dynamic> _newCondition(CoreClient core) => {'state': _newTest(core, trigger: false)};

/// A new node of `kind`, then the nodes of its body (a repeat's).
List<Map<String, dynamic>> _newNodes(CoreClient core, FlowGraph graph, String kind) {
  final id = graph.newId(kind);
  return switch (kind) {
    'set' => [{'id': id, ..._newAction(core)}],
    'invoke' => [{'id': id, 'invoke': newInvoke(core)}],
    'scene' => [{'id': id, 'scene': core.scenes.firstOrNull?.id ?? ''}],
    'delay' => [{'id': id, 'delay': '5m'}],
    'wait' => [{'id': id, 'wait': {'until': [_newCondition(core)], 'timeout': '30m'}}],
    'if' => [{'id': id, 'if': [_newCondition(core)]}],
    'repeat' => [
        {'id': id, 'repeat': {'count': 3}, 'body': [graph.newId('set')]},
        {'id': graph.newId('set'), ..._newAction(core)},
      ],
    _ => [{'id': id, 'stop': <String, dynamic>{}}],
  };
}

const _modes = ['single', 'restart', 'queued', 'parallel'];

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
  late String _mode = widget.automation?.config['mode'] as String? ?? 'single';
  late final _triggers = [for (final t in widget.automation?.triggers ?? [_newTrigger()]) _copy(t)];
  late final _conditions = [for (final c in widget.automation?.conditions ?? const <Map<String, dynamic>>[]) _copy(c)];
  late final _graph = widget.automation == null
      ? FlowGraph(['set-1'], [{'id': 'set-1', ..._newAction(widget.core)}])
      : FlowGraph.of(widget.automation!.config);
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
      if (_mode != 'single') 'mode': _mode,
      'trigger': _triggers,
      if (_conditions.isNotEmpty) 'condition': _conditions,
      ..._graph.toConfig(),
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
        if (!mounted) return;
        setState(() => _dry = dry);
        await showModalBottomSheet<void>(
          context: context,
          isScrollControlled: true,
          useSafeArea: true,
          builder: (_) => SingleChildScrollView(
            padding: const EdgeInsets.only(bottom: 24),
            // The sheet's whole width, not its text's.
            child: SizedBox(width: double.infinity, child: DryRunView(dry: dry, core: widget.core, config: _config)),
          ),
        );
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

  /// A sheet that edits in place: the canvas follows each change behind it.
  Future<void> _sheet(Widget Function(BuildContext context, VoidCallback changed) build) => showModalBottomSheet<void>(
        context: context,
        isScrollControlled: true,
        useSafeArea: true,
        builder: (context) => StatefulBuilder(
          builder: (context, setSheet) => Padding(
            padding: EdgeInsets.only(bottom: MediaQuery.viewInsetsOf(context).bottom),
            child: SingleChildScrollView(
              padding: const EdgeInsets.only(bottom: 24),
              child: build(context, () {
                setSheet(() {});
                _changed();
              }),
            ),
          ),
        ),
      );

  /// Its triggers and conditions: what starts it.
  Future<void> _editStart() => _sheet((context, changed) {
        final l = AppLocalizations.of(context);
        final core = widget.core;
        VoidCallback? removing(List<Map<String, dynamic>> list, Map<String, dynamic> item, {bool keepOne = false}) =>
            keepOne && list.length == 1 ? null : () { list.remove(item); changed(); };
        return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          SectionHeader(l.sectionWhen),
          for (final t in _triggers)
            _TriggerCard(key: ObjectKey(t), core: core, trigger: t, onChanged: changed, onRemove: removing(_triggers, t, keepOne: true)),
          _AddButton(l.addTrigger, () { _triggers.add(_newTrigger()); changed(); }),
          SectionHeader(l.sectionIf),
          for (final c in _conditions)
            _ConditionCard(key: ObjectKey(c), core: core, condition: c, triggers: _triggers, onChanged: changed, onRemove: removing(_conditions, c)),
          _AddButton(l.addCondition, () { _conditions.add({'between': ['18:00', '23:00']}); changed(); }),
        ]);
      });

  Future<void> _editNode(String id) => _sheet((context, changed) {
        final l = AppLocalizations.of(context);
        final node = _graph.node(id);
        if (node == null) return const SizedBox();
        return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(20, 0, 20, 8),
            child: Row(children: [
              Icon(kindIcon(kindOf(node)), color: Hm.select),
              const SizedBox(width: 12),
              Expanded(child: Text(kindLabel(context, kindOf(node)), style: Theme.of(context).textTheme.titleLarge)),
            ]),
          ),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 20),
            child: _NodeFields(core: widget.core, node: node, triggers: _triggers, onChanged: changed),
          ),
          Padding(
            padding: const EdgeInsets.fromLTRB(12, 16, 20, 0),
            child: Align(
              alignment: Alignment.centerLeft,
              child: TextButton.icon(
                style: TextButton.styleFrom(foregroundColor: Hm.danger),
                icon: const Icon(Icons.delete_outline),
                label: Text(l.removeStep),
                onPressed: () {
                  _graph.remove(id);
                  _changed();
                  Navigator.pop(context);
                },
              ),
            ),
          ),
        ]);
      });

  /// What to put after a port: a new step (between it and what followed, or as a branch of its
  /// own), or a link to a step already there.
  Future<void> _add(String? from, String port) async {
    final l = AppLocalizations.of(context);
    final core = widget.core;
    var parallel = false;
    final added = await showModalBottomSheet<String>(
      context: context,
      isScrollControlled: true,
      useSafeArea: true,
      builder: (context) => StatefulBuilder(
        builder: (context, setSheet) => SingleChildScrollView(
          padding: const EdgeInsets.only(bottom: 16),
          child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
            // Where it goes, in words: "If it fails…", "Otherwise…".
            SectionHeader(switch (port) {
              'next' => l.addThen,
              'error' => l.addOnError,
              'timeout' => l.addOnTimeout,
              'body' => l.addEachTurn,
              'then' => l.addIfYes,
              'else' => l.addIfNo,
              _ => l.addStep,
            }),
            for (final kind in const ['set', 'invoke', 'scene', 'delay', 'wait', 'if', 'repeat', 'stop'])
              ListTile(
                leading: Icon(kindIcon(kind)),
                title: Text(kindLabel(context, kind)),
                subtitle: Text(switch (kind) {
                  'set' => l.kindSetHint,
                  'invoke' => l.kindInvokeHint,
                  'scene' => l.kindSceneHint,
                  'delay' => l.kindDelayHint,
                  'wait' => l.kindWaitHint,
                  'if' => l.kindIfHint,
                  'repeat' => l.kindRepeatHint,
                  _ => l.kindStopHint,
                }),
                onTap: () {
                  final nodes = _newNodes(core, _graph, kind);
                  _graph.insert(from, port, nodes, parallel: parallel);
                  Navigator.pop(context, nodes.first['id'] as String);
                },
              ),
            if (_graph.targets(from, port).isNotEmpty)
              SwitchListTile(
                title: Text(l.addParallel),
                value: parallel,
                onChanged: (v) => setSheet(() => parallel = v),
              ),
            if (_graph.joinable(from, port).isNotEmpty) ...[
              SectionHeader(l.joinStep),
              for (final id in _graph.joinable(from, port))
                ListTile(
                  leading: Icon(kindIcon(kindOf(_graph.node(id)!))),
                  title: Text(nodeText(context, core, _graph.node(id)!, triggers: _triggers), maxLines: 2, overflow: TextOverflow.ellipsis),
                  onTap: () {
                    _graph.join(from, port, id);
                    Navigator.pop(context, '');
                  },
                ),
            ],
          ]),
        ),
      ),
    );
    if (added == null || !mounted) return;
    _changed();
    // A new step opens on its settings, as it is rarely right as it comes.
    if (added.isNotEmpty) await _editNode(added);
  }

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final theme = Theme.of(context);
    final core = widget.core;
    final dry = _dry;
    final canWait = _mode != 'single' || _graph.nodes.any((n) => const {'delay', 'wait', 'repeat'}.contains(kindOf(n)));
    final modes = {
      'single': (l.modeSingle, l.modeSingleHint),
      'restart': (l.modeRestart, l.modeRestartHint),
      'queued': (l.modeQueued, l.modeQueuedHint),
      'parallel': (l.modeParallel, l.modeParallelHint),
    };
    return Scaffold(
      appBar: AppBar(
        title: Text(widget.automation == null ? l.newAutomation : l.editAutomation),
        actions: [
          if (widget.automation != null)
            IconButton(icon: const Icon(Icons.delete_outline), tooltip: l.deleteAutomation, onPressed: _busy ? null : _delete),
        ],
      ),
      body: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
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
          padding: const EdgeInsets.fromLTRB(20, 4, 12, 8),
          child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
            // Only when it can matter: a run that waits can be triggered again meanwhile.
            if (canWait) ...[
              Expanded(
                child: DropdownButtonFormField<String>(
                  isExpanded: true,
                  itemHeight: null,
                  initialValue: _modes.contains(_mode) ? _mode : 'single',
                  decoration: InputDecoration(labelText: l.modeLabel, helperText: modes[_mode]?.$2, helperMaxLines: 2),
                  selectedItemBuilder: (context) => [for (final m in _modes) Text(modes[m]!.$1, overflow: TextOverflow.ellipsis)],
                  items: [
                    for (final m in _modes)
                      DropdownMenuItem(
                        value: m,
                        child: Padding(
                          padding: const EdgeInsets.symmetric(vertical: 6),
                          child: Column(crossAxisAlignment: CrossAxisAlignment.start, mainAxisSize: MainAxisSize.min, children: [
                            Text(modes[m]!.$1),
                            Text(modes[m]!.$2, style: theme.textTheme.bodySmall?.copyWith(color: Hm.textDim)),
                          ]),
                        ),
                      ),
                  ],
                  onChanged: (v) {
                    _mode = v ?? 'single';
                    _changed();
                  },
                ),
              ),
              const SizedBox(width: 12),
            ] else
              const Spacer(),
            Padding(padding: const EdgeInsets.only(top: 14), child: Text(l.automationEnabled, style: theme.textTheme.bodyMedium)),
            Padding(
              padding: const EdgeInsets.only(top: 2),
              child: Switch(
                value: _enabled,
                onChanged: (v) {
                  _enabled = v;
                  _changed();
                },
              ),
            ),
          ]),
        ),
        const Divider(height: 1),
        // A first flow: how it works, once.
        if (_graph.nodes.length <= 1)
          Padding(
            padding: const EdgeInsets.fromLTRB(20, 12, 20, 0),
            child: Text(l.flowHint, style: theme.textTheme.bodyMedium?.copyWith(color: Hm.textDim)),
          ),
        Expanded(
          child: FlowCanvas(
            graph: _graph,
            root: rootText(context, core, _triggers, _conditions),
            summary: (n) => nodeText(context, core, n, triggers: _triggers),
            icon: (n) => kindIcon(kindOf(n)),
            portLabel: (p) => portLabel(context, p),
            marks: dry == null ? null : _dryMarks(dry),
            onRoot: _editStart,
            onNode: _editNode,
            onPort: _add,
          ),
        ),
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

  /// Where a dry run goes: through the steps it would take, stopping at a wait that would wait.
  Map<String, NodeMark> _dryMarks(DryRun dry) => {
        for (final p in dry.path)
          p.node: switch (_graph.node(p.node)) {
            final n? when !p.ok && kindOf(n) == 'wait' => NodeMark.waiting,
            final n? when !p.ok && kindOf(n) == 'stop' => NodeMark.failed,
            _ => NodeMark.passed,
          },
      };
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

/// A node's settings, by kind.
class _NodeFields extends StatelessWidget {
  const _NodeFields({required this.core, required this.node, required this.triggers, required this.onChanged});

  final CoreClient core;
  final Map<String, dynamic> node;
  final List<Map<String, dynamic>> triggers;
  final VoidCallback onChanged;

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final n = node;
    final fromDevice = triggers.any((t) => t.containsKey('when'));
    switch (kindOf(n)) {
      case 'set':
        return _SetFields(core: core, set: _sub(n, 'set'), onChanged: onChanged, fromTrigger: fromDevice);
      case 'invoke':
        return InvokeFields(core: core, invoke: _sub(n, 'invoke'), onChanged: onChanged);
      case 'scene':
        final scenes = core.scenes;
        if (scenes.isEmpty) return Text(l.noScenes, style: Theme.of(context).textTheme.bodyMedium?.copyWith(color: Hm.danger));
        return DropdownButtonFormField<String>(
          isExpanded: true,
          initialValue: scenes.any((s) => s.id == n['scene']) ? n['scene'] as String : null,
          decoration: InputDecoration(labelText: l.kindScene),
          items: [for (final s in scenes) DropdownMenuItem(value: s.id, child: Text(s.name))],
          onChanged: (id) {
            n['scene'] = id;
            onChanged();
          },
        );
      case 'delay':
        return _Duration(value: n['delay'] as String, onChanged: (v) { n['delay'] = v; onChanged(); });
      case 'wait':
        final wait = _sub(n, 'wait');
        final event = wait.containsKey('trigger');
        return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          SegmentedButton<bool>(
            showSelectedIcon: false,
            segments: [ButtonSegment(value: false, label: Text(l.waitConditions)), ButtonSegment(value: true, label: Text(l.waitEvent))],
            selected: {event},
            onSelectionChanged: (s) {
              wait.remove('until');
              wait.remove('trigger');
              s.first ? wait['trigger'] = {'when': _newTest(core, trigger: true)} : wait['until'] = [_newCondition(core)];
              onChanged();
            },
          ),
          const SizedBox(height: 8),
          if (event)
            _TriggerCard(core: core, trigger: _sub(wait, 'trigger'), onChanged: onChanged, onRemove: null, inset: false)
          else
            _Conditions(core: core, list: _list(wait, 'until'), triggers: triggers, onChanged: onChanged),
          SwitchListTile(
            contentPadding: EdgeInsets.zero,
            title: Text(l.limitWait),
            value: wait['timeout'] is String,
            onChanged: (on) {
              on ? wait['timeout'] = '30m' : wait.remove('timeout');
              onChanged();
            },
          ),
          if (wait['timeout'] case final String timeout) _Duration(value: timeout, onChanged: (v) { wait['timeout'] = v; onChanged(); }),
        ]);
      case 'if':
        return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          _Conditions(core: core, list: _list(n, 'if'), triggers: triggers, onChanged: onChanged),
          SwitchListTile(
            contentPadding: EdgeInsets.zero,
            title: Text(l.anyCondition),
            value: n['any'] == true,
            onChanged: (v) {
              v ? n['any'] = true : n.remove('any');
              onChanged();
            },
          ),
        ]);
      case 'repeat':
        final repeat = _sub(n, 'repeat');
        final kind = const ['while', 'until', 'for_each'].firstWhere(repeat.containsKey, orElse: () => 'count');
        final each = {'select': repeat['for_each'] ?? '', 'cap': parseRoomSelector(repeat['for_each'] as String? ?? '')?.$2 ?? 'on_off'};
        return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          SegmentedButton<String>(
            showSelectedIcon: false,
            segments: [
              ButtonSegment(value: 'count', label: Text(l.repeatTimes)),
              ButtonSegment(value: 'while', label: Text(l.repeatWhileKind)),
              ButtonSegment(value: 'until', label: Text(l.repeatUntilKind)),
              ButtonSegment(value: 'for_each', label: Text(l.repeatEachKind)),
            ],
            selected: {kind},
            onSelectionChanged: (s) {
              repeat.clear();
              repeat.addAll(switch (s.first) {
                'while' || 'until' => {s.first: [_newCondition(core)]},
                'for_each' => {'for_each': (_newAction(core)['set'] as Map)['select']},
                _ => {'count': 3},
              });
              onChanged();
            },
          ),
          const SizedBox(height: 12),
          switch (kind) {
            'while' || 'until' => _Conditions(core: core, list: _list(repeat, kind), triggers: triggers, onChanged: onChanged),
            'for_each' => _Target(
                core: core,
                target: each,
                caps: core.entities.values.expand((e) => e.caps.keys).toSet().toList(),
                onChanged: (_) { repeat['for_each'] = each['select']; onChanged(); },
              ),
            _ => ValueField(
                cap: 'count',
                meta: const {'min': 1, 'max': 20},
                value: repeat['count'] as int? ?? 3,
                number: true,
                onChanged: (v) { repeat['count'] = (v as num).round(); onChanged(); },
              ),
          },
        ]);
      case 'merge':
        return SegmentedButton<String>(
          showSelectedIcon: false,
          segments: [ButtonSegment(value: 'all', label: Text(l.mergeAll)), ButtonSegment(value: 'any', label: Text(l.mergeAny))],
          selected: {n['merge'] as String? ?? 'all'},
          onSelectionChanged: (s) { n['merge'] = s.first; onChanged(); },
        );
      default:
        final stop = _sub(n, 'stop');
        return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          SwitchListTile(
            contentPadding: EdgeInsets.zero,
            title: Text(l.stopFails),
            value: stop['error'] is String,
            onChanged: (v) {
              v ? stop['error'] = l.runFailed : stop.remove('error');
              onChanged();
            },
          ),
          if (stop['error'] case final String error)
            TextValueField(label: l.stopWhy, value: error, onChanged: (v) { stop['error'] = v; onChanged(); }),
        ]);
    }
  }
}

/// A list of conditions, all to pass; at least one.
class _Conditions extends StatelessWidget {
  const _Conditions({required this.core, required this.list, required this.triggers, required this.onChanged});

  final CoreClient core;
  final List<Map<String, dynamic>> list;
  final List<Map<String, dynamic>> triggers;
  final VoidCallback onChanged;

  @override
  Widget build(BuildContext context) => Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        for (final c in list)
          _ConditionCard(
            key: ObjectKey(c),
            core: core,
            condition: c,
            triggers: triggers,
            inset: false,
            onChanged: onChanged,
            onRemove: list.length == 1 ? null : () { list.remove(c); onChanged(); },
          ),
        Align(
          alignment: Alignment.centerLeft,
          child: TextButton.icon(
            onPressed: () { list.add(_newCondition(core)); onChanged(); },
            icon: const Icon(Icons.add),
            label: Text(AppLocalizations.of(context).addCondition),
          ),
        ),
      ]);
}

/// A duration as the core writes it (`10m`, `1h30m`), with the usual ones a tap away.
class _Duration extends StatelessWidget {
  const _Duration({required this.value, required this.onChanged});

  final String value;
  final ValueChanged<String> onChanged;

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      Wrap(spacing: 6, runSpacing: 6, children: [
        for (final d in const ['30s', '1m', '5m', '15m', '30m', '1h', '2h'])
          ChoiceChip(label: Text(durationText(d)), selected: value == d, onSelected: (_) => onChanged(d)),
      ]),
      const SizedBox(height: 10),
      TextValueField(
        label: '${l.durationLabel} · ${durationText(value)}',
        value: value,
        // Kept only once it reads; the core would refuse it anyway.
        onChanged: (v) {
          if (durationSeconds(v.trim()) != null) onChanged(v.trim());
        },
        error: durationSeconds(value) == null ? l.durationInvalid : null,
      ),
    ]);
  }
}

/// A trigger, condition or action: its kind, its fields, and a way out.
class _Card extends StatelessWidget {
  const _Card({required this.kinds, required this.kind, required this.onKind, required this.onRemove, required this.children, this.inset = true});

  final List<(String, String)> kinds;
  final String kind;
  final ValueChanged<String> onKind;
  final VoidCallback? onRemove;
  final List<Widget> children;

  /// Its own side margins; none inside a node's settings, which have theirs.
  final bool inset;

  @override
  Widget build(BuildContext context) => Container(
        margin: EdgeInsets.fromLTRB(inset ? 20 : 0, 4, inset ? 20 : 0, 8),
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
  const _TriggerCard({super.key, required this.core, required this.trigger, required this.onChanged, required this.onRemove, this.inset = true});

  final CoreClient core;
  final Map<String, dynamic> trigger;
  final VoidCallback onChanged;
  final VoidCallback? onRemove;
  final bool inset;

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final t = trigger;
    final device = t.containsKey('when');
    return _Card(
      kinds: [('moment', l.kindMoment), ('device', l.kindDevice)],
      kind: device ? 'device' : 'moment',
      onRemove: onRemove,
      inset: inset,
      onKind: (kind) {
        // Its id stays: a `trigger` condition may name it.
        final id = t['id'];
        t.clear();
        t.addAll({'id': ?id, ...kind == 'device' ? {'when': _newTest(core, trigger: true)} : _newTrigger()});
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
  const _ConditionCard({
    super.key,
    required this.core,
    required this.condition,
    required this.triggers,
    required this.onChanged,
    required this.onRemove,
    this.inset = true,
  });

  final CoreClient core;
  final Map<String, dynamic> condition;

  /// The automation's: with several, a condition can say which one started the run.
  final List<Map<String, dynamic>> triggers;
  final VoidCallback onChanged;
  final VoidCallback? onRemove;
  final bool inset;

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final c = condition;
    final kind = const ['state', 'days', 'trigger'].firstWhere(c.containsKey, orElse: () => 'between');
    return _Card(
      kinds: [
        ('between', l.kindTimeRange),
        ('days', l.kindDays),
        ('state', l.kindState),
        if (triggers.length > 1 || kind == 'trigger') ('trigger', l.kindTrigger),
      ],
      kind: kind,
      onRemove: onRemove,
      inset: inset,
      onKind: (kind) {
        c.clear();
        if (kind == 'trigger') {
          // Named to be told apart: t1, t2...
          for (final (i, t) in triggers.indexed) {
            t['id'] ??= 't${i + 1}';
          }
        }
        c.addAll(switch (kind) {
          'days' => {'days': [...allDays.take(5)]},
          'state' => {'state': _newTest(core, trigger: false)},
          'trigger' => {'trigger': triggers.first['id']},
          _ => {'between': ['18:00', '23:00']},
        });
        onChanged();
      },
      children: switch (kind) {
        'trigger' => [
            DropdownButtonFormField<String>(
              isExpanded: true,
              initialValue: triggers.any((t) => t['id'] == c['trigger']) ? c['trigger'] as String : null,
              decoration: InputDecoration(labelText: l.kindTrigger),
              items: [
                for (final t in triggers)
                  if (t['id'] case final String id)
                    DropdownMenuItem(value: id, child: Text(triggerText(context, core, t), maxLines: 2, overflow: TextOverflow.ellipsis)),
              ],
              onChanged: (id) { c['trigger'] = id; onChanged(); },
            ),
          ],
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
        TextValueField(
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
        'above' || 'below' => isNumberCap(cap, meta) ? defaultCapValue(cap, meta) : 0,
        _ => defaultCapValue(cap, meta),
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
      if (isNumberCap(cap, meta) || trigger) ...[
        const SizedBox(height: 12),
        SegmentedButton<String>(
          showSelectedIcon: false,
          segments: [
            ButtonSegment(value: equal, label: Text(trigger ? l.testBecomes : l.testIs)),
            if (isNumberCap(cap, meta)) ...[
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
        ValueField(
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
  const _SetFields({required this.core, required this.set, required this.onChanged, this.fromTrigger = false});

  final CoreClient core;
  final Map<String, dynamic> set;
  final VoidCallback onChanged;

  /// A device starts it: the value can be the one it took.
  final bool fromTrigger;

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final cap = set['cap'] as String;
    final meta = _meta(core, cap);
    final caps = core.entities.values.expand((e) => e.caps.keys.where((c) => _writable(e, c))).toSet().toList();
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      _Target(core: core, target: set, caps: caps, onChanged: (capChanged) {
        if (capChanged && set['from'] == null) set['value'] = defaultCapValue(set['cap'] as String, _meta(core, set['cap'] as String));
        onChanged();
      }),
      if (fromTrigger || set['from'] != null)
        SwitchListTile(
          contentPadding: EdgeInsets.zero,
          title: Text(l.fromTrigger),
          value: set['from'] == 'trigger',
          onChanged: (on) {
            if (on) {
              set.remove('value');
              set['from'] = 'trigger';
            } else {
              set.remove('from');
              set['value'] = defaultCapValue(cap, meta);
            }
            onChanged();
          },
        ),
      if (set['from'] == null) ...[
        const SizedBox(height: 12),
        ValueField(
          key: ValueKey(cap),
          cap: cap,
          meta: meta,
          value: set['value'],
          onChanged: (v) {
            set['value'] = v;
            onChanged();
          },
        ),
      ],
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

/// An action the home can run: the first of a device that needs no parameter.
Map<String, dynamic> newInvoke(CoreClient core) {
  final device = core.entities.values.where((e) => e.actions.any((a) => a.params == null)).firstOrNull;
  return {
    'select': device == null ? 'plugin:pc' : 'plugin:${device.plugin}',
    'action': device?.actions.firstWhere((a) => a.params == null).name ?? 'play_pause',
  };
}

/// Where, which action, and its parameters (a menu when the plugin lists the choices).
class InvokeFields extends StatelessWidget {
  const InvokeFields({super.key, required this.core, required this.invoke, required this.onChanged});

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
        TextValueField(label: l.advancedSelector, value: select, onChanged: (v) {
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
              : TextValueField(
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
