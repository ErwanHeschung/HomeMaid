/// The room as a live model: a furnished dollhouse cut-away with the studio-rendered devices set in
/// it, annotated by leader-line callouts. Pink = physically on; lavender hatch = pending; ghosted =
/// offline; red = failed. It turns over the 90° arc its views were rendered for.
library;

import 'dart:math';

import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';

import '../l10n/app_localizations.dart';
import '../model.dart';
import '../theme.dart';
import '../widgets/state.dart';
import 'geometry.dart';
import 'models.dart';
import 'rooms.dart';
import 'sprites.dart';

/// The viewport annotates at most this many devices; the list below carries the rest.
const maxCallouts = 6;

/// Which entities get a callout first: the selected one, then what needs attention (failed,
/// pending), then what is on, then the rest in list order.
List<String> calloutPriority(List<Entity> entities, String? selected) {
  int rank(Entity e) => e.id == selected
      ? 0
      : isFailed(e)
          ? 1
          : isPending(e)
              ? 2
              : onLevel(e) > 0
                  ? 3
                  : 4;
  final ordered = [...entities.indexed]..sort((a, b) {
      final byRank = rank(a.$2).compareTo(rank(b.$2));
      return byRank != 0 ? byRank : a.$1.compareTo(b.$1);
    });
  return [for (final (_, e) in ordered) e.id];
}

class RoomViewport extends StatefulWidget {
  const RoomViewport({super.key, required this.kind, required this.entities, required this.selected, required this.onSelect});

  /// One of [roomKinds]: which furnished room the devices are set in.
  final String kind;
  final List<Entity> entities;
  final String? selected;
  final ValueChanged<String> onSelect;

  @override
  State<RoomViewport> createState() => _RoomViewportState();
}

class _RoomViewportState extends State<RoomViewport> with SingleTickerProviderStateMixin {
  final _camera = OrbitCamera();
  late final Ticker _ticker = createTicker(_tick);
  Duration? _last;

  /// Animated per-entity values, eased towards their targets.
  final _on = <String, double>{};
  final _pending = <String, double>{};
  double _spin = 0;

  RoomScene? _scene;
  String? _sceneKey;

  @override
  void initState() {
    super.initState();
    _camera.pitch = renderPitch;
    SpriteCache.instance.addListener(_imagesLoaded);
  }

  void _imagesLoaded() {
    if (mounted) setState(() {});
  }

  @override
  void dispose() {
    SpriteCache.instance.removeListener(_imagesLoaded);
    _ticker.dispose();
    super.dispose();
  }

  RoomScene get scene {
    final groups = groupDevices(widget.entities);
    final key = '${widget.kind}/${groups.map((g) => '${g.model}:${g.entities.map((e) => e.id).join(',')}').join('|')}';
    if (key != _sceneKey) {
      _scene = layoutRoom(widget.kind, groups);
      _sceneKey = key;
      final sprites = SpriteCache.instance;
      for (var i = 0; i < RoomCatalog.instance.views; i++) {
        sprites.get(roomPath(_scene!.room.id, i));
      }
      for (final d in _scene!.devices) {
        sprites.preload(d.sprite, d.group.entities.length);
      }
    }
    return _scene!;
  }

  /// The orbit stays on the arc the room was rendered for.
  void _turn(double by) => _camera.yaw = (_camera.yaw + by).clamp(RoomCatalog.instance.minYaw, 0.0);

  bool get _reduceMotion => MediaQuery.of(context).disableAnimations;

  @override
  void didUpdateWidget(RoomViewport old) {
    super.didUpdateWidget(old);
    _animate();
  }

  void _animate() {
    if (_reduceMotion) {
      for (final e in widget.entities) {
        _on[e.id] = onLevel(e);
        _pending[e.id] = isPending(e) ? 1 : 0;
      }
      return;
    }
    if (!_ticker.isActive) {
      _last = null;
      _ticker.start();
    }
  }

