/// Paired hosts (core and PC agents), from the `/pair` QR codes, kept in the secure storage
/// with their tokens, plus the remote's settings.
library;

import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';

class Connection {
  const Connection({required this.kind, required this.id, required this.host, required this.port, required this.token});

  /// `homemaid://pair?kind=core|agent&id=<hostname>&host=<ip>&port=<port>&token=<token>`
  static Connection? fromPairUri(String raw) {
    final uri = Uri.tryParse(raw);
    if (uri == null || uri.scheme != 'homemaid' || uri.host != 'pair') return null;
    final q = uri.queryParameters;
    final port = int.tryParse(q['port'] ?? '');
    if (!const {'core', 'agent'}.contains(q['kind']) || port == null) return null;
    final [id, host, token] = [q['id'], q['host'], q['token']];
    if (id == null || host == null || token == null || token.isEmpty) return null;
    return Connection(kind: q['kind']!, id: id, host: host, port: port, token: token);
  }

  factory Connection.fromJson(Map<String, dynamic> json) => Connection(
        kind: json['kind'] as String,
        id: json['id'] as String,
        host: json['host'] as String,
        port: json['port'] as int,
        token: json['token'] as String,
      );

  /// `core` or `agent`.
  final String kind;

  /// Hostname of the PC, also announced over mDNS.
  final String id;
  final String host;
  final int port;
  final String token;

  Map<String, Object> toJson() => {'kind': kind, 'id': id, 'host': host, 'port': port, 'token': token};

  Connection withHost(String host) => Connection(kind: kind, id: id, host: host, port: port, token: token);

  bool sameAs(Connection other) => kind == other.kind && id == other.id;
}

class Connections extends ChangeNotifier {
  Connections([FlutterSecureStorage? storage]) : _storage = storage ?? const FlutterSecureStorage();

  /// In memory only, nothing read from or written to the device (the demo).
  Connections.memory(this.all) : _storage = null;

  final FlutterSecureStorage? _storage;
  List<Connection> all = [];
  double speed = 1.8;
  int scrollSign = 1;

  Connection? get core => all.where((c) => c.kind == 'core').firstOrNull;
  List<Connection> get agents => all.where((c) => c.kind == 'agent').toList();

  Future<void> load() async {
    final storage = _storage;
    if (storage == null) return;
    final saved = await storage.read(key: 'connections');
    if (saved != null) {
      all = (jsonDecode(saved) as List).map((c) => Connection.fromJson(c as Map<String, dynamic>)).toList();
    }
    final settings = await storage.read(key: 'settings');
    if (settings != null) {
      final s = jsonDecode(settings) as Map<String, dynamic>;
      speed = (s['speed'] as num?)?.toDouble() ?? speed;
      scrollSign = s['scrollSign'] as int? ?? scrollSign;
    }
    notifyListeners();
  }

  /// Adds or replaces (same kind and id): pairing again updates the address and token.
  Future<void> put(Connection connection) async {
    all = [...all.where((c) => !c.sameAs(connection)), connection];
    await _save();
  }

  Future<void> remove(Connection connection) async {
    all = all.where((c) => !c.sameAs(connection)).toList();
    await _save();
  }

  Future<void> setRemote({double? speed, int? scrollSign}) async {
    this.speed = speed ?? this.speed;
    this.scrollSign = scrollSign ?? this.scrollSign;
    await _storage?.write(key: 'settings', value: jsonEncode({'speed': this.speed, 'scrollSign': this.scrollSign}));
    notifyListeners();
  }

  Future<void> _save() async {
    await _storage?.write(key: 'connections', value: jsonEncode(all));
    notifyListeners();
  }
}
