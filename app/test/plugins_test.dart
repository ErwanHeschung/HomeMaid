import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:homemaid/demo.dart';
import 'package:homemaid/model.dart';
import 'package:homemaid/screens/plugins.dart';

import 'automation_editor_test.dart' show pump;

void main() {
  test('plugins parse as the core sends them (api.rs)', () {
    final installed = InstalledPlugin.fromJson({
      'id': 'tplink', 'name': 'TP-Link Tapo', 'description': null, 'source': 'registry', 'version': '0.1.0', 'error': null,
      'permissions': ['local-network'], 'settings': {'hosts': '192.168.1.50'}, 'secrets': ['password'],
      'declared': [{'key': 'password', 'label': 'Password', 'secret': true, 'required': true}],
    });
    expect(installed.fromRegistry, isTrue);
    expect(installed.declared.single.secret, isTrue);
    final devices = InstalledPlugin.fromJson({
      'id': 'tplink', 'name': 'TP-Link Tapo', 'source': 'registry', 'declared': [],
      'device_settings': [{'key': 'host', 'label': 'IP', 'required': true}],
      'devices': [{'id': 'a1b2c3d4', 'settings': {'host': '192.168.1.89'}, 'secrets': ['password'], 'error': 'no answer'}],
    });
    expect(devices.deviceSettings.single.key, 'host');
    final strip = devices.devices.single;
    expect((strip.settings['host'], strip.error), ('192.168.1.89', 'no answer'));
    expect(strip.secrets, ['password']);
    final offered = RegistryPlugin.fromJson({
      'id': 'future', 'name': 'Future', 'description': '', 'repo': '', 'settings': [], 'version': null, 'permissions': null,
      'installed': null, 'update': false,
    });
    expect(offered.version, isNull, reason: 'needs a newer HomeMaid');
  });

  testWidgets('a plugin is installed once its required settings are filled in', (tester) async {
    final core = DemoCore();
    await pump(tester, PluginsScreen(core: core));
    expect(find.text('TP-Link Tapo'), findsWidgets);
    expect(find.text('Update to 0.2.0'), findsOneWidget);
    expect(find.text('Needs a newer HomeMaid'), findsOneWidget);

    await tester.tap(find.widgetWithText(FilledButton, 'Install'));
    await tester.pumpAndSettle();
    expect(find.text('Talk to devices on your home network'), findsOneWidget, reason: 'its permissions, in words');
    await tester.tap(find.widgetWithText(FilledButton, 'Install').last);
    await tester.pumpAndSettle();
    expect(find.textContaining('is required'), findsOneWidget, reason: 'refused, and the sheet stays');

    await tester.enterText(find.byType(TextField), 'AA:BB:CC:DD:EE:FF');
    await tester.tap(find.widgetWithText(FilledButton, 'Install').last);
    await tester.pumpAndSettle();
    expect(core.plugins.map((p) => p.id), contains('wake-on-lan'));
    expect(core.plugins.firstWhere((p) => p.id == 'wake-on-lan').settings['macs'], 'AA:BB:CC:DD:EE:FF');
    await tester.pump(const Duration(seconds: 1)); // the registry, read again
    expect(find.widgetWithText(FilledButton, 'Install'), findsNothing, reason: 'no longer offered');
  });
}
