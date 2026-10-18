import 'dart:math';

import 'package:flutter_test/flutter_test.dart';
import 'package:homemaid/trackpad.dart';

void main() {
  test('a short still touch is a left click, with two fingers a right click', () {
    final pad = Trackpad();
    pad.down(1, const Point(10, 10), 0);
    expect(pad.up(1, 100), {'type': 'click', 'button': 'left'});

    pad.down(1, const Point(10, 10), 1000);
    pad.down(2, const Point(50, 10), 1010);
    expect(pad.up(1, 1100), isNull, reason: 'one finger still down');
    expect(pad.up(2, 1120), {'type': 'click', 'button': 'right'});
  });

  test('moves are batched per frame and scaled', () {
    final pad = Trackpad(speed: 2);
    pad.down(1, const Point(0, 0), 0);
    pad.move(1, const Point(3, 0));
    pad.move(1, const Point(15, -1));
    expect(pad.flush(), [
      {'type': 'move', 'dx': 30.0, 'dy': -2.0},
    ]);
    expect(pad.flush(), isEmpty);
    expect(pad.up(1, 50), isNull, reason: 'it moved: not a tap');
  });

  test('two fingers scroll, in notches', () {
    final pad = Trackpad();
    pad.down(1, const Point(0, 100), 0);
    pad.down(2, const Point(40, 100), 0);
    pad.move(1, const Point(0, 20));
    pad.move(2, const Point(40, 20));
    final [scroll] = pad.flush();
    expect(scroll['type'], 'scroll');
    expect(scroll['dy'], closeTo(2.0, 1e-9), reason: '80 px up with two fingers = 2 notches');
  });
}
