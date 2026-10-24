import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:homemaid/demo.dart';
import 'package:homemaid/flow.dart';
import 'package:homemaid/model.dart';
import 'package:homemaid/screens/automation_editor.dart';
import 'package:homemaid/screens/home.dart';
import 'package:homemaid/viewport/rooms.dart';
import 'package:homemaid/widgets/state.dart';

import 'automation_editor_test.dart' show closeSheet, pump;

Scene scene(DemoCore core, String id) => core.scenes.firstWhere((s) => s.id == id);

void main() {
  setUpAll(() => RoomCatalog.instance =
      RoomCatalog.fromJson(jsonDecode(File('assets/rooms/rooms.json').readAsStringSync()) as Map<String, dynamic>));

  test('a scene is in force from observed states only', () {
    final core = DemoCore();
    expect(sceneState(scene(core, 'evening'), core.entities), SceneState.on);
    expect(sceneState(scene(core, 'games'), core.entities), SceneState.pending, reason: 'asked, not confirmed');
    expect(sceneState(scene(core, 'night'), core.entities), SceneState.off);

    final failing = Scene(id: 'f', name: 'F', targets: [(entity: 'x', cap: 'on_off', value: true)]);
    final entities = {
      'x': Entity(id: 'x', name: 'X', available: true, plugin: 'p', caps: {
        'on_off': Slot(meta: const {}, observed: const {'on_off': false}, desired: const {'on_off': true}, state: 'failed'),
      }),
    };
    expect(sceneState(failing, entities), SceneState.failed);
    expect(sceneState(Scene(id: 'e', name: 'E'), entities), SceneState.off, reason: 'nothing to set');
  });

  testWidgets('a scene chip puts it in force, and a new scene captures the room', (tester) async {
    final core = DemoCore();
    // Wide enough for the room's whole strip of scenes.
    await pump(tester, HomeScreen(core: core, onPair: () {}), width: 1800);
    // The Living room has the demo's scenes.
    await tester.scrollUntilVisible(find.text('Living room'), 200, scrollable: find.byType(Scrollable).first);
    await tester.tap(find.text('Living room'));
    await tester.pumpAndSettle();
    expect(find.text('Evening'), findsOneWidget);
    await tester.tap(find.text('Night'));
    await tester.pump();
    expect(sceneState(scene(core, 'night'), core.entities), SceneState.pending);
    await tester.pump(const Duration(seconds: 1));
    expect(sceneState(scene(core, 'night'), core.entities), SceneState.on, reason: 'confirmed by the devices');

    await tester.tap(find.text('Scene'));
    await tester.pumpAndSettle();
    await tester.enterText(find.widgetWithText(TextField, 'Name'), 'Cosy');
    await tester.tap(find.text('Take the current state'));
    await tester.pumpAndSettle();
    // Down the devices, to the actions.
    for (var i = 0; i < 6 && find.text('Add an action').hitTestable().evaluate().isEmpty; i++) {
      await tester.dragFrom(const Offset(200, 600), const Offset(0, -300));
      await tester.pumpAndSettle();
    }
    await tester.tap(find.text('Add an action'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    final cosy = scene(core, 'cosy');
    expect(cosy.room, 'Living room');
    final sets = (cosy.config['set'] as List).cast<Map<String, dynamic>>();
    final ceiling = sets.where((s) => s['select'] == 'id:demo-ceiling').toList();
    expect(ceiling.map((s) => (s['cap'], s['value'])), [('on_off', false)], reason: 'off, as it is now; off says all: no level sent');
    expect(sceneState(cosy, core.entities), SceneState.on, reason: 'taken from the state it is in');
    expect(cosy.config['invoke'], [{'select': 'plugin:pc', 'action': 'play_pause'}], reason: 'an action the home can run');
  });

  testWidgets('an automation step puts a scene in force', (tester) async {
    final core = DemoCore();
    await pump(tester, AutomationEditor(core: core));
    await tester.tap(find.bySemanticsLabel('next'));
    await tester.pumpAndSettle();
    await tester.tap(find.widgetWithText(ListTile, 'Scene'));
    await tester.pumpAndSettle();
    await closeSheet(tester);
    expect(find.text('Set the scene Evening'), findsOneWidget);
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    expect(FlowGraph.of(core.automations.last.config).node('scene-1')!['scene'], 'evening');
  });
}
