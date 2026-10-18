/// Furnished rooms (tools/render/room.py): a dollhouse cut-away per kind and size, rendered over a
/// 90° arc, with the places devices can go. A room's devices are set in the smallest size that
/// holds them all.
library;

import 'dart:convert';
import 'dart:math';

import 'package:flutter/services.dart';

import 'geometry.dart';
import 'models.dart';
import 'sprites.dart';

const roomKinds = ['living', 'kitchen', 'bedroom', 'office', 'bathroom', 'entrance', 'generic'];
const _sizes = ['s', 'm', 'l'];

/// The kind a room's name suggests (French and English), or `generic`.
String inferRoomKind(String? name) {
  final n = (name ?? '').toLowerCase();
  bool has(List<String> words) => words.any(n.contains);
  if (has(['salon', 'séjour', 'sejour', 'living', 'lounge'])) return 'living';
  if (has(['cuisine', 'kitchen'])) return 'kitchen';
  if (has(['chambre', 'bedroom', 'nursery'])) return 'bedroom';
  if (has(['bureau', 'office', 'study', 'atelier'])) return 'office';
  if (has(['bain', 'sdb', 'bath', 'douche', 'shower', 'toilet', 'wc', 'buanderie', 'laundry'])) return 'bathroom';
  if (has(['entrée', 'entree', 'hall', 'couloir', 'entrance', 'hallway', 'corridor'])) return 'entrance';
  return 'generic';
}

/// A place devices go: a run along a wall or a furniture top ([a] to [b]), or a spot ([a]).
class Mount {
  Mount.fromJson(Map<String, dynamic> j)
      : kind = j['mount'] as String,
        a = _point(j['a'] ?? j['at']),
        b = _point(j['b'] ?? j['at']),
        y = (j['y'] as num?)?.toDouble() ?? 0,
        minY = (j['minY'] as num?)?.toDouble() ?? 0,
        facing = ((j['facing'] as num?)?.toDouble() ?? 0) * pi / 180;

  static V3 _point(Object? xz) => V3((xz as List)[0].toDouble(), 0, xz[1].toDouble());

  final String kind;
  final V3 a, b;
  final double y, minY, facing;

  bool get isRun => kind != 'floor' && kind != 'pendant';
  double get length => (b - a).length;
}

class RoomSpec {
  RoomSpec.fromJson(this.id, Map<String, dynamic> j)
      : height = (j['h'] as num).toDouble(),
        scale = (j['scale'] as num).toDouble(),
        focus = V3(0, (j['focus'] as num).toDouble(), 0),
        aspect = (j['aspect'] as num).toDouble(),
        mounts = [for (final m in j['mounts'] as List) Mount.fromJson((m as Map).cast())];

  /// `<kind>_<size>`, the asset folder of its views.
  final String id;
  final double height;

  /// World units across the rendered image, and the point at its center.
  final double scale;
  final V3 focus;
  final double aspect;
  final List<Mount> mounts;
}

class RoomCatalog {
  RoomCatalog.fromJson(Map<String, dynamic> j)
      : views = j['views'] as int,
        step = (j['step'] as num).toDouble() * pi / 180,
        rooms = {
          for (final MapEntry(:key, :value) in (j['rooms'] as Map).cast<String, Object?>().entries)
            key: RoomSpec.fromJson(key, (value as Map).cast()),
        },
        bounds = {
          for (final MapEntry(:key, :value) in (j['models'] as Map).cast<String, Object?>().entries)
            key: [for (final v in value as List) (v as num).toDouble()],
        };

  static late RoomCatalog instance;

  static Future<void> load([AssetBundle? bundle]) async {
    instance = RoomCatalog.fromJson(jsonDecode(await (bundle ?? rootBundle).loadString('assets/rooms/rooms.json')) as Map<String, dynamic>);
  }

  /// Rendered views over the arc, [step] radians apart: view i is seen at yaw -i * step.
  final int views;
  final double step;
  final Map<String, RoomSpec> rooms;

