import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:homemaid/demo.dart';
import 'package:homemaid/flow.dart';
import 'package:homemaid/l10n/app_localizations.dart';
import 'package:homemaid/model.dart';
import 'package:homemaid/screens/automation_editor.dart';
import 'package:homemaid/screens/automations.dart';
import 'package:homemaid/theme.dart';
import 'package:homemaid/widgets/flow_canvas.dart';

Future<void> pump(WidgetTester tester, Widget child, {String locale = 'en', double width = 1080}) async {
  tester.view.physicalSize = Size(width, 2400);
  tester.view.devicePixelRatio = 2.5;
  addTearDown(tester.view.reset);
  await tester.pumpWidget(MaterialApp(
    theme: Hm.theme(),
    locale: Locale(locale),
    localizationsDelegates: const [
      AppLocalizations.delegate,
      GlobalMaterialLocalizations.delegate,
      GlobalWidgetsLocalizations.delegate,
      GlobalCupertinoLocalizations.delegate,
    ],
    supportedLocales: AppLocalizations.supportedLocales,
    home: Scaffold(body: child),
  ));
  await tester.pumpAndSettle();
}


/// Closes the sheet on top, as a tap above it would.
Future<void> closeSheet(WidgetTester tester) async {
  Navigator.of(tester.element(find.byType(BottomSheet).last)).pop();
  await tester.pumpAndSettle();
}

