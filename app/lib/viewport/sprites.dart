/// Studio renders of each model (tools/render/render.py): a turntable of [frames] views, plus one
/// glow pass per light part, loaded lazily and kept in a bounded cache.
library;

import 'dart:async';
import 'dart:math';
import 'dart:ui' as ui;

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

import 'models.dart';

/// Must match render.py.
const frames = 24;
const orthoScale = 3.2;
const renderPitch = 0.62;

/// The asset folder of a group's renders. Strips are rendered with 2 to 6 outlets; longer ones
/// show as a 6-outlet strip.
String spriteName(DeviceGroup group) =>
    group.model == 'strip' ? 'strip_${group.entities.length.clamp(2, 6)}' : group.model;

/// Turntable frame showing the model at [yaw].
int frameAt(double yaw) => ((yaw / (2 * pi) * frames).round() % frames + frames) % frames;

/// Yaw of the nearest rendered frame: where the orbit settles.
double snapYaw(double yaw) => (yaw / (2 * pi) * frames).round() * 2 * pi / frames;

String basePath(String name, int frame) => 'assets/models/$name/base_${frame.toString().padLeft(2, '0')}.webp';

String glowPath(String name, int light, int frame) =>
    'assets/models/$name/glow${light}_${frame.toString().padLeft(2, '0')}.webp';

class SpriteCache extends ChangeNotifier {
  SpriteCache._();

  static final instance = SpriteCache._();

  /// ponytail: least-recently-used, by count (~6 models' turntables and glows at 320/192 px);
  /// size it by bytes if models get bigger.
  static const _capacity = 420;
  static const _roomCapacity = 14;

  /// Insertion-ordered: the first key is the least recently used.
  final _images = <String, ui.Image>{};
  final _loading = <String>{};
  final _missing = <String>{};

  /// The image if decoded; otherwise starts loading it and returns null.
  ui.Image? get(String path) {
    final image = _images.remove(path);
    if (image != null) {
      _images[path] = image;
      return image;
    }
    if (!_loading.contains(path) && !_missing.contains(path)) _load(path);
    return null;
  }

  bool missing(String path) => _missing.contains(path);

  /// Decodes a model's whole turntable up front, so a fast spin never waits on a frame.
  void preload(String name, int lights) {
    for (var k = 0; k < frames; k++) {
      get(basePath(name, k));
      for (var j = 0; j < lights; j++) {
        get(glowPath(name, j, k));
      }
    }
  }

  /// The decoded frame closest to [frame] (by turntable distance), while [frame] itself loads.
  /// A room's views ([count] of them, over an arc) do not wrap around.
  ui.Image? nearest(String Function(int frame) path, int frame, {int count = frames, bool wrap = true}) {
    for (var d = 0; d <= count; d++) {
      for (final k in {frame + d, frame - d}) {
        final i = wrap ? k % count : k;
        if (i < 0 || i >= count) continue;
        final image = _images[path(i)];
        if (image != null) return image;
      }
    }
    return null;
  }

  Timer? _notify;

  /// Many frames land together while preloading: repaint once for the batch.
  void _changed() => _notify ??= Timer(const Duration(milliseconds: 60), () {
        _notify = null;
        notifyListeners();
      });

  Future<void> _load(String path) async {
    _loading.add(path);
    try {
      final data = await rootBundle.load(path);
      final codec = await ui.instantiateImageCodec(data.buffer.asUint8List());
      _images[path] = (await codec.getNextFrame()).image;
      while (_images.length > _capacity) {
        _images.remove(_images.keys.first)!.dispose();
      }
      // ponytail: room views are big (~3 MB decoded): keep two rooms' worth, by count.
      final rooms = _images.keys.where((k) => k.startsWith('assets/rooms/')).toList();
      for (final k in rooms.take(max(0, rooms.length - _roomCapacity))) {
        _images.remove(k)!.dispose();
      }
      _changed();
    } catch (_) {
      _missing.add(path);
      _changed();
    } finally {
      _loading.remove(path);
    }
  }
}
