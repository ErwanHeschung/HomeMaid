import 'package:flutter/material.dart';
import 'package:intl/intl.dart';

import '../clients.dart';
import '../flow.dart';
import '../l10n/app_localizations.dart';
import '../model.dart';
import '../theme.dart';
import '../widgets/flow_canvas.dart';
import '../widgets/model_names.dart';
import '../widgets/state.dart';
import 'automation_editor.dart';
import 'common.dart';

/// The core's automations (from homemaid.toml). Each row says when it runs and how its last run
/// went, with its on/off switch; its sheet puts it into words, tries it (dry run), runs it, or
/// opens it in the editor; "+" makes a new one.
class AutomationsScreen extends StatelessWidget {
  const AutomationsScreen({super.key, required this.core, required this.onPair});

  final CoreClient? core;
  final VoidCallback onPair;

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final core = this.core;
    if (core == null) return EmptyState(title: l.pairHomeTitle, hint: l.pairHomeHint, onPair: onPair);
    final theme = Theme.of(context);
    return ListenableBuilder(
      listenable: core,
      builder: (context, _) {
        final list = core.automations;
        return ListView(padding: const EdgeInsets.only(bottom: 16), children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(20, 20, 8, 4),
            child: Row(children: [
              Expanded(child: Text(l.navAutomations, style: theme.textTheme.displaySmall)),
              IconButton(
                icon: const Icon(Icons.add),
                tooltip: l.newAutomation,
                onPressed: core.connected ? () => editAutomation(context, core) : null,
              ),
            ]),
          ),
          Padding(
            padding: const EdgeInsets.fromLTRB(20, 0, 20, 12),
            child: Text(l.automationCount(list.length), style: theme.textTheme.bodyMedium?.copyWith(color: Hm.textDim)),
          ),
          if (!core.connected) OfflineBanner(l.homeUnreachable),
          const Divider(),
          if (list.isEmpty)
            Padding(
              padding: const EdgeInsets.all(24),
              child: Text(l.noAutomations, style: theme.textTheme.bodyMedium?.copyWith(color: Hm.textDim)),
            ),
          for (final a in list) ...[
            _AutomationRow(automation: a, core: core),
            const Divider(indent: 50),
          ],
        ]);
      },
    );
  }
}

/// How the last run went, in the state grammar: pink acted, red failed, outline skipped,
/// dim never ran.
class _RunMark extends StatelessWidget {
  const _RunMark(this.run, {this.size = 14});

  final Run? run;
  final double size;

  @override
  Widget build(BuildContext context) {
    final run = this.run;
    final (fill, ring) = run == null
        ? (Hm.textDim.withValues(alpha: 0.35), null)
        : run.skipped != null
            ? (null, Hm.textDim)
            : run.failed
                ? (Hm.danger, null)
                : (Hm.on, null);
    return Container(
      width: size,
      height: size,
      decoration: BoxDecoration(
        color: fill,
        border: ring == null ? null : Border.all(color: ring, width: 1.5),
      ),
    );
  }
}

class _AutomationRow extends StatelessWidget {
  const _AutomationRow({required this.automation, required this.core});

  final Automation automation;
  final CoreClient core;

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final theme = Theme.of(context);
    final a = automation;
    final when = a.triggers.isEmpty ? '' : triggerText(context, core, a.triggers.first) + (a.triggers.length > 1 ? ' · +${a.triggers.length - 1}' : '');
    final running = a.running.isNotEmpty;
    final status = running
        ? l.runInProgress
        : !a.enabled
            ? l.automationDisabled
            : a.nextRun != null
                ? l.nextAt(whenText(context, a.nextRun!))
                : null;
    return InkWell(
      onTap: () => showModalBottomSheet<void>(
        context: context,
        isScrollControlled: true,
        useSafeArea: true,
        builder: (_) => AutomationSheet(core: core, id: a.id),
      ),
      child: ConstrainedBox(
        constraints: const BoxConstraints(minHeight: 64),
        child: Padding(
          padding: const EdgeInsets.fromLTRB(20, 10, 20, 10),
          child: Row(children: [
            _RunMark(a.runs.firstOrNull),
            const SizedBox(width: 16),
            Expanded(
              child: Column(crossAxisAlignment: CrossAxisAlignment.start, mainAxisSize: MainAxisSize.min, children: [
                Text(a.name, style: theme.textTheme.titleMedium, maxLines: 1, overflow: TextOverflow.ellipsis),
                const SizedBox(height: 2),
                Text(when, style: theme.textTheme.bodyMedium?.copyWith(color: Hm.textDim), maxLines: 2, overflow: TextOverflow.ellipsis),
                if (a.problems.isNotEmpty)
                  Text(l.problemCount(a.problems.length), style: theme.textTheme.bodySmall?.copyWith(color: Hm.danger)),
              ]),
            ),
            if (status != null) ...[
              const SizedBox(width: 12),
              Text(status, style: theme.textTheme.labelSmall?.copyWith(color: running ? Hm.pending : Hm.textDim, fontFeatures: Hm.tabular)),
            ],
            const SizedBox(width: 4),
            Semantics(
              label: a.name,
              child: Switch(
              value: a.config['enabled'] != false,
              onChanged: (on) {
                final config = {...a.config}..remove('enabled');
                core.saveAutomation({...config, if (!on) 'enabled': false}).catchError((Object e) {
                  if (context.mounted) showError(context, e);
                });
              },
            ),
            ),
          ]),
        ),
      ),
    );
  }
}