  void _tick(Duration now) {
    final dt = _last == null ? 0.016 : (now - _last!).inMicroseconds / 1e6;
    _last = now;
    var moving = false;
    // Exponential ease-out towards each target, ~120 ms time constant.
    final k = 1 - exp(-dt / 0.12);
    for (final e in widget.entities) {
      for (final (map, target) in [(_on, onLevel(e)), (_pending, isPending(e) ? 1.0 : 0.0)]) {
        final v = map[e.id] ?? target;
        final next = v + (target - v) * k;
        map[e.id] = (next - target).abs() < 0.004 ? target : next;
        moving |= map[e.id] != target;
      }
    }
    if (_spin.abs() > 0.25) {
      final before = _camera.yaw;
      _turn(_spin * dt);
      // Stops at the end of the arc.
      _spin = _camera.yaw == before ? 0 : _spin * exp(-dt * 3.5);
      moving = true;
    } else {
      // Settle on the nearest rendered view, so the resting image is a crisp frame.
      _spin = 0;
      final target = snapYaw(_camera.yaw);
      _camera.yaw += (target - _camera.yaw) * k;
      if ((target - _camera.yaw).abs() < 0.002) {
        _camera.yaw = target;
      } else {
        moving = true;
      }
    }
    setState(() {});
    if (!moving) _ticker.stop();
  }

  @override
  Widget build(BuildContext context) {
    final scene = this.scene;
    for (final e in widget.entities) {
      _on.putIfAbsent(e.id, () => onLevel(e));
      _pending.putIfAbsent(e.id, () => isPending(e) ? 1 : 0);
    }
    return LayoutBuilder(builder: (context, constraints) {
      final size = constraints.biggest;
      final room = scene.room;
      // Everything is drawn at the room's rendered view nearest the orbit: devices and callouts
      // step with the room instead of sliding over it.
      final rooms = RoomCatalog.instance;
      final shown = OrbitCamera(yaw: -rooms.viewAt(_camera.yaw) * rooms.step, pitch: _camera.pitch);
      final projector = Projector(shown, size, focus: room.focus, width: room.scale, height: room.scale / room.aspect);
      final byId = {for (final e in widget.entities) e.id: e};
      final annotated = calloutPriority(widget.entities, widget.selected).where(scene.anchors.containsKey).take(maxCallouts);
      final callouts = layoutCallouts({for (final id in annotated) id: projector.project(scene.anchors[id]!)}, size);
      final overflow = widget.entities.length - callouts.length;
      return GestureDetector(
        behavior: HitTestBehavior.opaque,
        // A turntable: the renders exist at one pitch, so only the yaw follows the finger.
        onHorizontalDragUpdate: (d) => setState(() => _turn(d.delta.dx * 0.008)),
        onHorizontalDragEnd: (d) {
          if (_reduceMotion) {
            setState(() => _camera.yaw = snapYaw(_camera.yaw));
            return;
          }
          _spin = d.velocity.pixelsPerSecond.dx * 0.008;
          _animate();
        },
        child: Stack(children: [
          Positioned.fill(
            child: CustomPaint(
              painter: _RoomPainter(
                scene: scene,
                projector: projector,
                entities: byId,
                on: _on,
                pending: _pending,
                selected: widget.selected,
                callouts: callouts,
              ),
            ),
          ),
          for (final c in callouts)
            Positioned(
              left: c.box.left,
              top: c.box.top,
              width: c.box.width,
              height: c.box.height,
              // The rows carry the same text at full scale: callouts stay inside their boxes.
              child: MediaQuery.withClampedTextScaling(
                maxScaleFactor: 1.15,
                child: _CalloutLabel(entity: byId[c.id]!, left: c.left, onTap: () => widget.onSelect(c.id)),
              ),
            ),
          if (overflow > 0)
            Positioned(
              right: 12,
              bottom: 8,
              child: Text(
                AppLocalizations.of(context).moreInList(overflow),
                style: Theme.of(context).textTheme.labelSmall?.copyWith(color: Hm.textDim, fontFeatures: Hm.tabular),
              ),
            ),
        ]),
      );
    });
  }
}

/// A CAD callout: text resting on a horizontal shoulder, the leader running from the shoulder's
/// inner end to the anchor on the model.
class Callout {
  Callout(this.id, this.anchor, this.box, this.left);

  final String id;
  final Offset anchor;

  /// The tappable label area; the shoulder runs along its bottom edge.
  final Rect box;
  final bool left;

  Offset get shoulderInner => left ? box.bottomRight : box.bottomLeft;
  Offset get shoulderOuter => left ? box.bottomLeft : box.bottomRight;
}

