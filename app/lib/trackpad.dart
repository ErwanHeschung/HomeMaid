/// Trackpad gestures, independent of Flutter so they can be unit tested: one finger moves,
/// two fingers scroll, a short still touch clicks (right click with two fingers).
/// Same behavior as the agent's `remote.html`.
library;

import 'dart:math';

class Trackpad {
  Trackpad({this.speed = 1.8, this.scrollSign = 1});

  /// Pointer pixels per finger pixel.
  double speed;

  /// 1 for natural scrolling, -1 to invert.
  int scrollSign;

  static const _tapMaxMs = 250;
  static const _tapMaxMove = 10.0;
  static const _scrollPx = 40.0;

  final _pointers = <int, Point<double>>{};
  int _gestureStart = 0;
  double _moved = 0;
  int _fingers = 0;
  double _dx = 0, _dy = 0, _sx = 0, _sy = 0;

  bool get touching => _pointers.isNotEmpty;

  void down(int pointer, Point<double> position, int timeMs) {
    _pointers[pointer] = position;
    if (_pointers.length == 1) {
      _gestureStart = timeMs;
      _moved = 0;
      _fingers = 1;
    } else {
      _fingers = max(_fingers, _pointers.length);
    }
  }

  void move(int pointer, Point<double> position) {
    final last = _pointers[pointer];
    if (last == null) return;
    _pointers[pointer] = position;
    final dx = position.x - last.x, dy = position.y - last.y;
    _moved += dx.abs() + dy.abs();
    if (_pointers.length == 1) {
      _dx += dx * speed;
      _dy += dy * speed;
    } else {
      _sx += -dx / _scrollPx / _pointers.length * scrollSign;
      _sy += -dy / _scrollPx / _pointers.length * scrollSign;
    }
  }

  /// Returns the click request if the gesture ending here was a tap.
  Map<String, Object>? up(int pointer, int timeMs) {
    _pointers.remove(pointer);
    if (_pointers.isNotEmpty || _fingers == 0) return null;
    final tap = timeMs - _gestureStart < _tapMaxMs && _moved < _tapMaxMove;
    final fingers = _fingers;
    _fingers = 0;
    if (!tap) return null;
    return {'type': 'click', 'button': fingers > 1 ? 'right' : 'left'};
  }

  /// Requests accumulated since the last call; call once per frame.
  List<Map<String, Object>> flush() {
    final requests = <Map<String, Object>>[
      if (_dx != 0 || _dy != 0) {'type': 'move', 'dx': _dx, 'dy': _dy},
      if (_sx != 0 || _sy != 0) {'type': 'scroll', 'dx': _sx, 'dy': _sy},
    ];
    _dx = _dy = _sx = _sy = 0;
    return requests;
  }
}