/// One automation in words, its last runs, and Try it / Run now.
class AutomationSheet extends StatefulWidget {
  const AutomationSheet({super.key, required this.core, required this.id});

  final CoreClient core;
  final String id;

  @override
  State<AutomationSheet> createState() => _AutomationSheetState();
}

class _AutomationSheetState extends State<AutomationSheet> {
  DryRun? _dry;
  bool _busy = false;

  Future<void> _try() async {
    setState(() => _busy = true);
    try {
      final dry = await widget.core.dryRun(widget.id);
      if (mounted) setState(() => _dry = dry);
    } catch (e) {
      if (mounted) showError(context, e);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _run() => _doing(() => widget.core.runAutomation(widget.id));

  Future<void> _stop() => _doing(() => widget.core.stopAutomation(widget.id));

  Future<void> _doing(Future<void> Function() work) async {
    setState(() => _busy = true);
    try {
      await work();
    } catch (e) {
      if (mounted) showError(context, e);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) => ListenableBuilder(
        listenable: widget.core,
        builder: (context, _) {
          final a = widget.core.automations.where((a) => a.id == widget.id).firstOrNull;
          if (a == null) return const SizedBox(height: 120);
          final l = AppLocalizations.of(context);
          final theme = Theme.of(context);
          final core = widget.core;
          final graph = FlowGraph.of(a.config);
          final at = graph.layout();
          final steps = [...graph.nodes]..sort((x, y) {
              final (p, q) = (at[x['id']]!, at[y['id']]!);
              return p.layer != q.layer ? p.layer.compareTo(q.layer) : p.column.compareTo(q.column);
            });
          // A list of settings and actions one after the other reads as well in sentences.
          final plain = graph.nodes.every((n) => const {'set', 'invoke'}.contains(kindOf(n)) && graph.targets(n['id'] as String, 'next').length <= 1) &&
              graph.start.length == 1;
          Widget line(String text, {Color? color}) => Padding(
                padding: const EdgeInsets.fromLTRB(20, 4, 20, 4),
                child: Text(text, style: theme.textTheme.bodyLarge?.copyWith(color: color)),
              );
          return SafeArea(
            top: false,
            child: SingleChildScrollView(
              padding: const EdgeInsets.only(bottom: 16),
              child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
                Padding(
                  padding: const EdgeInsets.fromLTRB(20, 0, 8, 4),
                  child: Row(children: [
                    Expanded(child: Text(a.name, style: theme.textTheme.headlineSmall)),
                    IconButton(
                      icon: const Icon(Icons.edit_outlined),
                      tooltip: l.editAutomation,
                      onPressed: () async {
                        // Deleted from the editor: nothing left to show here.
                        if (await editAutomation(context, core, a) && context.mounted) Navigator.pop(context);
                      },
                    ),
                  ]),
                ),
                for (final p in a.problems) line(p, color: Hm.danger),
                SectionHeader(l.sectionWhen),
                for (final t in a.triggers) line(triggerText(context, core, t)),
                if (a.conditions.isNotEmpty) ...[
                  SectionHeader(l.sectionIf),
                  for (final c in a.conditions) line(conditionText(context, core, c, triggers: a.triggers)),
                ],
                SectionHeader(l.sectionThen),
                // Branches and waits read on the flow, not in a list.
                if (plain) for (final n in steps) line(nodeText(context, core, n, triggers: a.triggers)),
                if (!plain)
                  Padding(
                    // With the button's own 12 px, its icon on the 20 px gutter.
                    padding: const EdgeInsets.fromLTRB(8, 0, 20, 0),
                    child: Align(
                      alignment: Alignment.centerLeft,
                      child: TextButton.icon(
                        icon: const Icon(Icons.account_tree_outlined),
                        label: Text('${l.showFlow} · ${l.stepCount(steps.length)}'),
                        onPressed: () => showFlow(context, core, a, running: a.running.firstOrNull),
                      ),
                    ),
                  ),
                if (a.delegated.isNotEmpty) line(l.runsWithoutHub(a.delegated.join(', ')), color: Hm.textDim),
                for (final r in a.running)
                  InkWell(
                    onTap: () => showFlow(context, core, a, running: r),
                    child: Padding(
                      padding: const EdgeInsets.fromLTRB(20, 12, 20, 4),
                      child: Row(children: [
                        Expanded(
                          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                            Text(l.runningSince(whenText(context, r.at)), style: theme.textTheme.titleSmall?.copyWith(color: Hm.pending)),
                            if (r.nodes.isNotEmpty)
                              Text(
                                l.waitingAt([for (final id in r.nodes) if (graph.node(id) case final n?) nodeText(context, core, n, triggers: a.triggers)].join(', ')),
                                style: theme.textTheme.bodyMedium?.copyWith(color: Hm.textDim),
                              ),
                          ]),
                        ),
                        OutlinedButton(onPressed: _busy ? null : _stop, child: Text(l.stopRuns)),
                      ]),
                    ),
                  ),
                Padding(
                  padding: const EdgeInsets.fromLTRB(20, 20, 20, 4),
                  child: Row(children: [
                    Expanded(child: OutlinedButton(onPressed: _busy ? null : _try, child: Text(l.tryIt))),
                    const SizedBox(width: 12),
                    Expanded(child: FilledButton(onPressed: _busy || !a.enabled ? null : _run, child: Text(l.runNow))),
                  ]),
                ),
                if (_dry != null) DryRunView(dry: _dry!, core: core, config: a.config),
                SectionHeader(l.sectionRuns),
                if (a.runs.isEmpty) line(l.neverRan, color: Hm.textDim),
                for (final run in a.runs) _RunRow(run: run, onTap: run.steps.isEmpty || plain ? null : () => showFlow(context, core, a, run: run)),
              ]),
            ),
          );
        },
      );
}