const _labelWidth = 124.0;
const _labelHeight = 48.0;
const _margin = 12.0;

/// Stacks labels in two columns by anchor height, never overlapping, never off the view.
List<Callout> layoutCallouts(Map<String, Offset> anchors, Size size) {
  final out = <Callout>[];
  for (final left in [true, false]) {
    final side = anchors.entries.where((a) => (a.value.dx < size.width / 2) == left).toList()
      ..sort((a, b) => a.value.dy.compareTo(b.value.dy));
    final tops = <double>[];
    for (final a in side) {
      final wanted = a.value.dy - _labelHeight - 18;
      tops.add(max(wanted, tops.isEmpty ? 4 : tops.last + _labelHeight + 4));
    }
    // Pushed past the bottom: slide the column back up.
    final overflow = tops.isEmpty ? 0 : tops.last + _labelHeight - (size.height - 4);
    for (final (i, a) in side.indexed) {
      final top = max(4.0, tops[i] - max(0, overflow));
      final x = left ? _margin : size.width - _margin - _labelWidth;
      out.add(Callout(a.key, a.value, Rect.fromLTWH(x, top, _labelWidth, _labelHeight), left));
    }
  }
  return out;
}

class _CalloutLabel extends StatelessWidget {
  const _CalloutLabel({required this.entity, required this.left, required this.onTap});

