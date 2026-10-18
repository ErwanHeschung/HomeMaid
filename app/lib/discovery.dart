/// Finds a paired host again when its IP changed, from the mDNS announcements of the core and
/// the agents (`_homemaid-<kind>._tcp`, TXT `id` = hostname).
library;

import 'dart:async';
import 'dart:io';

import 'package:bonsoir/bonsoir.dart';

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
          if (ipv4.isNotEmpty) found.complete(ipv4.first);
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
