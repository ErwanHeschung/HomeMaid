/// Turns a room's entities into models set in a furnished room. The model comes from the user's
/// choice (`entity.model`, saved in homemaid.toml) or, by default, from what the device can do:
/// never from a brand, so a new plugin is modeled too.
library;

import '../model.dart';
import 'geometry.dart';

/// Where a model can go in a room, in order of preference, and at which height: for a wall, its
/// base above the floor; for a surface or a window, above the top or the sill; for a door, above
/// the floor. Floor spots and pendants take no height.
typedef MountPref = (String kind, double y);

/// A model of the catalog: its size on screen in a room ([scale]: furniture is life-size, devices
/// are enlarged to stay readable, like a diorama), where it goes, and where its callout attaches
/// (relative to its origin, unscaled).
class ModelSpec {
  const ModelSpec(this.anchor, this.scale, this.mounts);

  final V3 anchor;
  final double scale;
  final List<MountPref> mounts;
}

const _floor = ('floor', 0.0), _surface = ('surface', 0.0), _pendant = ('pendant', 0.0);

/// Every model the app can draw; tools/render/render.py renders the same names.
const catalog = <String, ModelSpec>{
  'strip': ModelSpec(V3(0, 0.25, 0.1), 0.55, [('wall', 0), _floor]), // anchors by outlet
  'plug': ModelSpec(V3(0, 0.73, 0.1), 0.45, [('wall', 0.05), _floor]),
  'lamp': ModelSpec(V3(0, 1.2, 0), 0.5, [_surface, _floor]),
  'screen': ModelSpec(V3(0, 1.5, 0), 0.55, [_surface, _floor]),
  'module': ModelSpec(V3(0, 0.24, 0), 0.55, [_surface, ('wall', 0), _floor]),
  'bulb': ModelSpec(V3(0, 0.86, 0), 0.55, [_pendant, _surface]),
  'ceiling_light': ModelSpec(V3(0, 0.23, 0), 0.7, [_pendant]),
  'light_strip': ModelSpec(V3(0.5, 0.07, 0), 0.6, [('wall', 1.3), _surface]),
  'floor_lamp': ModelSpec(V3(0, 1.52, 0), 0.95, [_floor, ('wall', 0)]),
  'thermostat': ModelSpec(V3(0, 0.66, 0), 0.4, [('wall', 1.0), _surface]),
  'radiator': ModelSpec(V3(0, 0.8, 0), 0.7, [('wall', 0.12), _floor]),
  'fan': ModelSpec(V3(0, 1.37, 0), 0.8, [_floor, ('wall', 0)]),
  'air_purifier': ModelSpec(V3(0, 0.98, 0), 0.75, [_floor, ('wall', 0)]),
  'air_conditioner': ModelSpec(V3(0, 1.08, 0), 0.6, [('wall', 1.35)]),
  'tv': ModelSpec(V3(0, 1.32, 0), 0.8, [_surface, ('wall', 0.7)]),
  'speaker': ModelSpec(V3(0, 0.63, 0), 0.55, [_surface, _floor]),
  'camera': ModelSpec(V3(0, 0.59, 0), 0.5, [('wall', 1.45), _surface]),
  // On a door: the lock at handle height, the contact at the top of the leaf.
  'door_lock': ModelSpec(V3(0, 0.78, 0), 0.6, [('door', 0.75), ('wall', 0.75)]),
  'contact_sensor': ModelSpec(V3(0, 0.48, 0), 0.6, [('door', 1.65), ('window', 0.3), ('wall', 1.0)]),
  'motion_sensor': ModelSpec(V3(0, 0.37, 0), 0.5, [('wall', 1.6), _surface]),
  'smoke_detector': ModelSpec(V3(0, 0.17, 0), 0.45, [('wall', 1.9)]),
  'doorbell': ModelSpec(V3(0, 0.64, 0), 0.6, [('wall', 0.9)]),
  'climate_sensor': ModelSpec(V3(0, 0.46, 0), 0.45, [_surface, ('wall', 1.2)]),
  'kettle': ModelSpec(V3(0, 0.62, 0), 0.5, [_surface, _floor]),
  'coffee_maker': ModelSpec(V3(0, 0.9, 0), 0.5, [_surface, _floor]),
  'washing_machine': ModelSpec(V3(0, 1.16, 0), 0.75, [('wall', 0), _floor]),
  'fridge': ModelSpec(V3(0, 1.52, 0), 1.1, [('wall', 0), _floor]),
  'robot_vacuum': ModelSpec(V3(0, 0.17, 0.15), 0.55, [_floor]),
  'shutter': ModelSpec(V3(0, 1.52, 0), 0.8, [('window', -0.1), ('wall', 0.5)]),
};

/// Models the user can pick in the app (the strip follows its outlet count, so it is inferred).
final choosableModels = [for (final id in catalog.keys) if (id != 'strip') id];

/// What the capabilities suggest when the user chose nothing.
String inferredModel(List<Entity> entities) {
  final kinds = {for (final e in entities) ...e.caps.keys};
  if (kinds.contains('brightness')) return 'lamp';
  if (kinds.contains('volume')) return 'screen';
  if (entities.every((e) => e.caps.containsKey('on_off'))) return entities.length > 1 ? 'strip' : 'plug';
  if (kinds.any((k) => k.contains('temperature') || k.contains('humidity'))) return 'climate_sensor';
  return 'module';
}

/// Entities of one physical device (same `device`), in a stable order.
class DeviceGroup {
  DeviceGroup(this.entities) : model = _modelOf(entities);

  final List<Entity> entities;

  /// A [catalog] key.
  final String model;

  static String _modelOf(List<Entity> entities) {
    final chosen = entities.map((e) => e.model).nonNulls.where(catalog.containsKey).firstOrNull;
    // A strip only makes sense for several outlets.
    if (chosen != null && (chosen != 'strip' || entities.length > 1)) return chosen;
    return inferredModel(entities);
  }

  ModelSpec get spec => catalog[model]!;

  /// Where outlet [i] of a strip sits on it, unscaled; the strip is 0.62 a socket plus 0.5 long.
  static V3 outlet(int outlets, int i) => V3(-(0.62 * outlets + 0.5) / 2 + 0.45 + i * 0.62, 0.25, 0.1);
}

List<DeviceGroup> groupDevices(Iterable<Entity> entities) {
  final groups = <String, List<Entity>>{};
  for (final e in entities) {
    groups.putIfAbsent(e.device ?? e.id, () => []).add(e);
  }
  final list = [
    for (final members in groups.values) DeviceGroup(members..sort((a, b) => a.name.compareTo(b.name))),
  ];
  list.sort((a, b) => a.entities.first.name.toLowerCase().compareTo(b.entities.first.name.toLowerCase()));
  return list;
}
