import 'dart:ui';

import 'package:flutter_test/flutter_test.dart';
import 'package:homemaid/theme.dart';

/// The matrix applied to one colour, as the engine does (channels 0-255).
List<double> apply(List<double> m, List<double> rgba) =>
    [for (var row = 0; row < 4; row++) m[row * 5 + 4] + [for (var i = 0; i < 4; i++) m[row * 5 + i] * rgba[i]].reduce((a, b) => a + b)];

void main() {
  test('duotone: black becomes the graphite, white the ink, transparency stays', () {
    final m = Hm.duotoneMatrix(Hm.raised, Hm.text);
    List<double> rgb(Color c) => [c.r * 255, c.g * 255, c.b * 255];
    final black = apply(m, [0, 0, 0, 255]), white = apply(m, [255, 255, 255, 128]);
    for (var i = 0; i < 3; i++) {
      expect(black[i], closeTo(rgb(Hm.raised)[i], 0.01));
      expect(white[i], closeTo(rgb(Hm.text)[i], 0.01));
    }
    expect(white[3], 128, reason: 'alpha untouched');
    // A saturated colour lands in between, by its brightness: green is brighter than blue.
    final green = apply(m, [0, 255, 0, 255])[0], blue = apply(m, [0, 0, 255, 255])[0];
    expect(green, greaterThan(blue));
  });
}
