import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:homemaid/l10n/app_localizations.dart';
import 'package:homemaid/model.dart';
import 'package:homemaid/widgets/capability.dart';

Slot slot(Object? observed, {Map<String, dynamic> meta = const {}, String state = 'idle'}) =>
    Slot(meta: meta, observed: observed, state: state);

Widget app(Widget child) => MaterialApp(
      locale: const Locale('fr'),
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      supportedLocales: AppLocalizations.supportedLocales,
      home: Scaffold(body: child),
    );

Future<Object?> pump(WidgetTester tester, String kind, Slot slot, {bool enabled = true}) async {
  Object? sent;
  await tester.pumpWidget(app(CapabilityTile(kind: kind, slot: slot, enabled: enabled, onSet: (v) => sent = v)));
  return sent;
}

void main() {
  testWidgets('on_off is a switch that sends the new value', (tester) async {
    Object? sent;
    await tester.pumpWidget(app(CapabilityTile(kind: 'on_off', slot: slot({'on_off': false}), enabled: true, onSet: (v) => sent = v)));
    await tester.tap(find.byType(Switch));
    expect(sent, isTrue);
    expect(find.text('Alimentation'), findsOneWidget);
  });

  testWidgets('volume is a slider with its unit', (tester) async {
    await pump(tester, 'volume', slot({'volume': 22}, meta: {'min': 0.0, 'max': 100.0, 'unit': '%'}));
    expect(find.byType(Slider), findsOneWidget);
    expect(find.text('22 %'), findsOneWidget);
  });

  testWidgets('sensors render from their schema', (tester) async {
    await pump(tester, 'custom:cpu', slot({'custom': 12.4}, meta: {'read_only': true, 'unit': '%', 'schema': {'type': 'number'}}));
    expect(find.text('12 %'), findsOneWidget);
    await pump(tester, 'custom:active_window', slot({'custom': null}, meta: {'read_only': true, 'schema': {'type': ['string', 'null']}}));
    expect(find.text('—'), findsOneWidget);
    expect(find.text('Fenêtre active'), findsOneWidget);
    await pump(tester, 'power', slot({'power': {'watts': 4.2}}));
    expect(find.text('4.2 W'), findsOneWidget);
  });

  testWidgets('offline or read-only controls are disabled', (tester) async {
    await pump(tester, 'on_off', slot({'on_off': true}), enabled: false);
    expect(tester.widget<Switch>(find.byType(Switch)).onChanged, isNull);
  });

  testWidgets('a pending command shows a spinner', (tester) async {
    await pump(tester, 'on_off', slot({'on_off': false}, state: 'pending'));
    expect(find.byType(CircularProgressIndicator), findsOneWidget);
  });
}
