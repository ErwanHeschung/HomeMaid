import 'dart:convert';
import 'dart:math';
import 'dart:io';
import 'dart:ui';

import 'package:flutter_test/flutter_test.dart';
import 'package:homemaid/model.dart';
import 'package:homemaid/viewport/geometry.dart';
import 'package:homemaid/viewport/models.dart';
import 'package:homemaid/viewport/room_viewport.dart';
import 'package:homemaid/viewport/rooms.dart';
import 'package:homemaid/viewport/sprites.dart';

Entity entity(String id, Map<String, Object?> caps, {String? device}) => Entity(
      id: id,
      name: id,
      available: true,
      plugin: 'x',
      device: device,
      caps: {for (final MapEntry(:key, :value) in caps.entries) key: Slot(meta: const {}, observed: value, state: 'idle')},
    );

final rooms = RoomCatalog.fromJson(jsonDecode(File('assets/rooms/rooms.json').readAsStringSync()) as Map<String, dynamic>);

void main() {
  test('the camera looks down at the front of the room', () {
    final camera = OrbitCamera(yaw: 0, pitch: 0.6);
    final front = camera.view(const V3(0, 0, 1)), back = camera.view(const V3(0, 0, -1));
    expect(front.z, lessThan(back.z), reason: 'front is nearer');
    expect(camera.view(const V3(0, 1, 0)).y, greaterThan(camera.view(V3.zero).y), reason: 'up is up');
  });

  test('models follow capabilities, not brands', () {
    final fixture = (jsonDecode(File('test/entities.json').readAsStringSync()) as List)
        .map((e) => Entity.fromJson(e as Map<String, dynamic>));
    final models = {for (final g in groupDevices(fixture)) g.entities.first.plugin: g.model};
    expect(models, {'pc': 'screen', 'mock': 'lamp'});

    final outlets = [for (final i in [1, 2, 3]) entity('o$i', {'on_off': {'on_off': false}}, device: 'strip')];
    expect(groupDevices(outlets).single.model, 'strip');
    expect(groupDevices([entity('plug', {'on_off': {'on_off': true}})]).single.model, 'plug');
    expect(groupDevices([entity('thing', {'power': {'power': {'watts': 3}}})]).single.model, 'module');
    expect(groupDevices([entity('t', {'custom:temperature': null})]).single.model, 'climate_sensor');
  });

  test('every entity of a room gets an anchor, a strip one per outlet', () {
    final entities = [
      entity('a', {'on_off': null}),
      entity('b', {'brightness': null}),
      for (final i in [1, 2, 3]) entity('s$i', {'on_off': null}, device: 'strip'),
    ];
    final scene = layoutRoom('living', groupDevices(entities), catalog: rooms);
    expect(scene.unplaced, 0);
    expect(scene.room.id, 'living_s', reason: 'the smallest room that holds them');
    expect(scene.anchors.keys.toSet(), entities.map((e) => e.id).toSet());
    expect(scene.anchors['s1']!.x, isNot(scene.anchors['s2']!.x), reason: 'outlets apart');
  });

  test('a crowded room takes a bigger size, devices never overlap on a run', () {
    final entities = [for (var i = 0; i < 16; i++) entity('p$i', {'on_off': null})];
    final scene = layoutRoom('bedroom', groupDevices(entities), catalog: rooms);
    expect(scene.room.id, isNot('bedroom_s'));
    final spots = [for (final d in scene.devices) (d.center.x * 100).round() * 100000 + (d.center.z * 100).round()];
    expect(spots.toSet(), hasLength(spots.length), reason: 'no two plugs at the same place');
  });

  test('a pendant hangs where it hides no device on the wall', () {
    final washer = Entity(id: 'w', name: 'w', available: true, plugin: 'x', model: 'washing_machine', caps: {'on_off': Slot(meta: const {}, state: 'idle')});
    final bulb = Entity(id: 'b', name: 'b', available: true, plugin: 'x', model: 'bulb', caps: {'brightness': Slot(meta: const {}, state: 'idle')});
    final scene = layoutRoom('bathroom', groupDevices([washer, bulb]), catalog: rooms);
    final hung = scene.devices.singleWhere((d) => d.hung), wall = scene.devices.singleWhere((d) => !d.hung);
    expect(overlapOf(hung, wall, rooms), 0, reason: 'not hanging in front of the washing machine');
  });

  test('room kinds come from the name, in French and English', () {
    expect(inferRoomKind('Salon'), 'living');
    expect(inferRoomKind('Kitchen'), 'kitchen');
    expect(inferRoomKind('Chambre de Léa'), 'bedroom');
    expect(inferRoomKind('Salle de bain'), 'bathroom');
    expect(inferRoomKind('Entrée'), 'entrance');
    expect(inferRoomKind('Garage'), 'generic');
    expect(inferRoomKind(null), 'generic');
  });

  test('the arc maps to the rendered views', () {
    expect(rooms.viewAt(0), 0);
    expect(rooms.viewAt(-pi / 4), 3);
    expect(rooms.viewAt(-pi), rooms.views - 1, reason: 'clamped to the arc');
    expect(rooms.minYaw, closeTo(-pi / 2, 1e-9));
    expect(OrbitCamera().yaw, closeTo(snapYaw(OrbitCamera().yaw), 1e-9), reason: 'starts on a frame');
  });

  test('callouts never overlap and stay inside the view', () {
    const size = Size(400, 300);
    final anchors = {for (var i = 0; i < 6; i++) 'e$i': Offset(i.isEven ? 120 : 280, 150 + i.toDouble())};
    final callouts = layoutCallouts(anchors, size);
    expect(callouts, hasLength(6));
    for (final c in callouts) {
      expect((Offset.zero & size).contains(c.box.topLeft) && (Offset.zero & size).contains(c.box.bottomRight - const Offset(1, 1)), isTrue);
      for (final other in callouts.where((o) => o != c)) {
        expect(c.box.overlaps(other.box), isFalse);
      }
    }
  });

  test('the orbit maps to rendered frames and settles on one', () {
    const step = 2 * pi / frames;
    expect(frameAt(0), 0);
    expect(frameAt(step * 3 + 0.01), 3);
    expect(frameAt(-step), frames - 1, reason: 'negative yaw wraps');
    expect(snapYaw(step * 2.4), closeTo(step * 2, 1e-9));
  });

  test('every archetype has renders, strips from 2 to 6 outlets', () {
    DeviceGroup group(int n, Map<String, Object?> caps) =>
        groupDevices([for (var i = 0; i < n; i++) entity('e$i', caps, device: 'd')]).single;
    expect(spriteName(group(3, {'on_off': null})), 'strip_3');
    expect(spriteName(group(7, {'on_off': null})), 'strip_6', reason: 'longer strips show as 6 outlets');
    expect(spriteName(group(1, {'brightness': null})), 'lamp');
  });

  test("the user's choice wins over inference, except a strip of one outlet", () {
    Entity chosen(String model, Map<String, Object?> caps) => Entity(
          id: model,
          name: model,
          available: true,
          plugin: 'x',
          model: model,
          caps: {for (final k in caps.keys) k: Slot(meta: const {}, state: 'idle')},
        );
    expect(groupDevices([chosen('kettle', {'on_off': null})]).single.model, 'kettle');
    expect(groupDevices([chosen('strip', {'on_off': null})]).single.model, 'plug');
    expect(groupDevices([chosen('unknown-model', {'on_off': null})]).single.model, 'plug');
    for (final id in choosableModels) {
      expect(rooms.bounds[id], isNotNull, reason: '$id has renders and bounds');
      expect(catalog[id]!.mounts, isNotEmpty, reason: id);
    }
  });

  test('callouts go first to what needs attention', () {
    final calm = entity('calm', {'on_off': {'on_off': false}});
    final lit = entity('lit', {'on_off': {'on_off': true}});
    final failed = Entity(
      id: 'failed',
      name: 'failed',
      available: true,
      plugin: 'x',
      caps: {'on_off': Slot(meta: const {}, observed: {'on_off': false}, state: 'failed')},
    );
    expect(calloutPriority([calm, lit, failed], null), ['failed', 'lit', 'calm']);
    expect(calloutPriority([calm, lit, failed], 'calm').first, 'calm');
  });
}
