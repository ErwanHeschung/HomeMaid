/// Finds a paired host again when its IP changed, from the mDNS announcements of the core and
/// the agents (`_homemaid-<kind>._tcp`, TXT `id` = hostname). Anyone on the network can announce
/// that name: a new address is kept only once it proves it knows the token.
library;

import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math';

import 'package:bonsoir/bonsoir.dart';
import 'package:crypto/crypto.dart';

import 'connections.dart';

Future<String?> locate(Connection connection) async {
  final discovery = BonsoirDiscovery(type: '_homemaid-${connection.kind}._tcp');
  final found = Completer<String?>();
  try {
    await discovery.initialize();
    discovery.eventStream!.listen((event) {
      switch (event) {
        case BonsoirDiscoveryServiceFoundEvent():
          event.service.resolve(discovery.serviceResolver);
        case BonsoirDiscoveryServiceResolvedEvent():
          final service = event.service;
          if ((service.attributes['id'] ?? service.name) != connection.id || found.isCompleted) return;
          // IPv4 first: IPv6 link-local addresses need a scope to be dialed.
          final ipv4 = service.hostAddresses.where((a) => InternetAddress.tryParse(a)?.type == InternetAddressType.IPv4);
          if (ipv4.isEmpty) return;
          knowsToken(connection, ipv4.first).then((ok) {
            if (ok && !found.isCompleted) found.complete(ipv4.first);
          });
        default:
      }
    });
    await discovery.start();
    return await found.future.timeout(const Duration(seconds: 5), onTimeout: () => null);
  } catch (_) {
    // No multicast on this network: keep the paired address.
    return null;
  } finally {
    await discovery.stop();
  }
}

/// The host's answer to `/hello?nonce=`: HMAC-SHA256(token, "homemaid-host:" + nonce), in hex.
String proof(String token, String nonce) =>
    Hmac(sha256, utf8.encode(token)).convert(utf8.encode('homemaid-host:$nonce')).toString();

/// Whether [host] knows the connection's token, asked without sending it.
// ponytail: an impostor that can also reach the real host could relay the proof; binding it to the
// dialed address would stop that. Such an attacker can already read the plain ws:// on the LAN.
Future<bool> knowsToken(Connection connection, String host) async {
  final random = Random.secure();
  final nonce = [for (var i = 0; i < 16; i++) random.nextInt(256).toRadixString(16).padLeft(2, '0')].join();
  final client = HttpClient()..connectionTimeout = const Duration(seconds: 3);
  try {
    final request = await client.get(host, connection.port, '/hello?nonce=$nonce');
    final response = await request.close().timeout(const Duration(seconds: 3));
    final body = await response.transform(utf8.decoder).join().timeout(const Duration(seconds: 3));
    return response.statusCode == 200 && body == proof(connection.token, nonce);
  } catch (_) {
    return false;
  } finally {
    client.close(force: true);
  }
}
