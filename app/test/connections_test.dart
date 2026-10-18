import 'package:flutter_test/flutter_test.dart';
import 'package:homemaid/connections.dart';

void main() {
  test('reads the QR code of /pair', () {
    final c = Connection.fromPairUri('homemaid://pair?kind=agent&id=MY%20PC&host=192.168.1.10&port=8124&token=a%26b%3Dc')!;
    expect([c.kind, c.id, c.host, c.port, c.token], ['agent', 'MY PC', '192.168.1.10', 8124, 'a&b=c']);
  });

  test('refuses anything else', () {
    for (final raw in [
      'https://example.com',
      'homemaid://pair?kind=evil&id=a&host=h&port=1&token=t',
      'homemaid://pair?kind=core&id=a&host=h&port=x&token=t',
      'homemaid://pair?kind=core&id=a&host=h&port=1',
      'not a uri at all',
    ]) {
      expect(Connection.fromPairUri(raw), isNull, reason: raw);
    }
  });
}