class DryRunView extends StatelessWidget {
  const DryRunView({super.key, required this.dry, required this.core, this.config});

  final DryRun dry;
  final CoreClient core;

  /// The automation tried, to name its steps.
  final Map<String, dynamic>? config;

  String _step(BuildContext context, String id) {
    final config = this.config;
    final node = config == null ? null : FlowGraph.of(config).node(id);
    if (node == null) return id;
    final triggers = [for (final t in (config!['trigger'] as List?) ?? const []) (t as Map).cast<String, dynamic>()];
    return nodeText(context, core, node, triggers: triggers);
  }

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final theme = Theme.of(context);
    final small = theme.textTheme.bodyMedium;
    return Container(
      margin: const EdgeInsets.fromLTRB(20, 12, 20, 0),
      padding: const EdgeInsets.all(14),
      decoration: BoxDecoration(
        border: Border.all(color: dry.wouldRun ? Hm.select : Hm.lineStrong),
        borderRadius: BorderRadius.circular(Hm.radius),
      ),
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        Text(dry.wouldRun ? l.dryWouldRun : l.dryWouldNot,
            style: theme.textTheme.titleSmall?.copyWith(color: dry.wouldRun ? Hm.select : Hm.textDim)),
        for (final c in dry.conditions)
          Padding(
            padding: const EdgeInsets.only(top: 6),
            child: Text(c.reason, style: small?.copyWith(color: c.ok ? Hm.textDim : Hm.text)),
          ),
        // What decides the way: an `if`, a wait, a delay, a repeat.
        for (final p in dry.path)
          if (p.outcome.isNotEmpty)
            Padding(
              padding: const EdgeInsets.only(top: 6),
              child: Text('${_step(context, p.node)} · ${p.outcome}', style: small?.copyWith(color: Hm.textDim)),
            ),
        for (final a in dry.actions) ...[
          const SizedBox(height: 10),
          Text(actionText(context, core, a.action), style: small),
          if (a.targets.isEmpty) Text(l.dryNoTarget, style: small?.copyWith(color: Hm.danger)),
          for (final t in a.targets)
            Padding(
              padding: const EdgeInsets.only(left: 12, top: 2),
              child: Text(
                '${t.name} · ${t.error ?? (t.changes ? l.dryChanges : l.dryAlready)}',
                style: small?.copyWith(color: t.error != null ? Hm.danger : Hm.textDim, fontFeatures: Hm.tabular),
              ),
            ),
        ],
      ]),
    );
  }
}

