import 'package:flutter/material.dart';
import 'package:intl/intl.dart';

import '../clients.dart';
import '../l10n/app_localizations.dart';
import '../model.dart';
import '../theme.dart';
import '../widgets/model_names.dart';
import '../widgets/state.dart';
import 'common.dart';

/// The core's automations, read-only: they are written in homemaid.toml. Each row says when it
/// runs and how its last run went; its sheet puts it into words, tries it (dry run) or runs it.
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
            padding: const EdgeInsets.fromLTRB(20, 20, 20, 4),
            child: Text(l.navAutomations, style: theme.textTheme.displaySmall),
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

/// How the last run went, in the state grammar: amber acted, red failed, outline skipped,
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
    final status = !a.enabled
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
              Text(status, style: theme.textTheme.labelSmall?.copyWith(color: Hm.textDim, fontFeatures: Hm.tabular)),
            ],
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

  Future<void> _run() async {
    setState(() => _busy = true);
    try {
      await widget.core.runAutomation(widget.id);
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
                  padding: const EdgeInsets.fromLTRB(20, 0, 20, 4),
                  child: Text(a.name, style: theme.textTheme.headlineSmall),
                ),
                for (final p in a.problems) line(p, color: Hm.danger),
                SectionHeader(l.sectionWhen),
                for (final t in a.triggers) line(triggerText(context, core, t)),
                if (a.conditions.isNotEmpty) ...[
                  SectionHeader(l.sectionIf),
                  for (final c in a.conditions) line(conditionText(context, core, c)),
                ],
                SectionHeader(l.sectionThen),
                for (final act in a.actions) line(actionText(context, core, act)),
                Padding(
                  padding: const EdgeInsets.fromLTRB(20, 20, 20, 4),
                  child: Row(children: [
                    Expanded(child: OutlinedButton(onPressed: _busy ? null : _try, child: Text(l.tryIt))),
                    const SizedBox(width: 12),
                    Expanded(child: FilledButton(onPressed: _busy || !a.enabled ? null : _run, child: Text(l.runNow))),
                  ]),
                ),
                if (_dry != null) _DryRunView(dry: _dry!, core: core),
                SectionHeader(l.sectionRuns),
                if (a.runs.isEmpty) line(l.neverRan, color: Hm.textDim),
                for (final run in a.runs) _RunRow(run: run),
              ]),
            ),
          );
        },
      );
}

class _DryRunView extends StatelessWidget {
  const _DryRunView({required this.dry, required this.core});

  final DryRun dry;
  final CoreClient core;

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
  const _RunRow({required this.run});

  final Run run;

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final theme = Theme.of(context);
    final outcome = run.skipped != null
        ? '${l.runSkipped} · ${run.skipped}'
        : run.failed
            ? '${l.runFailed} · ${run.results.where((r) => !r.ok).map((r) => '${r.target}: ${r.outcome}').join(', ')}'
            : '${l.runActed} · ${run.results.map((r) => r.target).toSet().join(', ')}';
    return Padding(
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
      ]),
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
  return parts.isEmpty ? select : parts.join(' · ');
}

String _value(BuildContext context, String cap, Object? value) {
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

String triggerText(BuildContext context, CoreClient core, Map<String, dynamic> t) {
  final l = AppLocalizations.of(context);
  if (t['at'] case final String at) return l.atTime(_days(context, t['days'] as List? ?? const []), at);
  final when = (t['when'] as Map).cast<String, dynamic>();
  final (target, cap) = (targetText(context, when['select'] as String), when['cap'] as String);
  final test = (when['test'] as Map).cast<String, dynamic>();
  final label = capLabel(context, cap);
  if (test.containsKey('is')) return l.whenIs(target, label, _value(context, cap, test['is']));
  if (test['above'] case final num x) return l.whenAbove(target, label, _value(context, cap, x));
  if (test['below'] case final num x) return l.whenBelow(target, label, _value(context, cap, x));
  return l.whenChanges(target, label);
}

String conditionText(BuildContext context, CoreClient core, Map<String, dynamic> c) {
  final l = AppLocalizations.of(context);
  if (c['between'] case [final String from, final String to]) return l.ifBetween(from, to);
  if (c['days'] case final List days) return _days(context, days);
  final state = (c['state'] as Map).cast<String, dynamic>();
  final (target, cap) = (targetText(context, state['select'] as String), state['cap'] as String);
  final test = (state['test'] as Map).cast<String, dynamic>();
  final label = capLabel(context, cap);
  final text = test.containsKey('is')
      ? l.ifIs(target, label, _value(context, cap, test['is']))
      : test['above'] is num
          ? l.ifAbove(target, label, _value(context, cap, test['above']))
          : l.ifBelow(target, label, _value(context, cap, test['below']));
  return state['all'] == true ? l.ifAll(text) : text;
}

String actionText(BuildContext context, CoreClient core, Map<String, dynamic> a) {
  final l = AppLocalizations.of(context);
  if (a['set'] case final Map set) {
    final cap = set['cap'] as String;
    return l.doSet(targetText(context, set['select'] as String), capLabel(context, cap), _value(context, cap, set['value']));
  }
  final invoke = (a['invoke'] as Map).cast<String, dynamic>();
  return l.doInvoke(actionLabel(context, invoke['action'] as String), targetText(context, invoke['select'] as String));
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