void main() {
  test('a new automation gets its id from its name', () {
    expect(slug('Le café du matin !'), 'le-cafe-du-matin');
    expect(slug('  Lumière   au crépuscule '), 'lumiere-au-crepuscule');
    expect(slug('!!!'), '');
  });

  testWidgets('a sunset automation is made from the + of the list', (tester) async {
    final core = DemoCore();
    final count = core.automations.length;
    await pump(tester, AutomationsScreen(core: core, onPair: () {}));
    await tester.tap(find.byTooltip('New automation'));
    await tester.pumpAndSettle();

    await tester.enterText(find.widgetWithText(TextField, 'Name'), 'Lumière au crépuscule');
    await tester.tap(find.text('Every day at 19:00'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Sunset').first);
    await tester.pumpAndSettle();
    expect(find.text('at sunset'), findsWidgets, reason: 'the time point in words');
    await closeSheet(tester);
    expect(find.text('Every day at sunset'), findsOneWidget, reason: 'the start card follows');

    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    expect(core.automations, hasLength(count + 1));
    final saved = core.automations.last;
    expect(saved.id, 'lumiere-au-crepuscule');
    expect(saved.config['trigger'], [{'at': 'sunset'}]);
    expect(saved.config['start'], ['set-1']);
    expect(find.text('Lumière au crépuscule'), findsOneWidget, reason: 'listed');
  });

  testWidgets('a step is added after another from its +, and opens on its settings', (tester) async {
    final core = DemoCore();
    await pump(tester, AutomationEditor(core: core));
    expect(find.text('If triggered again while waiting'), findsNothing, reason: 'nothing waits: no question');
    await tester.tap(find.bySemanticsLabel('next'));
    await tester.pumpAndSettle();
    await tester.tap(find.widgetWithText(ListTile, 'Pause'));
    await tester.pumpAndSettle();
    expect(find.text('Duration · 5 min'), findsOneWidget, reason: 'its settings, at once');
    await tester.tap(find.widgetWithText(ChoiceChip, '1 h'));
    await tester.pumpAndSettle();
    await closeSheet(tester);
    expect(find.text('Wait 1 h'), findsOneWidget, reason: 'on the canvas');
    expect(find.text('If triggered again while waiting'), findsOneWidget, reason: 'it can wait now: the question appears');

    // A branch beside it, from the same port.
    await tester.tap(find.bySemanticsLabel('next').first);
    await tester.pumpAndSettle();
    await tester.tap(find.text('At the same time as what follows'));
    await tester.pumpAndSettle();
    await tester.tap(find.widgetWithText(ListTile, 'Stop'));
    await tester.pumpAndSettle();
    await closeSheet(tester);

    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    final graph = FlowGraph.of(core.automations.last.config);
    expect(graph.targets('set-1', 'next'), ['delay-1', 'stop-1']);
    expect(graph.node('delay-1')!['delay'], '1h');
  });

  testWidgets('a step is removed from its settings, what followed takes its place', (tester) async {
    final core = DemoCore();
    final leaving = core.automations.firstWhere((a) => a.id == 'leaving');
    await pump(tester, AutomationEditor(core: core, automation: leaving));
    expect(find.text('When every branch is done'), findsOneWidget, reason: 'the merge of its two branches');
    await tester.tap(find.text('When every branch is done'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Remove this step'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    final graph = FlowGraph.of(core.automations.firstWhere((a) => a.id == 'leaving').config);
    expect(graph.node('join'), isNull);
    expect(graph.targets('lights', 'next'), ['heater'], reason: 'the first branch goes on');
    expect(graph.targets('pc', 'next'), isEmpty);
  });

  testWidgets('an automation is edited and deleted', (tester) async {
    final core = DemoCore();
    final evening = core.automations.firstWhere((a) => a.id == 'evening');
    await pump(tester, Builder(builder: (context) => TextButton(onPressed: () => editAutomation(context, core, evening), child: const Text('open'))));
    await tester.tap(find.text('open'));
    await tester.pumpAndSettle();
    expect(find.text('Edit automation'), findsOneWidget);
    await tester.tap(find.textContaining('Weekdays at 19:30'));
    await tester.pumpAndSettle();
    expect(find.text('19:30'), findsOneWidget, reason: 'its trigger, as written');

    await tester.scrollUntilVisible(find.text('Add a condition'), 300, scrollable: find.byType(Scrollable).last);
    await tester.tap(find.text('Add a condition'));
    await tester.pumpAndSettle();
    await closeSheet(tester);
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    final edited = core.automations.firstWhere((a) => a.id == 'evening');
    expect(edited.conditions, hasLength(2));
    expect(edited.runs, isNotEmpty, reason: 'its runs stay');
    expect(edited.config['node'], hasLength(1), reason: 'its action, saved as a node');

    await tester.tap(find.text('open'));
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Delete'));
    await tester.pumpAndSettle();
    await tester.tap(find.widgetWithText(FilledButton, 'Delete'));
    await tester.pumpAndSettle();
    expect(core.automations.where((a) => a.id == 'evening'), isEmpty);
  });

  testWidgets("the core's refusal shows in the form, nothing lost", (tester) async {
    final core = DemoCore();
    final broken = Automation(id: 'broken', name: 'Broken', enabled: true, triggers: [{'at': '07:00'}]);
    await pump(tester, AutomationEditor(core: core, automation: broken));
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    expect(find.textContaining('needs at least one'), findsOneWidget);
    expect(find.text('Edit automation'), findsOneWidget, reason: 'still open');
  });

  testWidgets('a small phone in French: the busiest flow fits', (tester) async {
    final core = DemoCore();
    final busy = Automation(
      id: 'busy',
      name: 'Occupée',
      enabled: true,
      triggers: [
        {'when': {'select': 'cap:brightness', 'cap': 'brightness', 'above': 50}},
        {'at': 'sunrise-45', 'days': ['mon', 'wed']},
      ],
      conditions: [
        {'between': ['sunset', '23:30']},
        {'state': {'select': 'cap:on_off', 'cap': 'on_off', 'is': false, 'all': true}},
        {'days': ['sat', 'sun']},
      ],
      actions: [
        {'set': {'select': 'cap:brightness', 'cap': 'brightness', 'value': 30}},
        {'invoke': {'select': 'plugin:pc', 'action': 'play_pause'}},
        {'set': {'select': 'tag:night | room:Cave', 'cap': 'on_off', 'value': true}},
      ],
    );
    await pump(tester, AutomationEditor(core: core, automation: busy), locale: 'fr', width: 900);
    expect(tester.takeException(), isNull);
    await tester.tap(find.textContaining('room:Cave'));
    await tester.pumpAndSettle();
    expect(find.text('Sélecteur (avancé)'), findsOneWidget, reason: 'the hand-written selector, as text');
    await closeSheet(tester);
    final leaving = core.automations.firstWhere((a) => a.id == 'leaving');
    await pump(tester, AutomationEditor(core: core, automation: leaving), locale: 'fr', width: 900);
    expect(find.text('Enregistrer'), findsOneWidget, reason: 'the bottom bar stays');
    expect(tester.takeException(), isNull);
  });

  testWidgets('sunrise without a location says so, and the location is set from there', (tester) async {
    final core = DemoCore()..location = null;
    await pump(tester, AutomationEditor(core: core));
    await tester.tap(find.text('Every day at 19:00'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Sunset').first);
    await tester.pumpAndSettle();
    expect(find.text("Sunrise and sunset need the home's location."), findsOneWidget);

    await tester.tap(find.text('Set the location'));
    await tester.pumpAndSettle();
    await tester.enterText(find.widgetWithText(TextField, 'Latitude'), '95');
    await tester.enterText(find.widgetWithText(TextField, 'Longitude'), '2,35');
    await tester.tap(find.widgetWithText(FilledButton, 'Save').last);
    await tester.pumpAndSettle();
    expect(find.textContaining('Latitude -90 to 90'), findsOneWidget, reason: 'checked before sending');
    await tester.enterText(find.widgetWithText(TextField, 'Latitude'), '48,86');
    await tester.tap(find.widgetWithText(FilledButton, 'Save').last);
    await tester.pumpAndSettle();
    expect(core.location, (latitude: 48.86, longitude: 2.35));
    expect(find.text("Sunrise and sunset need the home's location."), findsNothing);
  });

  testWidgets('a home without switches gets an action it can do', (tester) async {
    final core = DemoCore();
    core.entities.removeWhere((id, e) => id != 'demo-pc');
    await pump(tester, AutomationEditor(core: core));
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    final set = FlowGraph.of(core.automations.last.config).nodes.single['set'] as Map;
    expect(set['cap'], isNot('on_off'), reason: 'the PC has no switch');
    expect(core.entities['demo-pc']!.caps.containsKey(set['cap']), isTrue);
  });

  testWidgets("an action's choices come as a menu, and say what they launch", (tester) async {
    final core = DemoCore();
    final pc = core.entities['demo-pc']!;
    core.entities['demo-pc'] = Entity(
      id: pc.id, name: pc.name, room: pc.room, available: true, plugin: 'pc', caps: pc.caps,
      actions: [
        const ActionDecl(name: 'launch', params: {
          'type': 'object',
          'properties': {'app': {'type': 'string', 'enum': ['notepad', 'steam']}},
        }),
      ],
    );
    final launch = Automation(id: 'l', name: 'Launch', enabled: true, triggers: [{'at': '07:00'}], actions: [
      {'invoke': {'select': 'plugin:pc', 'action': 'launch', 'params': {'app': 'Netflix'}}},
    ]);
    await pump(tester, AutomationEditor(core: core, automation: launch));
    await tester.tap(find.text('Launch Netflix on PC'));
    await tester.pumpAndSettle();
    await tester.tap(find.ancestor(of: find.text('App'), matching: find.byType(DropdownButtonFormField<String>)));
    await tester.pumpAndSettle();
    await tester.tap(find.text('steam').last);
    await tester.pumpAndSettle();
    await closeSheet(tester);
    expect(find.text('Launch steam on PC'), findsOneWidget, reason: 'the card says what it launches');
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    final saved = FlowGraph.of(core.automations.firstWhere((a) => a.id == 'l').config).nodes.single;
    expect((saved['invoke'] as Map)['params'], {'app': 'steam'});
  });

  testWidgets('a run waiting shows where, is stopped, and its path lights the flow', (tester) async {
    final core = DemoCore();
    await pump(tester, AutomationSheet(core: core, id: 'living-evening'));
    expect(find.textContaining('Running since'), findsOneWidget);
    expect(find.textContaining('At: Wait until'), findsOneWidget, reason: 'the node it waits on');
    await tester.tap(find.widgetWithText(OutlinedButton, 'Stop'));
    await tester.pumpAndSettle();
    expect(find.textContaining('Running since'), findsNothing);
    expect(find.text('Stopped · by hand'), findsOneWidget);

    await tester.tap(find.text('Stopped · by hand'));
    await tester.pumpAndSettle();
    expect(find.byType(FlowViewer), findsOneWidget);
    final marks = tester.widget<FlowCanvas>(find.byType(FlowCanvas)).marks!;
    expect(marks['on'], NodeMark.passed);
    expect(marks.containsKey('off'), isFalse, reason: 'not reached: a ghost');
  });
}