class _RunRow extends StatelessWidget {
  const _RunRow({required this.run, this.onTap});

  final Run run;

  /// Shows it on the flow.
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final theme = Theme.of(context);
    final acted = run.results.map((r) => r.target).toSet().join(', ');
    final outcome = run.skipped != null
        ? '${l.runSkipped} · ${run.skipped}'
        : run.stopped != null
            ? '${l.runStopped} · ${stoppedText(context, run.stopped!)}'
            : run.error != null
                ? '${l.runFailed} · ${run.error}'
                : run.failed
                    ? '${l.runFailed} · ${run.results.where((r) => !r.ok).map((r) => '${r.target}: ${r.outcome}').join(', ')}'
                    : acted.isEmpty
                        ? l.runActed
                        : '${l.runActed} · $acted';
    return InkWell(
      onTap: onTap,
      child: Padding(
      padding: const EdgeInsets.fromLTRB(20, 6, 20, 6),
      child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
        Padding(padding: const EdgeInsets.only(top: 4), child: _RunMark(run, size: 10)),
        const SizedBox(width: 14),
        Expanded(
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Text(
              '${whenText(context, run.at)} · ${run.trigger == 'manual' ? l.ranByHand : run.trigger}',
              style: theme.textTheme.labelMedium?.copyWith(color: Hm.textDim, fontFeatures: Hm.tabular),
            ),
            Text(outcome, style: theme.textTheme.bodyMedium?.copyWith(color: run.failed ? Hm.danger : Hm.text)),
          ]),
        ),
        if (onTap != null) const Icon(Icons.account_tree_outlined, size: 18, color: Hm.textDim),
      ]),
      ),
    );
  }
}

/// Opens the automation's flow, read only, with a run on it (one not over, or one that ended).
Future<void> showFlow(BuildContext context, CoreClient core, Automation a, {Run? run, RunningRun? running}) =>
    Navigator.of(context).push(MaterialPageRoute(builder: (_) => FlowViewer(core: core, automation: a, run: run, running: running)));

class FlowViewer extends StatelessWidget {
  const FlowViewer({super.key, required this.core, required this.automation, this.run, this.running});

  final CoreClient core;
  final Automation automation;
  final Run? run;
  final RunningRun? running;

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final a = automation;
    final steps = running?.steps ?? run?.steps;
    final marks = steps == null ? null : runMarks(steps, running?.nodes ?? const []);
    return Scaffold(
      appBar: AppBar(
        title: Text(a.name),
        bottom: steps == null
            ? null
            : PreferredSize(
                preferredSize: const Size.fromHeight(24),
                child: Padding(
                  padding: const EdgeInsets.only(bottom: 8),
                  child: Text(
                    running != null ? l.runInProgress : l.runOf(whenText(context, run!.at)),
                    style: Theme.of(context).textTheme.labelMedium?.copyWith(color: running != null ? Hm.pending : Hm.textDim),
                  ),
                ),
              ),
      ),
      body: FlowCanvas(
        graph: FlowGraph.of(a.config),
        root: rootText(context, core, a.triggers, a.conditions),
        summary: (n) => nodeText(context, core, n, triggers: a.triggers),
        icon: (n) => kindIcon(kindOf(n)),
        portLabel: (p) => portLabel(context, p),
        marks: marks,
        // What it did there: every turn of a repeat.
        onNode: steps == null
            ? null
            : (id) {
                final here = steps.where((s) => s.node == id).toList();
                ScaffoldMessenger.of(context)
                  ..hideCurrentSnackBar()
                  ..showSnackBar(SnackBar(
                    content: Text(here.isEmpty ? l.notReached : here.map((s) => s.outcome).where((o) => o.isNotEmpty).join('\n')),
                  ));
              },
      ),
    );
  }
}

