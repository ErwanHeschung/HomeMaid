import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:homemaid/model.dart';

void main() {
  // Real `entities.list` output of the core (pc + mock plugins).
  final entities = (jsonDecode(File('test/entities.json').readAsStringSync()) as List)
      .map((e) => Entity.fromJson(e as Map<String, dynamic>))
      .toList();

  test('parses the core output', () {
    final pc = entities.firstWhere((e) => e.plugin == 'pc');
    expect(pc.available, isTrue);
    expect(pc.caps['volume']!.max, 100);
    expect(pc.caps['volume']!.shown, isA<int>());
    expect(pc.caps['custom:cpu']!.readOnly, isTrue);
    expect(pc.caps['custom:cpu']!.schema, {'type': 'number'});
    final mock = entities.firstWhere((e) => e.plugin == 'mock');
    expect(mock.caps['power']!.shown, isA<Map>(), reason: '{"watts": ...}');
  });

  test('controls come before sensors', () {
    final pc = entities.firstWhere((e) => e.plugin == 'pc');
    expect(pc.orderedCaps.map((c) => c.key), ['mute', 'volume', 'custom:active_window', 'custom:cpu', 'custom:memory']);
  });

  test('a pending command shows its desired value, a failed one keeps its reason', () {
    final pending = Slot.fromJson({
      'meta': {},
      'observed': {'on_off': false},
      'desired': {'on_off': true},
      'status': {'state': 'pending', 'attempts': 1},
    });
    expect(pending.shown, isTrue);
    final failed = Slot.fromJson({
      'meta': {},
      'observed': {'on_off': false},
      'desired': {'on_off': true},
      'status': {'state': 'failed', 'reason': 'not confirmed after 5 attempts'},
    });
    expect(failed.shown, isFalse, reason: 'the device state, not the wish');
    expect(failed.reason, contains('5 attempts'));
  });

  test('values for the core use the capability key, custom ones share "custom"', () {
    expect(capValue('volume', 30), {'volume': 30});
    expect(capValue('custom:fan', 2), {'custom': 2});
  });

  test('rooms sorted, entities without a room last', () {
    Entity entity(String name, String? room) =>
        Entity(id: name, name: name, room: room, available: true, plugin: 'x', caps: const {});
    final groups = byRoom([entity('b', null), entity('z', 'Salon'), entity('a', 'Bureau'), entity('y', 'Salon')]);
    expect(groups.map((g) => g.key), ['Bureau', 'Salon', null]);
    expect(groups[1].value.map((e) => e.name), ['y', 'z']);
  });
}
