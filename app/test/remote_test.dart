import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:homemaid/screens/remote.dart';

void main() {
  test('launchers find their bundled logo by config or by name', () {
    expect(logoFor('netflix', 'Whatever'), 'netflix');
    expect(logoFor(null, 'Spotify'), 'spotify');
    expect(logoFor(null, 'Firefox'), 'firefoxbrowser', reason: 'alias');
    expect(logoFor(null, 'Google Chrome'), 'googlechrome');
    expect(logoFor('nope', 'Notepad'), isNull, reason: 'no logo: initials');
    for (final name in ['netflix', 'youtube', 'spotify', 'twitch', 'crunchyroll', 'firefoxbrowser', 'vlcmediaplayer']) {
      expect(File('assets/logos/$name.svg').existsSync(), isTrue, reason: name);
    }
  });
}