// ------------------------------------------------------------------------------------- words

/// "Today 19:30", "Tomorrow 07:00", "Mon 22:00", "12/10 08:15".
String whenText(BuildContext context, DateTime at) {
  final l = AppLocalizations.of(context);
  final locale = Localizations.localeOf(context).toLanguageTag();
  final time = DateFormat.Hm(locale).format(at);
  final today = DateUtils.dateOnly(DateTime.now());
  final day = DateUtils.dateOnly(at).difference(today).inDays;
  return switch (day) {
    0 => '${l.today} $time',
    1 => '${l.tomorrow} $time',
    -1 || (> 1 && < 7) => '${DateFormat.E(locale).format(at)} $time',
    _ => '${DateFormat.Md(locale).format(at)} $time',
  };
}

String _days(BuildContext context, List<Object?> days) {
  final l = AppLocalizations.of(context);
  final set = days.cast<String>().toSet();
  const order = ['mon', 'tue', 'wed', 'thu', 'fri', 'sat', 'sun'];
  if (set.length == 7) return l.everyDay;
  if (set.length == 5 && !set.contains('sat') && !set.contains('sun')) return l.weekdays;
  if (set.length == 2 && set.containsAll(['sat', 'sun'])) return l.weekend;
  final locale = Localizations.localeOf(context).toLanguageTag();
  // 2024-01-01 was a Monday.
  return [for (final (i, d) in order.indexed) if (set.contains(d)) DateFormat.E(locale).format(DateTime(2024, 1, 1 + i))].join(', ');
}

/// A selector in words where it is simple (`room:Salon & model:lamp` -> "Salon · Lampe"); the
/// selector itself otherwise.
String targetText(BuildContext context, String select) {
  final l = AppLocalizations.of(context);
  if (select == 'trigger') return l.targetTrigger;
  if (select == 'item') return l.targetItem;
  if (RegExp(r'[|!()]').hasMatch(select)) return select;
  final parts = <String>[];
  for (final term in select.split('&').map((t) => t.trim())) {
    final colon = term.indexOf(':');
    if (colon < 0) return select;
    final key = term.substring(0, colon);
    var value = term.substring(colon + 1);
    if (value.startsWith('"') && value.endsWith('"') && value.length > 1) value = value.substring(1, value.length - 1);
    switch (key) {
      case 'cap':
        break; // Said by the capability that follows.
      case 'model':
        parts.add(modelLabel(context, value));
      case 'plugin':
        parts.add(value == 'pc' ? 'PC' : value);
      case 'tag':
        parts.add('#$value');
      default:
        parts.add(value);
    }
  }
  // Only a capability: wherever it is.
  return parts.isEmpty ? l.wholeHome : parts.join(' · ');
}

String valueText(BuildContext context, String cap, Object? value) {
  final l = AppLocalizations.of(context);
  return switch ((cap, value)) {
    ('on_off', true) => l.stateOn,
    ('on_off', false) => l.stateOff,
    ('mute', true) => l.muted,
    ('mute', false) => l.unmute,
    ('brightness' || 'volume', num n) => percent(n),
    (_, bool b) => yesNoWord(l, cap, b) ?? (b ? l.yes : l.no),
    (_, num n) => n % 1 == 0 ? '${n.toInt()}' : '$n',
    _ => '$value',
  };
}

/// `sunset`, `sunrise+15`, `sunset-30` (the core's syntax) in words: with "at" for a trigger, as
/// a noun for a range. Null for a clock time.
String? sunText(BuildContext context, String text, {required bool at}) {
  final m = RegExp(r'^(sunrise|sunset)([+-]\d+)?$').firstMatch(text);
  if (m == null) return null;
  final l = AppLocalizations.of(context);
  final rise = m[1] == 'sunrise';
  final offset = int.parse(m[2] ?? '0');
  if (offset == 0) return at ? (rise ? l.atSunrise : l.atSunset) : (rise ? l.sunrise : l.sunset);
  final event = rise ? l.sunrise : l.sunset;
  return offset < 0 ? l.sunBefore(-offset, event) : l.sunAfter(offset, event);
}

/// The test of a `when` or a `state`: in the config (`becomes = true`), or as the core describes
/// it (`test: {is: true}`).
Map<String, dynamic> _test(Map<String, dynamic> m) => ((m['test'] as Map?) ?? m).cast<String, dynamic>();

