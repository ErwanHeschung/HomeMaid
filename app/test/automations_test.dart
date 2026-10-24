import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:homemaid/demo.dart';
import 'package:homemaid/l10n/app_localizations.dart';
import 'package:homemaid/model.dart';
import 'package:homemaid/screens/automations.dart';
import 'package:homemaid/widgets/flow_canvas.dart';

/// `automations.list` as the core sends it (crates/homemaid/src/automations.rs).
const listed = {
  'id': 'evening',
  'name': 'Lampe du soir',
  'enabled': true,
  'triggers': [
    {'at': '19:30', 'days': ['mon', 'tue', 'wed', 'thu', 'fri']},
    {'when': {'select': 'plugin:pc', 'cap': 'volume', 'test': {'above': 50}, 'all': false}},
  ],
  'conditions': [
    {'between': ['18:00', '01:00']},
    {'state': {'select': 'room:Salon & cap:on_off', 'cap': 'on_off', 'test': {'is': false}, 'all': true}},
  ],
  'actions': [
    {'set': {'select': 'room:Salon & model:lamp', 'cap': 'brightness', 'value': 40}},
    {'invoke': {'select': 'plugin:pc', 'action': 'play_pause', 'params': null}},
  ],
  'next_run': '2026-10-19T17:30:00Z',
  'problems': ['`room:Cave` matches no device'],
  'delegated': ['Lampe'],
  'runs': [
    {'at': '2026-10-18T19:30:00+02:00', 'trigger': '19:30', 'skipped': null, 'results': [
      {'entity': null, 'target': 'Lampe', 'action': 'set brightness 40', 'ok': false, 'outcome': 'not confirmed'},
    ]},
  ],
};

Future<BuildContext> localized(WidgetTester tester, String locale) async {
  late BuildContext context;
  await tester.pumpWidget(MaterialApp(
    locale: Locale(locale),
    localizationsDelegates: AppLocalizations.localizationsDelegates,
    supportedLocales: AppLocalizations.supportedLocales,
    home: Builder(builder: (c) {
      context = c;
      return const SizedBox();
    }),
  ));
  return context;
}

