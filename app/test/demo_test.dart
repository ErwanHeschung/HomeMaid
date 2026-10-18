import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:homemaid/l10n/app_localizations.dart';
import 'package:homemaid/demo.dart';
import 'package:homemaid/model.dart';
import 'package:homemaid/theme.dart';
import 'package:homemaid/viewport/models.dart';
import 'package:homemaid/viewport/room_viewport.dart';
import 'package:homemaid/viewport/rooms.dart';
import 'package:homemaid/widgets/state.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  test('the demo covers every model and every state', () {
    final core = DemoCore();
    final entities = core.entities.values.toList();
    final shown = {for (final g in groupDevices(entities)) g.model};
    expect(shown, containsAll([...catalog.keys]), reason: 'every model of the catalog: ${catalog.keys.toSet().difference(shown)}');
    expect(entities.any((e) => onLevel(e) > 0 && e.caps.containsKey('on_off')), isTrue, reason: 'on');
    expect(entities.any(isPending), isTrue, reason: 'pending');
    expect(entities.any(isFailed), isTrue, reason: 'failed');
    expect(entities.any((e) => !e.available), isTrue, reason: 'offline');
    expect(entities.any((e) => e.room == null), isTrue, reason: 'no room');
    expect(byRoom(entities).any((r) => r.value.length > maxCallouts), isTrue, reason: 'past the callout cap');
    expect(core.synthetic, isTrue, reason: 'labeled as demo data');
  });

  test('every demo room sets all its devices in its furnished room', () {
    final rooms = RoomCatalog.fromJson(jsonDecode(File('assets/rooms/rooms.json').readAsStringSync()) as Map<String, dynamic>);
    for (final MapEntry(key: room, value: members) in byRoom(DemoCore().entities.values)) {
      final scene = layoutRoom(inferRoomKind(room), groupDevices(members), catalog: rooms);
      expect(scene.unplaced, 0, reason: '$room in ${scene.room.id}');
    }
  });

  test('a command goes pending, then confirmed; the stubborn device fails', () async {
    final core = DemoCore();
    final command = core.setDesired('demo-coffee', 'on_off', true);
    expect(core.entities['demo-coffee']!.caps['on_off']!.pending, isTrue);
    await command;
    expect(core.entities['demo-coffee']!.caps['on_off']!.state, 'confirmed');
    expect(core.entities['demo-coffee']!.caps['on_off']!.shown, isTrue);

    await core.setDesired('demo-garage', 'on_off', true);
    expect(core.entities['demo-garage']!.caps['on_off']!.failed, isTrue);
  });

  test('a pending wish never lights anything: on is the reported state only', () {
    final console = DemoCore().entities['demo-strip-3']!;
    expect(console.caps['on_off']!.shown, isTrue, reason: 'the wish is shown');
    expect(onLevel(console), 0, reason: 'but the device still reports off');
    expect(stateColor(console), Hm.pending);
  });

  testWidgets('every sensor says its state, the TV that it is on', (tester) async {
    late BuildContext context;
    await tester.pumpWidget(MaterialApp(
      locale: const Locale('en'),
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      supportedLocales: AppLocalizations.supportedLocales,
      home: Builder(builder: (c) => const SizedBox()).withContext((c) => context = c),
    ));
    final demo = DemoCore().entities;
    expect(summary(context, demo['demo-lock']!), startsWith('Locked'));
    expect(summary(context, demo['demo-door']!), 'Closed');
    expect(summary(context, demo['demo-motion']!), 'No motion');
    expect(summary(context, demo['demo-smoke']!), 'No smoke');
    expect(summary(context, demo['demo-tvset']!), 'On · Muted');
    expect(summary(context, demo['demo-kettle']!), 'On · 1850 W');
    for (final e in demo.values) {
      expect(summary(context, e), isNotEmpty, reason: e.name);
    }
  });
}

extension on Widget {
  Widget withContext(void Function(BuildContext) grab) => Builder(builder: (c) {
        grab(c);
        return this;
      });
}