const allDays = ['mon', 'tue', 'wed', 'thu', 'fri', 'sat', 'sun'];

String triggerText(BuildContext context, CoreClient core, Map<String, dynamic> t) {
  final l = AppLocalizations.of(context);
  if (t['at'] case final String at) {
    final days = _days(context, t['days'] as List? ?? allDays);
    final sun = sunText(context, at, at: true);
    return sun == null ? l.atTime(days, at) : l.atSun(days, sun);
  }
  final when = (t['when'] as Map).cast<String, dynamic>();
  final (target, cap) = (targetText(context, when['select'] as String), when['cap'] as String);
  final test = _test(when);
  final label = capLabel(context, cap);
  if (test['becomes'] ?? test['is'] case final Object value) return l.whenIs(target, label, valueText(context, cap, value));
  if (test['above'] case final num x) return l.whenAbove(target, label, valueText(context, cap, x));
  if (test['below'] case final num x) return l.whenBelow(target, label, valueText(context, cap, x));
  return l.whenChanges(target, label);
}

/// `triggers`: the automation's, for a `trigger` condition (which one started the run). `bare`:
/// without "If", after "Wait until" or "Until".
String conditionText(BuildContext context, CoreClient core, Map<String, dynamic> c,
    {List<Map<String, dynamic>> triggers = const [], bool bare = false}) {
  final l = AppLocalizations.of(context);
  if (c['trigger'] case final String id) {
    final t = triggers.where((t) => t['id'] == id).firstOrNull;
    return l.ifTrigger(t == null ? id : triggerText(context, core, t));
  }
  if (c['between'] case [final String from, final String to]) {
    return l.ifBetween(sunText(context, from, at: false) ?? from, sunText(context, to, at: false) ?? to);
  }
  if (c['days'] case final List days) return _days(context, days);
  final state = (c['state'] as Map).cast<String, dynamic>();
  final (target, cap) = (targetText(context, state['select'] as String), state['cap'] as String);
  final test = _test(state);
  final label = capLabel(context, cap);
  final text = test.containsKey('is')
      ? (bare ? l.stateIs : l.ifIs)(target, label, valueText(context, cap, test['is']))
      : test['above'] is num
          ? (bare ? l.stateAbove : l.ifAbove)(target, label, valueText(context, cap, test['above']))
          : (bare ? l.stateBelow : l.ifBelow)(target, label, valueText(context, cap, test['below']));
  return state['all'] == true ? l.ifAll(text) : text;
}

String actionText(BuildContext context, CoreClient core, Map<String, dynamic> a) {
  final l = AppLocalizations.of(context);
  if (a['set'] case final Map set) {
    final cap = set['cap'] as String;
    final value = set['from'] == 'trigger' ? l.valueFromTrigger : valueText(context, cap, set['value']);
    return l.doSet(targetText(context, set['select'] as String), capLabel(context, cap), value);
  }
  final invoke = (a['invoke'] as Map).cast<String, dynamic>();
  // "Launch notepad": the parameters say what.
  final what = ((invoke['params'] as Map?)?.values ?? const []).where((v) => v is String || v is num).join(', ');
  final action = actionLabel(context, invoke['action'] as String);
  return l.doInvoke(what.isEmpty ? action : '$action $what', targetText(context, invoke['select'] as String));
}

/// The words of the common actions; others humanized.
String actionLabel(BuildContext context, String name) {
  final l = AppLocalizations.of(context);
  return switch (name) {
    'play_pause' => l.playPause,
    'next' => l.next,
    'previous' => l.previous,
    'lock' => l.powerLock,
    'sleep' => l.powerSleep,
    'shutdown' => l.powerShutdown,
    _ => name.isEmpty ? name : name[0].toUpperCase() + name.substring(1).replaceAll('_', ' '),
  };
}

/// `1h30m` in words: `1 h 30 min`.
String durationText(String text) {
  final seconds = durationSeconds(text);
  if (seconds == null) return text;
  final (h, m, s) = (seconds ~/ 3600, seconds ~/ 60 % 60, seconds % 60);
  return [if (h > 0) '$h h', if (m > 0) '$m min', if (s > 0) '$s s'].join(' ');
}

String _conditions(BuildContext context, CoreClient core, Object? list, List<Map<String, dynamic>> triggers, {bool any = false, bool bare = false}) {
  final l = AppLocalizations.of(context);
  final texts = [for (final c in (list as List?) ?? const []) conditionText(context, core, (c as Map).cast(), triggers: triggers, bare: bare)];
  return texts.isEmpty ? '' : texts.reduce((a, b) => any ? l.conditionsOr(a, b) : l.conditionsAnd(a, b));
}