  final Entity entity;
  final bool left;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final align = left ? CrossAxisAlignment.start : CrossAxisAlignment.end;
    return Semantics(
      button: true,
      label: '${entity.name}, ${summary(context, entity)}',
      child: InkWell(
        onTap: onTap,
        borderRadius: BorderRadius.circular(Hm.radius),
        // A CAD callout plate, as wide as its text: the room shows through everywhere else, but
        // walls and devices never run through the words.
        child: Align(
          alignment: left ? Alignment.bottomLeft : Alignment.bottomRight,
          child: DecoratedBox(
            decoration: BoxDecoration(
              color: Hm.ground.withValues(alpha: 0.88),
              border: Border.all(color: Hm.line),
              borderRadius: BorderRadius.circular(Hm.radius),
            ),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 4),
              child: ExcludeSemantics(
            child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: align, children: [
              Text(entity.name, maxLines: 1, overflow: TextOverflow.ellipsis, style: theme.textTheme.labelMedium),
              FlipText(
                summary(context, entity),
                alignment: left ? Alignment.centerLeft : Alignment.centerRight,
                style: theme.textTheme.labelSmall!.copyWith(
                  color: stateColor(entity),
                  fontFeatures: Hm.tabular,
                ),
              ),
            ]),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _RoomPainter extends CustomPainter {
  _RoomPainter({
    required this.scene,
    required this.projector,
    required this.entities,
    required this.on,
    required this.pending,
    required this.selected,
    required this.callouts,
  });

  final RoomScene scene;
  final Projector projector;
  final Map<String, Entity> entities;
  final Map<String, double> on;
  final Map<String, double> pending;
  final String? selected;
  final List<Callout> callouts;

  final _sprites = SpriteCache.instance;

  @override
  void paint(Canvas canvas, Size size) {
    final camera = projector.camera;
    _paintRoom(canvas);
    _paintLightPools(canvas);
    // Far to near, each device whole. Still decoding: nothing rather than a placeholder.
    final devices = [...scene.devices]..sort((a, b) => camera.view(b.center).z.compareTo(camera.view(a.center).z));
    for (final d in devices) {
      if (d.hung) _paintCord(canvas, d);
      _paintSprite(canvas, d);
    }
    _paintLeaders(canvas);
  }

  /// The furnished room at the camera's view (or the nearest decoded one while it loads).
  void _paintRoom(Canvas canvas) {
    final rooms = RoomCatalog.instance;
    final room = scene.room;
    final view = rooms.viewAt(projector.camera.yaw);
    String path(int i) => roomPath(room.id, i);
    final image = _sprites.get(path(view)) ?? _sprites.nearest(path, view, count: rooms.views, wrap: false);
    if (image == null) return;
    final width = room.scale * projector.scale;
    final rect = Rect.fromCenter(center: projector.project(room.focus), width: width, height: width / room.aspect);
    canvas.drawImageRect(
      image,
      Rect.fromLTWH(0, 0, image.width.toDouble(), image.height.toDouble()),
      rect,
      Paint()..filterQuality = FilterQuality.medium,
    );
  }

  /// A pendant's cord, up to the ceiling, and its canopy.
  void _paintCord(Canvas canvas, Placed d) {
    final top = projector.project(V3(d.center.x, scene.room.height, d.center.z));
    final paint = _stroke(Hm.textDim.withValues(alpha: 0.7), 1);
    canvas.drawLine(projector.project(d.center), top, paint);
    canvas.drawOval(Rect.fromCenter(center: top, width: 8, height: 3.5), Paint()..color = Hm.textDim);
  }

  /// Draws the render of [d] at the camera's yaw, or the nearest decoded view while it loads.
  void _paintSprite(Canvas canvas, Placed d) {
    final name = d.sprite;
    final frame = frameAt(projector.camera.yaw - d.facing);
    final base = _sprites.get(basePath(name, frame)) ?? _sprites.nearest((k) => basePath(name, k), frame);
    if (base == null) return;
    final side = orthoScale * projector.scale * d.scale;
    final rect = Rect.fromCenter(center: projector.project(d.center), width: side, height: side);
    final src = Rect.fromLTWH(0, 0, base.width.toDouble(), base.height.toDouble());
    final members = d.group.entities;
    final allOffline = members.every((e) => !e.available);
    final single = members.length == 1;
    final selectedHere = members.any((e) => e.id == selected);
    final failedHere = single && isFailed(members.first);

    // Halo behind the silhouette: lavender, or red when the device's command failed.
    if (selectedHere || failedHere) {
      canvas.drawImageRect(
        base,
        src,
        rect,
        Paint()
          ..colorFilter = ColorFilter.mode(failedHere ? Hm.danger : Hm.select, BlendMode.srcIn)
          ..maskFilter = MaskFilter.blur(BlurStyle.outer, failedHere ? 10 : 7),
      );
    }

    if (allOffline) {
      // Offline: a ghost of the device, desaturated.
      canvas.drawImageRect(
        base,
        src,
        rect,
        Paint()..colorFilter = const ColorFilter.matrix(_ghost),
      );
      return;
    }

    final hatch = single ? (pending[members.first.id] ?? 0) : 0.0;
    if (hatch > 0.01) {
      canvas.saveLayer(rect, Paint());
      canvas.drawImageRect(base, src, rect, Paint());
      _hatchRect(canvas, rect, Hm.pending.withValues(alpha: 0.75 * hatch), BlendMode.srcATop);
      canvas.restore();
    } else {
      canvas.drawImageRect(base, src, rect, Paint());
    }

    // Light passes, tinted lavender by multiplication (the render's falloff survives), with a bloom.
    // Large emitters (screens) are kept softer.
    final strength = const {'screen', 'tv', 'washing_machine', 'fridge', 'climate_sensor'}.contains(d.group.model) ? 0.75 : 1.0;
    for (final (j, e) in members.indexed) {
      final level = (on[e.id] ?? 0) * strength;
      if (level < 0.01 || !e.available || j >= 6) continue;
      final glow = _sprites.get(glowPath(name, j, frame)) ?? _sprites.nearest((k) => glowPath(name, j, k), frame);
      if (glow == null) continue;
      final gsrc = Rect.fromLTWH(0, 0, glow.width.toDouble(), glow.height.toDouble());
      final tint = ColorFilter.mode(Hm.on.withValues(alpha: level), BlendMode.modulate);
      canvas.drawImageRect(glow, gsrc, rect, Paint()
        ..colorFilter = tint
        ..maskFilter = MaskFilter.blur(BlurStyle.normal, 9 * d.scale)
        ..blendMode = BlendMode.plus);
      canvas.drawImageRect(glow, gsrc, rect, Paint()..colorFilter = tint);
    }

    // Several entities on one device: each outlet shows its own state at its anchor.
    if (!single) {
      for (final e in members) {
        final anchor = scene.anchors[e.id];
        if (anchor == null) continue;
        final mark = Rect.fromCenter(center: projector.project(anchor), width: 9, height: 9);
        final outline = Paint()
          ..style = PaintingStyle.stroke
          ..strokeWidth = 1.5;
        if (!e.available) {
          canvas.drawRect(mark, outline..color = Hm.textDim);
        } else if (isFailed(e)) {
          canvas.drawRect(mark, outline..color = Hm.danger);
        } else if ((pending[e.id] ?? 0) > 0.01) {
          canvas.save();
          canvas.clipRect(mark);
          _hatchRect(canvas, mark, Hm.pending.withValues(alpha: pending[e.id]!), BlendMode.srcOver);
          canvas.restore();
          canvas.drawRect(mark, outline..color = Hm.pending);
        }
      }
    }
  }

  static const _ghost = <double>[
    0.21, 0.72, 0.07, 0, 0, //
    0.21, 0.72, 0.07, 0, 0, //
    0.21, 0.72, 0.07, 0, 0, //
    0, 0, 0, 0.32, 0,
  ];

  void _hatchRect(Canvas canvas, Rect rect, Color color, BlendMode mode) {
    final paint = Paint()
      ..color = color
      ..strokeWidth = 1.3
      ..blendMode = mode;
    for (var x = rect.left - rect.height; x < rect.right; x += 5) {
      canvas.drawLine(Offset(x, rect.bottom), Offset(x + rect.height, rect.top), paint);
    }
  }

  /// Models whose own render glows wide; the rest (appliances, plugs) only have a small LED, so
  /// their spill on the floor is stronger to say "on".
  static const _lamps = {'lamp', 'bulb', 'ceiling_light', 'floor_lamp', 'light_strip', 'screen', 'tv'};

  /// Pink spilled on the floor under what is on.
  void _paintLightPools(Canvas canvas) {
    for (final MapEntry(key: id, value: anchor) in scene.anchors.entries) {
      final level = on[id] ?? 0;
      if (level < 0.01 || !(entities[id]?.available ?? false)) continue;
      final ring = [
        for (var i = 0; i < 24; i++)
          projector.project(V3(anchor.x + 0.8 * cos(2 * pi * i / 24), 0.002, anchor.z + 0.8 * sin(2 * pi * i / 24))),
      ];
      canvas.drawPath(
        Path()..addPolygon(ring, true),
        Paint()
          ..color = Hm.on.withValues(alpha: (_lamps.contains(scene.devices.firstWhere((d) => d.group.entities.any((e) => e.id == id)).group.model) ? 0.13 : 0.24) * level)
          ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 14),
      );
    }
  }

  void _paintLeaders(Canvas canvas) {
    for (final c in callouts) {
      final entity = entities[c.id];
      final color = c.id == selected ? Hm.select : Hm.textDim.withValues(alpha: 0.8);
      final path = Path()
        ..moveTo(c.shoulderOuter.dx, c.shoulderOuter.dy)
        ..lineTo(c.shoulderInner.dx, c.shoulderInner.dy)
        ..lineTo(c.anchor.dx, c.anchor.dy);
      canvas.drawPath(path, _stroke(color, 1));
      if (entity != null) _paintPick(canvas, c.anchor, entity);
    }
  }

  /// The pick point at a callout's end carries the state, for the devices whose light is too
  /// small to see: lavender on, pink ring pending, outline offline, red failed or alarming.
  void _paintPick(Canvas canvas, Offset at, Entity e) {
    canvas.drawCircle(at, 6.5, Paint()..color = Hm.ground);
    final ring = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1.5;
    if (!e.available) {
      canvas.drawCircle(at, 4.5, ring..color = Hm.textDim);
    } else if (isFailed(e) || alarming(e)) {
      canvas.drawCircle(at, 5, Paint()..color = Hm.danger);
    } else if ((pending[e.id] ?? 0) > 0.01) {
      canvas.drawCircle(at, 2.5, Paint()..color = Hm.pending);
      canvas.drawCircle(at, 4.5, ring..color = Hm.pending);
    } else {
      canvas.drawCircle(at, 5, Paint()..color = Color.lerp(Hm.textDim, Hm.on, on[e.id] ?? 0)!);
    }
  }

  static Paint _stroke(Color color, double width) => Paint()
    ..style = PaintingStyle.stroke
    ..color = color
    ..strokeWidth = width
    ..strokeJoin = StrokeJoin.round;

  @override
  bool shouldRepaint(_RoomPainter old) => true;
}
