/// Minimal 3D for the room viewport: points, an orbit camera and an orthographic projector, the
/// same conventions as the studio renders it composes (tools/render).
library;

import 'dart:math';
import 'dart:ui';

/// A point in room units: x along the back wall, y up, z towards the open side.
class V3 {
  const V3(this.x, this.y, this.z);

  final double x, y, z;

  static const zero = V3(0, 0, 0);

  V3 operator +(V3 o) => V3(x + o.x, y + o.y, z + o.z);
  V3 operator -(V3 o) => V3(x - o.x, y - o.y, z - o.z);
  V3 scale(double s) => V3(x * s, y * s, z * s);
  double get length => sqrt(x * x + y * y + z * z);

  /// Turned about the vertical axis by [facing] (0 keeps +z, pi/2 turns +z to +x).
  V3 turn(double facing) {
    final c = cos(facing), s = sin(facing);
    return V3(x * c + z * s, y, -x * s + z * c);
  }

  @override
  String toString() => 'V3($x, $y, $z)';
}

/// Looks at the room from above: [yaw] turns around the vertical axis, [pitch] tilts the floor
/// towards the viewer (0 = level, pi/2 = straight down).
class OrbitCamera {
  OrbitCamera({this.yaw = -pi / 4, this.pitch = 0.62});

  double yaw;
  double pitch;

  /// World to view space: x right, y up, z away from the viewer.
  V3 view(V3 p) {
    final cy = cos(yaw), sy = sin(yaw), cp = cos(pitch), sp = sin(pitch);
    final x = p.x * cy - p.z * sy;
    final z1 = -(p.x * sy + p.z * cy);
    return V3(x, p.y * cp + z1 * sp, -p.y * sp + z1 * cp);
  }
}

/// Fits a [width] by [height] (world units, in view space) frame around [focus] into [size],
/// [focus] at the middle. Orthographic, like the renders.
class Projector {
  Projector(this.camera, Size size, {required V3 focus, required double width, required double height})
      : scale = min(size.width / width, size.height / height) {
    final f = camera.view(focus);
    _center = Offset(size.width / 2 - f.x * scale, size.height / 2 + f.y * scale);
  }

  final OrbitCamera camera;
  late final Offset _center;

  /// Screen pixels per world unit.
  final double scale;

  /// Screen position of a view-space point.
  Offset screen(V3 v) => Offset(_center.dx + v.x * scale, _center.dy - v.y * scale);

  Offset project(V3 world) => screen(camera.view(world));
}