void main() {
  test('automations and dry runs parse from the core', () {
    final a = Automation.fromJson(listed);
    expect(a.triggers, hasLength(2));
    expect(a.nextRun!.isUtc, isFalse, reason: 'shown in local time');
    expect(a.runs.single.failed, isTrue);
    expect(a.delegated, ['Lampe']);
    final dry = DryRun.fromJson({
      'would_run': false,
      'conditions': [{'condition': {}, 'ok': false, 'reason': '12:00 is not between 18:00 and 01:00'}],
      'actions': [
        {'action': (listed['actions']! as List)[0], 'targets': [{'entity': 'x', 'name': 'Lampe', 'changes': true, 'error': null}]},
      ],
    });
    expect(dry.wouldRun, isFalse);
    expect(dry.actions.single.targets.single.name, 'Lampe');
  });

  testWidgets('automations read as sentences, in French and English', (tester) async {
    final core = DemoCore();
    final a = Automation.fromJson(listed);
    var c = await localized(tester, 'en');
    expect(triggerText(c, core, a.triggers[0]), 'Weekdays at 19:30');
    expect(triggerText(c, core, a.triggers[1]), 'When PC · Volume goes above 50 %');
    expect(conditionText(c, core, a.conditions[1]), 'If Salon · Power is Off (all of them)');
    expect(actionText(c, core, a.actions[0]), 'Set Salon · Desk lamp · Brightness to 40 %');
    expect(actionText(c, core, a.actions[1]), 'Play or pause on PC');
    c = await localized(tester, 'fr');
    expect(triggerText(c, core, a.triggers[0]), 'Du lundi au vendredi à 19:30');
    expect(conditionText(c, core, a.conditions[0]), 'Entre 18:00 et 01:00');
  });

  testWidgets('automations as written read the same as described', (tester) async {
    final core = DemoCore();
    final c = await localized(tester, 'en');
    final a = Automation.fromJson({
      'id': 'x', 'name': 'X', 'enabled': true,
      'config': {
        'id': 'x',
        'trigger': [{'when': {'select': 'plugin:pc', 'cap': 'volume', 'above': 50}}, {'at': '07:00'}],
        'condition': [{'state': {'select': 'room:Salon & cap:on_off', 'cap': 'on_off', 'is': false, 'all': true}}],
        'action': [{'set': {'select': 'room:Salon & model:lamp', 'cap': 'brightness', 'value': 40}}],
      },
    });
    expect(triggerText(c, core, a.triggers[0]), 'When PC · Volume goes above 50 %');
    expect(triggerText(c, core, a.triggers[1]), 'Every day at 07:00', reason: 'no days written: every day');
    expect(conditionText(c, core, a.conditions[0]), 'If Salon · Power is Off (all of them)');
    expect(a.config['trigger'], hasLength(2), reason: 'kept as written, to edit');
  });

  test('the demo saves, replaces and deletes automations in memory', () async {
    final core = DemoCore();
    final count = core.automations.length;
    final config = {'id': 'new', 'name': 'Nouveau', 'trigger': [{'at': 'sunset'}], 'action': [{'set': {'select': 'cap:on_off', 'cap': 'on_off', 'value': true}}]};
    await core.saveAutomation(config);
    expect(core.automations.length, count + 1);
    await core.saveAutomation({...config, 'name': 'Renommé'});
    expect(core.automations.where((a) => a.id == 'new').single.name, 'Renommé');
    expect((await core.dryRunConfig(config)).actions.single.targets, isNotEmpty);
    expect(core.saveAutomation({...config, 'action': []}), throwsA(contains('needs at least one')));
    await core.deleteAutomation('new');
    expect(core.automations.length, count);
  });

  testWidgets('sunrise and sunset read as words', (tester) async {
    final core = DemoCore();
    const all = ['mon', 'tue', 'wed', 'thu', 'fri', 'sat', 'sun'];
    var c = await localized(tester, 'en');
    expect(triggerText(c, core, {'at': 'sunset', 'days': all}), 'Every day at sunset');
    expect(triggerText(c, core, {'at': 'sunrise+15', 'days': ['sat', 'sun']}), 'Weekends 15 min after sunrise');
    c = await localized(tester, 'fr');
    expect(triggerText(c, core, {'at': 'sunset-30', 'days': all}), 'Tous les jours 30 min avant le coucher du soleil');
    expect(conditionText(c, core, {'between': ['sunset', '01:00']}), 'Entre le coucher du soleil et 01:00');
  });

  test('the demo covers every case', () {
    final list = DemoCore().automations;
    expect(list.any((a) => a.runs.firstOrNull?.skipped == null && a.runs.firstOrNull?.failed == false), isTrue, reason: 'acted');
    expect(list.any((a) => a.runs.firstOrNull?.failed ?? false), isTrue, reason: 'failed');
    expect(list.any((a) => a.runs.firstOrNull?.skipped != null), isTrue, reason: 'skipped');
    expect(list.any((a) => a.runs.isEmpty), isTrue, reason: 'never ran');
    expect(list.any((a) => a.problems.isNotEmpty), isTrue, reason: 'problem');
    expect(list.any((a) => a.delegated.isNotEmpty), isTrue, reason: 'runs without the hub');
    expect(list.any((a) => a.triggers.any((t) => t.containsKey('when'))), isTrue, reason: 'state trigger');
  });

  testWidgets('Try it shows what would happen', (tester) async {
    final core = DemoCore();
    await tester.pumpWidget(MaterialApp(
      locale: const Locale('en'),
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      supportedLocales: AppLocalizations.supportedLocales,
      home: Scaffold(body: AutomationSheet(core: core, id: 'pause-on-mute')),
    ));
    await tester.tap(find.text('Try it'));
    await tester.pump(const Duration(milliseconds: 400));
    expect(find.text('Would run now'), findsOneWidget);
    expect(find.text('DEMO-PC · to do'), findsOneWidget);
  });

  testWidgets('steps read as sentences, runs bring their path', (tester) async {
    final core = DemoCore();
    final c = await localized(tester, 'en');
    final triggers = [{'id': 'dusk', 'at': 'sunset'}];
    String text(Map<String, dynamic> node) => nodeText(c, core, node, triggers: triggers);
    expect(text({'id': 'd', 'delay': '1h30m'}), 'Wait 1 h 30 min');
    expect(
      text({'id': 'w', 'wait': {'until': [{'days': ['sat', 'sun']}], 'timeout': '2h'}}),
      'Wait until: Weekends (2 h at most)',
    );
    expect(text({'id': 'i', 'if': [{'trigger': 'dusk'}, {'days': ['mon']}], 'any': true}), 'Started by: Every day at sunset or Mon');
    expect(text({'id': 'r', 'repeat': {'count': 3}, 'body': ['d']}), '3 times');
    expect(text({'id': 'e', 'repeat': {'for_each': 'cap:on_off'}, 'body': ['d']}), 'For each: Whole home');
    expect(text({'id': 's', 'set': {'select': 'item', 'cap': 'brightness', 'from': 'trigger'}}), 'Set this device · Brightness to its new value');
    expect(text({'id': 'm', 'merge': 'any'}), 'As soon as one branch arrives');
    expect(text({'id': 'x', 'stop': {'error': 'no PC'}}), 'Fail: no PC');

    final run = Run.fromJson({
      'at': '2026-10-18T19:30:00+02:00', 'trigger': 'manual', 'skipped': null, 'error': null, 'stopped': 'by hand',
      'results': [], 'steps': [{'node': 'on', 'at': '2026-10-18T19:30:01+02:00', 'ok': true, 'outcome': 'Lampe: pending'}],
    });
    expect(run.failed, isFalse, reason: 'stopped is not failed');
    expect(runMarks(run.steps, ['wait']), {'on': NodeMark.passed, 'wait': NodeMark.waiting});
    expect(Run.fromJson({'at': '', 'trigger': '', 'error': 'too many steps', 'results': []}).failed, isTrue);
  });
}