  /// Each model's box in its own frame: x0, x1, y0, y1, z0, z1.
  final Map<String, List<double>> bounds;

  double get minYaw => -(views - 1) * step;

  int viewAt(double yaw) => (-yaw / step).round().clamp(0, views - 1);
}

String roomPath(String room, int view) => 'assets/rooms/$room/view_$view.webp';

/// A device set in the room.
class Placed {
  Placed(this.group, this.sprite, this.center, this.facing, this.scale, {this.hung = false});

  final DeviceGroup group;

  /// The render to draw ([spriteName], or its `_hung` variant for a pendant).
  final String sprite;
  final V3 center;
  final double facing, scale;

  /// Hangs from a cord up to the ceiling.
  final bool hung;

  /// A point of the model (its own frame, unscaled) in the room.
  V3 toWorld(V3 local) => center + local.scale(scale).turn(facing);
}

class RoomScene {
  RoomScene(this.room, this.devices, this.anchors, this.unplaced);

  final RoomSpec room;
  final List<Placed> devices;

  /// Where each entity's callout attaches.
  final Map<String, V3> anchors;

  /// Devices the room had no place for: listed only.
  final int unplaced;
}

/// Sets [groups] in a room of [kind]: the smallest size that holds them all, else the largest.
RoomScene layoutRoom(String kind, List<DeviceGroup> groups, {RoomCatalog? catalog}) {
  final rooms = (catalog ?? RoomCatalog.instance).rooms;
  late RoomScene scene;
  for (final size in _sizes) {
    scene = _layout(rooms['${kind}_$size'] ?? rooms['generic_$size']!, groups, catalog ?? RoomCatalog.instance);
    if (scene.unplaced == 0) break;
  }
  return scene;
}

const _gap = 0.12;

/// Devices on the floor turn to the open corner, where the camera is.
const _floorFacing = -pi / 4;

/// Pendants hang this far below the ceiling.
const _drop = 0.5;

class _Run {
  _Run(this.mount);

  final Mount mount;
  final items = <(DeviceGroup, String, double width, double y, double back, double scale)>[];
  double used = 0;

  double get free => mount.length - used;
}