/// What a node does, in a sentence. `triggers`: the automation's.
String nodeText(BuildContext context, CoreClient core, Map<String, dynamic> node, {List<Map<String, dynamic>> triggers = const []}) {
  final l = AppLocalizations.of(context);
  Map<String, dynamic> sub(String key) => ((node[key] as Map?) ?? const {}).cast<String, dynamic>();
  switch (kindOf(node)) {
    case 'set' || 'invoke':
      return actionText(context, core, node);
    case 'scene':
      final id = node['scene'] as String;
      return l.nodeScene(core.scenes.where((s) => s.id == id).firstOrNull?.name ?? id);
    case 'delay':
      return l.nodeDelay(durationText(node['delay'] as String));
    case 'wait':
      final wait = sub('wait');
      final text = wait['trigger'] is Map
          ? l.nodeWaitFor(triggerText(context, core, (wait['trigger'] as Map).cast()))
          : l.nodeWaitUntil(_conditions(context, core, wait['until'], triggers, bare: true));
      return wait['timeout'] is String ? l.nodeAtMost(text, durationText(wait['timeout'] as String)) : text;
    case 'if':
      return _conditions(context, core, node['if'], triggers, any: node['any'] == true);
    case 'repeat':
      final r = sub('repeat');
      if (r['count'] case final int n) return l.repeatCount(n);
      if (r['while'] != null) return l.repeatWhile(_conditions(context, core, r['while'], triggers, bare: true));
      if (r['until'] != null) return l.repeatUntil(_conditions(context, core, r['until'], triggers, bare: true));
      return l.repeatEach(targetText(context, r['for_each'] as String? ?? ''));
    case 'merge':
      return node['merge'] == 'any' ? l.mergeAny : l.mergeAll;
    default:
      final error = sub('stop')['error'] as String?;
      return error == null ? l.stopOk : l.stopError(error);
  }
}

String kindLabel(BuildContext context, String kind) {
  final l = AppLocalizations.of(context);
  return switch (kind) {
    'set' => l.kindSet,
    'invoke' => l.kindInvoke,
    'scene' => l.kindScene,
    'delay' => l.kindDelay,
    'wait' => l.kindWait,
    'if' => l.kindIf,
    'repeat' => l.kindRepeat,
    'merge' => l.kindMerge,
    _ => l.kindStop,
  };
}

IconData kindIcon(String kind) => switch (kind) {
      'set' => Icons.tune,
      'invoke' => Icons.play_arrow_outlined,
      'scene' => Icons.auto_awesome_outlined,
      'delay' => Icons.hourglass_bottom,
      'wait' => Icons.pause_circle_outline,
      'if' => Icons.call_split,
      'repeat' => Icons.repeat,
      'merge' => Icons.call_merge,
      _ => Icons.stop_circle_outlined,
    };

String portLabel(BuildContext context, String port) {
  final l = AppLocalizations.of(context);
  return switch (port) {
    'error' => l.portError,
    'timeout' => l.portTimeout,
    'then' => l.portThen,
    'else' => l.portElse,
    'body' => l.portBody,
    'start' => l.flowStart,
    _ => l.portNext,
  };
}

/// What starts it, on the canvas's top card: its triggers, then its conditions.
String rootText(BuildContext context, CoreClient core, List<Map<String, dynamic>> triggers, List<Map<String, dynamic>> conditions) => [
      for (final t in triggers) triggerText(context, core, t),
      for (final c in conditions) conditionText(context, core, c, triggers: triggers),
    ].join('\n');

/// Why the core stopped a run, in words.
String stoppedText(BuildContext context, String why) {
  final l = AppLocalizations.of(context);
  return switch (why) {
    'by hand' => l.stoppedByHand,
    'started again' => l.stoppedRestarted,
    'the automation was edited' => l.stoppedEdited,
    _ => why,
  };
}

/// A run on the canvas: where it went (and failed), where it waits; the rest it did not reach.
Map<String, NodeMark> runMarks(List<RunStep> steps, [List<String> waiting = const []]) => {
      for (final s in steps) s.node: s.ok ? NodeMark.passed : NodeMark.failed,
      for (final n in waiting) n: NodeMark.waiting,
    };

