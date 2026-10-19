import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:homemaid/demo.dart';
import 'package:homemaid/l10n/app_localizations.dart';
import 'package:homemaid/model.dart';
import 'package:homemaid/screens/automation_editor.dart';
import 'package:homemaid/screens/automations.dart';
import 'package:homemaid/theme.dart';

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
    await tester.tap(find.text('Sunset').first);
    await tester.pumpAndSettle();
    expect(find.text('at sunset'), findsOneWidget, reason: 'the time point in words');
    expect(find.textContaining('Applies to:'), findsWidgets, reason: 'the action shows what it matches');

    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    expect(core.automations, hasLength(count + 1));
    final saved = core.automations.last;
    expect(saved.id, 'lumiere-au-crepuscule');
    expect(saved.config['trigger'], [{'at': 'sunset'}]);
    expect(find.text('Lumière au crépuscule'), findsOneWidget, reason: 'listed');
  });

  testWidgets('an automation is edited and deleted', (tester) async {
    final core = DemoCore();
    final evening = core.automations.firstWhere((a) => a.id == 'evening');
    await pump(tester, Builder(builder: (context) => TextButton(onPressed: () => editAutomation(context, core, evening), child: const Text('open'))));
    await tester.tap(find.text('open'));
    await tester.pumpAndSettle();
    expect(find.text('Edit automation'), findsOneWidget);
    expect(find.text('19:30'), findsOneWidget, reason: 'its trigger, as written');

    await tester.scrollUntilVisible(find.text('Add a condition'), 300, scrollable: find.byType(Scrollable).first);
    await tester.tap(find.text('Add a condition'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    final edited = core.automations.firstWhere((a) => a.id == 'evening');
    expect(edited.conditions, hasLength(2));
    expect(edited.runs, isNotEmpty, reason: 'its runs stay');

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

  testWidgets('a small phone in French: the busiest cards fit', (tester) async {
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
    final list = find.byType(Scrollable).first;
    // Down the whole list, so every card is laid out.
    for (var i = 0; i < 12; i++) {
      await tester.drag(list, const Offset(0, -300));
      await tester.pumpAndSettle();
    }
    expect(find.text('Sélecteur (avancé)'), findsOneWidget, reason: 'the hand-written selector, as text');
    expect(find.text('Enregistrer'), findsOneWidget, reason: 'the bottom bar stays');
    expect(tester.takeException(), isNull);
  });

  testWidgets('sunrise without a location says so, and the location is set from there', (tester) async {
    final core = DemoCore()..location = null;
    await pump(tester, AutomationEditor(core: core));
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
    final set = (core.automations.last.actions.single['set'] as Map);
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
    final list = find.byType(Scrollable).first;
    await tester.scrollUntilVisible(find.text('App'), 300, scrollable: list);
    await tester.drag(list, const Offset(0, -300));
    await tester.pumpAndSettle();
    await tester.tap(find.ancestor(of: find.text('App'), matching: find.byType(DropdownButtonFormField<String>)));
    await tester.pumpAndSettle();
    await tester.tap(find.text('steam').last);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    final saved = core.automations.firstWhere((a) => a.id == 'l');
    expect((saved.actions.single['invoke'] as Map)['params'], {'app': 'steam'});

    final c = tester.element(find.byType(Navigator));
    expect(actionText(c, core, saved.actions.single), 'Launch steam on PC');
  });
}
