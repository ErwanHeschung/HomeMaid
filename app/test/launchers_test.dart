import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:homemaid/demo.dart';
import 'package:homemaid/screens/launchers.dart';

import 'automation_editor_test.dart' show pump;

void main() {
  test('every site of the catalogue has its logo and a web address', () async {
    TestWidgetsFlutterBinding.ensureInitialized();
    final catalog = await loadCatalog();
    expect(catalog, isNotEmpty);
    for (final site in catalog) {
      expect(File('assets/logos/${site.icon}.svg').existsSync(), isTrue, reason: site.name);
      expect(isWebAddress(site.open!), isTrue, reason: site.name);
    }
    expect(isWebAddress('https://app.plex.tv/desktop'), isTrue);
    for (final bad in ['C:\\Windows\\cmd.exe', 'notepad', 'file:///etc/passwd', 'https://localhost', '']) {
      expect(isWebAddress(bad), isFalse, reason: bad);
    }
  });

  testWidgets('sites are added from the catalogue or by address, never a program', (tester) async {
    final agent = demoAgent();
    await pump(tester, LauncherSheet(agent: agent));
    // The catalogue is an asset, read for real.
    await tester.runAsync(() => Future<void>.delayed(const Duration(milliseconds: 200)));
    await tester.pumpAndSettle();
    expect(find.text('Added'), findsNWidgets(5), reason: 'the demo PC already has five');

    await tester.tap(find.text('Plex'));
    await tester.pumpAndSettle();
    expect(agent.shortcuts.map((s) => s.name), contains('Plex'));

    final list = find.byType(Scrollable).first;
    await tester.scrollUntilVisible(find.text('Add the site'), 300, scrollable: list);
    await tester.enterText(find.widgetWithText(TextField, 'Name'), 'Arte');
    await tester.enterText(find.widgetWithText(TextField, 'Address'), 'C:\\evil.exe');
    await tester.tap(find.text('Add the site'));
    await tester.pumpAndSettle();
    expect(find.textContaining('a web address'), findsOneWidget);
    await tester.enterText(find.widgetWithText(TextField, 'Address'), 'https://www.arte.tv');
    await tester.tap(find.text('Add the site'));
    await tester.pumpAndSettle();
    expect(agent.shortcuts.last.open, 'https://www.arte.tv');
  });

  testWidgets("the PC's apps are searched, launched and pinned", (tester) async {
    final agent = demoAgent();
    await pump(tester, LauncherSheet(agent: agent));
    await tester.tap(find.text('Apps'));
    await tester.pumpAndSettle();
    await tester.enterText(find.widgetWithText(TextField, 'Search apps'), 'ste');
    await tester.pumpAndSettle();
    expect(find.text('Steam'), findsOneWidget);
    expect(find.text('Notepad'), findsNothing);
    await tester.tap(find.byTooltip('Pin Steam'));
    await tester.pumpAndSettle();
    expect(agent.apps, contains('Steam'));
    await tester.tap(find.byTooltip('Unpin Steam'));
    await tester.pumpAndSettle();
    expect(agent.apps, isNot(contains('Steam')));
  });

  testWidgets('removing from the strip asks first', (tester) async {
    final agent = demoAgent();
    await pump(tester, Builder(builder: (context) => TextButton(onPressed: () => removeLauncher(context, agent, 'Netflix', site: true), child: const Text('remove'))));
    await tester.tap(find.text('remove'));
    await tester.pumpAndSettle();
    expect(find.text('Remove \u201cNetflix\u201d from the remote?'), findsOneWidget);
    await tester.tap(find.widgetWithText(FilledButton, 'Remove'));
    await tester.pumpAndSettle();
    expect(agent.shortcuts.map((s) => s.name), isNot(contains('Netflix')));
  });
}