RoomScene _layout(RoomSpec room, List<DeviceGroup> groups, RoomCatalog catalog) {
  final runs = [for (final m in room.mounts) if (m.isRun) _Run(m)];
  final floor = [for (final m in room.mounts) if (m.kind == 'floor') m.a];
  // Pendants: the room's own places, then over floor spots (hanging, they take no floor).
  final hooks = [for (final m in room.mounts) if (m.kind == 'pendant') m.a, ...floor];
  var nextFloor = 0;
  final devices = <Placed>[];
  // Pendants hang last, where they hide the least of what is already set (see below).
  final pendants = <DeviceGroup>[];
  var unplaced = 0;

  // The pickiest first, then the widest.
  final order = [...groups]..sort((a, b) {
      final byChoice = a.spec.mounts.length.compareTo(b.spec.mounts.length);
      return byChoice != 0 ? byChoice : _width(b, catalog).compareTo(_width(a, catalog));
    });
  for (final g in order) {
    final spec = g.spec;
    final sprite = spriteName(g);
    final box = catalog.bounds[sprite]!;
    final width = _width(g, catalog);
    var placed = false;
    for (final (kind, y) in spec.mounts) {
      if (kind == 'floor') {
        if (nextFloor >= floor.length) continue;
        devices.add(Placed(g, sprite, floor[nextFloor++], _floorFacing, spec.scale));
      } else if (kind == 'pendant') {
        if (pendants.length >= hooks.length) continue;
        pendants.add(g);
      } else {
        final top = (box[3] - box[2]) * spec.scale;
        final fits = runs.where((r) =>
            r.mount.kind == kind &&
            r.free >= width + (r.items.isEmpty ? 0 : _gap) &&
            (kind != 'wall' || (y >= r.mount.minY && y + top <= room.height - 0.05)));
        if (fits.isEmpty) continue;
        final run = fits.reduce((a, b) => b.free > a.free ? b : a);
        run.used += width + (run.items.isEmpty ? 0 : _gap);
        run.items.add((g, sprite, width, y, -box[4] * spec.scale, spec.scale));
      }
      placed = true;
      break;
    }
    if (!placed) unplaced++;
  }

  // Each run's devices side by side, the group centered on it; against a wall, backs to it.
  for (final run in runs) {
    final m = run.mount;
    final along = (m.b - m.a).scale(1 / max(m.length, 1e-6));
    final out = const V3(0, 0, 1).turn(m.facing);
    var at = (m.length - run.used) / 2;
    for (final (g, sprite, width, y, back, scale) in run.items) {
      final p = m.a + along.scale(at + width / 2);
      final onSurface = m.kind == 'surface';
      final height = switch (m.kind) { 'surface' || 'window' => m.y + y, _ => y };
      final center = onSurface ? p : p + out.scale(back + 0.02);
      devices.add(Placed(g, sprite, V3(center.x, height, center.z), m.facing, scale));
      at += width + _gap;
    }
  }

  // Seen from the open corner, a pendant rises in front of the back walls: hang each one where its
  // outline covers the least of the devices already set, the room's own places first on a tie.
  final free = [...hooks];
  for (final g in pendants) {
    final sprite = '${spriteName(g)}_hung';
    double cover(V3 hook) {
      final p = Placed(g, sprite, V3(hook.x, room.height - _drop, hook.z), 0, g.spec.scale, hung: true);
      final mine = _outline(p, catalog);
      return devices.fold(0.0, (sum, d) => sum + _overlap(mine, _outline(d, catalog)));
    }

    var best = 0;
    var least = double.infinity;
    for (final (i, hook) in free.indexed) {
      final c = cover(hook) + i * 1e-3;
      if (c < least) (best, least) = (i, c);
    }
    final hook = free.removeAt(best);
    devices.add(Placed(g, sprite, V3(hook.x, room.height - _drop, hook.z), 0, g.spec.scale, hung: true));
  }

  final anchors = <String, V3>{};
  for (final d in devices) {
    final members = d.group.entities;
    if (d.group.model == 'strip') {
      for (final (i, e) in members.indexed) {
        anchors[e.id] = d.toWorld(DeviceGroup.outlet(min(members.length, 6), min(i, 5)));
      }
    } else {
      final local = d.hung ? V3(0, catalog.bounds[d.sprite]![2], 0) : d.group.spec.anchor;
      for (final e in members) {
        anchors[e.id] = d.toWorld(local);
      }
    }
  }
  return RoomScene(room, devices, anchors, unplaced);
}

/// A device's screen outline at the default view, in view units: left, bottom, right, top.
(double, double, double, double) _outline(Placed d, RoomCatalog catalog) {
  final box = catalog.bounds[d.sprite]!;
  final camera = OrbitCamera();
  final c = camera.view(d.center);
  final half = max(box[1] - box[0], box[5] - box[4]) * d.scale / 2;
  final up = cos(camera.pitch) * d.scale;
  return (c.x - half, c.y + box[2] * up, c.x + half, c.y + box[3] * up);
}

/// How much two devices' outlines overlap at the default view (view units squared).
double overlapOf(Placed a, Placed b, RoomCatalog catalog) => _overlap(_outline(a, catalog), _outline(b, catalog));

double _overlap((double, double, double, double) a, (double, double, double, double) b) {
  final w = min(a.$3, b.$3) - max(a.$1, b.$1), h = min(a.$4, b.$4) - max(a.$2, b.$2);
  return w > 0 && h > 0 ? w * h : 0;
}

/// Length a device takes along a run: a strip by its outlets (its cable trails behind).
double _width(DeviceGroup g, RoomCatalog catalog) {
  if (g.model == 'strip') return (0.62 * min(g.entities.length, 6) + 0.5) * g.spec.scale;
  final box = catalog.bounds[spriteName(g)]!;
  return (box[1] - box[0]) * g.spec.scale;
}
