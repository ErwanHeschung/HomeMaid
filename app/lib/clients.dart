/// Live connections to the paired hosts: the core (JSON-RPC) and the PC agents (remote).
library;

import 'dart:async';
import 'dart:convert';
import 'dart:math';

import 'package:flutter/foundation.dart';
import 'package:web_socket_channel/web_socket_channel.dart';

import 'connections.dart';
import 'model.dart';

/// Why a host cannot be talked to; the UI words it in the user's language.
enum LinkError { unreachable, lost, notConnected, timeout }

class LinkException implements Exception {
  const LinkException(this.error);

  final LinkError error;

  @override
  String toString() => error.name;
}

/// Looks a paired host up on the network (mDNS), in case its IP changed.
typedef Locate = Future<String?> Function(Connection connection);

/// A WebSocket to a paired host that reconnects with backoff and, after repeated failures,
/// asks [locate] where the host went.
class HostSocket {
  HostSocket(this.connection, {required this.onMessage, required this.onState, required this.onMoved, required this.locate});

  Connection connection;
  final void Function(Map<String, dynamic> message) onMessage;
  final void Function(bool connected, LinkError? error) onState;
  final Future<void> Function(Connection moved) onMoved;
  final Locate locate;

  WebSocketChannel? _ws;
  bool _closed = false;
  int _failures = 0;

  void start() => _connect();

  Future<void> _connect() async {
    if (_closed) return;
    final uri = Uri(
      scheme: 'ws',
      host: connection.host,
      port: connection.port,
      path: '/ws',
      queryParameters: {'token': connection.token},
    );
    final ws = WebSocketChannel.connect(uri);
    try {
      await ws.ready.timeout(const Duration(seconds: 5));
    } catch (e) {
      _lost(ws, LinkError.unreachable);
      return;
    }
    _ws = ws;
    _failures = 0;
    onState(true, null);
    ws.stream.listen(
      (raw) {
        if (raw is String) onMessage((jsonDecode(raw) as Map).cast<String, dynamic>());
      },
      onDone: () => _lost(ws, LinkError.lost),
      onError: (Object _) => _lost(ws, LinkError.lost),
      cancelOnError: true,
    );
  }

  void _lost(WebSocketChannel ws, LinkError why) {
    if (_ws != null && !identical(_ws, ws)) return;
    _ws = null;
    onState(false, why);
    if (_closed) return;
    _failures++;
    final delay = Duration(seconds: min(30, 1 << min(_failures - 1, 5)));
    Future.delayed(delay, () async {
      if (_closed) return;
      if (_failures >= 2) {
        final host = await locate(connection);
        if (host != null && host != connection.host) {
          connection = connection.withHost(host);
          await onMoved(connection);
        }
      }
      await _connect();
    });
  }

  bool send(Object message) {
    final ws = _ws;
    if (ws == null) return false;
    ws.sink.add(jsonEncode(message));
    return true;
  }

  void close() {
    _closed = true;
    _ws?.sink.close();
  }
}

class CoreClient extends ChangeNotifier {
  CoreClient(Connection connection, {required Future<void> Function(Connection) onMoved, required Locate locate}) {
    _socket = HostSocket(connection, onMessage: _onMessage, onState: _onState, onMoved: onMoved, locate: locate)..start();
  }

  /// No socket: for a local data source (the demo).
  CoreClient.detached();

  /// True when the entities are not a real home (the demo): the UI says so.
  bool get synthetic => false;

  HostSocket? _socket;
  final entities = <String, Entity>{};
  bool connected = false;
  LinkError? error;
  int _nextId = 0;
  final _calls = <int, Completer<Object?>>{};

  Connection get connection => _socket!.connection;

  List<String> get rooms => entities.values.map((e) => e.room).nonNulls.toSet().toList()..sort();

  /// Room kinds the user chose, by room name; the others are inferred from the name.
  final roomKinds = <String, String>{};

  /// The core's automations (written in its homemaid.toml), with their last runs.
  List<Automation> automations = const [];

  Future<Object?> call(String method, [Object? params]) {
    final id = ++_nextId;
    final completer = Completer<Object?>();
    _calls[id] = completer;
    if (!(_socket?.send({'jsonrpc': '2.0', 'id': id, 'method': method, 'params': params}) ?? false)) {
      _calls.remove(id);
      return Future.error(const LinkException(LinkError.notConnected));
    }
    return completer.future.timeout(const Duration(seconds: 10), onTimeout: () {
      _calls.remove(id);
      throw const LinkException(LinkError.timeout);
    });
  }

  Future<void> setDesired(String id, String cap, Object? value) =>
      call('entity.set_desired', {'id': id, 'cap': cap, 'value': capValue(cap, value)});

  /// An empty room removes it; an empty model goes back to the inferred one.
  Future<void> update(String id, {String? name, String? room, String? model}) =>
      call('entity.update', {'id': id, 'name': ?name, 'room': ?room, 'model': ?model});

  /// Runs an entity's one-shot action now.
  Future<void> invoke(String id, String action, [Map<String, Object?>? params]) =>
      call('entity.invoke', {'id': id, 'action': action, 'params': ?params});

  Future<void> refreshAutomations() async {
    final list = await call('automations.list') as List;
    automations = [for (final a in list) Automation.fromJson((a as Map).cast())];
    notifyListeners();
  }

  Future<DryRun> dryRun(String id) async => DryRun.fromJson((await call('automation.dry_run', {'id': id}) as Map).cast());

  Future<Run> runAutomation(String id) async {
    final run = Run.fromJson((await call('automation.run', {'id': id}) as Map).cast());
    await refreshAutomations();
    return run;
  }

  /// Saved in homemaid.toml; an empty kind goes back to the inferred one.
  Future<void> updateRoom(String name, String kind) => call('room.update', {'name': name, 'kind': kind});

  void _onState(bool up, LinkError? why) {
    connected = up;
    error = why;
    if (up) {
      _resync();
    } else {
      for (final call in _calls.values) {
        call.completeError(LinkException(why ?? LinkError.lost));
      }
      _calls.clear();
    }
    notifyListeners();
  }

  Future<void> _resync() async {
    try {
      await call('subscribe');
      final list = await call('entities.list') as List;
      final rooms = await call('rooms.list').catchError((Object _) => <Object?>[]) as List;
      roomKinds
        ..clear()
        ..addEntries(rooms.map((r) => MapEntry((r as Map)['name'] as String, r['kind'] as String)));
      // An older core has no automations: an empty list.
      await refreshAutomations().catchError((Object _) {});
      entities
        ..clear()
        ..addEntries(list.map((json) {
          final entity = Entity.fromJson((json as Map).cast<String, dynamic>());
          return MapEntry(entity.id, entity);
        }));
      notifyListeners();
    } catch (_) {
      // Lost again: the socket reconnects and resyncs.
    }
  }

  void _onMessage(Map<String, dynamic> message) {
    if (message['id'] case final int id) {
      final call = _calls.remove(id);
      final error = message['error'];
      if (error is Map) {
        call?.completeError(error['message'] ?? 'erreur');
      } else {
        call?.complete(message['result']);
      }
      return;
    }
    final params = (message['params'] as Map?)?.cast<String, dynamic>();
    if (params == null) return;
    switch (message['method']) {
      case 'state.changed':
        final entity = entities[params['id']];
        if (entity == null) return;
        entity.caps[params['cap'] as String] = Slot.fromJson((params['slot'] as Map).cast<String, dynamic>());
      case 'entity.changed':
        final entity = Entity.fromJson((params['entity'] as Map).cast<String, dynamic>());
        entities[entity.id] = entity;
      case 'automation.ran':
        // The run comes with its automation's new history and next time: read them again.
        refreshAutomations().catchError((Object _) {});
        return;
      case 'room.changed':
        final name = params['name'] as String;
        if (params['kind'] case final String kind) {
          roomKinds[name] = kind;
        } else {
          roomKinds.remove(name);
        }
      default:
        return;
    }
    notifyListeners();
  }

  @override
  void dispose() {
    _socket?.close();
    super.dispose();
  }
}

/// A service on the remote (`[[agent.shortcut]]`): its logo and the playback actions it has keys for.
class Shortcut {
  const Shortcut({required this.name, this.icon, this.actions = const {}});

  Shortcut.fromJson(Map<String, dynamic> j)
      : name = j['name'] as String,
        icon = j['icon'] as String?,
        actions = {...?(j['actions'] as List?)?.cast<String>()};

  final String name;
  final String? icon;
  final Set<String> actions;
}

/// The PC remote: requests are fire-and-forget, only errors come back.
class AgentClient extends ChangeNotifier {
  AgentClient(Connection connection, {required Future<void> Function(Connection) onMoved, required Locate locate})
      : hostname = connection.id {
    _socket = HostSocket(connection, onMessage: _onMessage, onState: _onState, onMoved: onMoved, locate: locate)..start();
  }

  /// No socket: requests go nowhere (the demo).
  AgentClient.detached({required this.hostname, this.apps = const [], this.shortcuts = const [], this.volume, this.muted})
      : connected = true;

  HostSocket? _socket;
  String hostname;
  List<String> apps = const [];
  List<Shortcut> shortcuts = const [];

  /// The service the playback buttons drive: the last one opened.
  String? active;
  bool connected = false;
  LinkError? linkError;

  /// Last request the agent refused, in its own words.
  String? refused;

  /// The PC's master volume (0-100) and mute, as last read; null until it answers.
  int? volume;
  bool? muted;

  Connection get connection => _socket!.connection;

  void send(Map<String, Object> request) => _socket?.send(request);

  /// Shown at once, then sent: the agent answers nothing on success.
  void setVolume(int level) {
    volume = level;
    send({'type': 'set_volume', 'level': level});
    notifyListeners();
  }

  void setMuted(bool value) {
    muted = value;
    send({'type': 'set_mute', 'muted': value});
    notifyListeners();
  }

  /// Opens a service and makes it the one the playback buttons drive.
  void open(Shortcut shortcut) {
    active = shortcut.name;
    send({'type': 'open', 'shortcut': shortcut.name});
    notifyListeners();
  }

  Shortcut? get activeShortcut => shortcuts.where((s) => s.name == active).firstOrNull;

  /// Asks for the PC's state (volume, mute): it comes back as a `state` message.
  void readState() => send({'type': 'state'});

  void _onState(bool up, LinkError? why) {
    connected = up;
    linkError = why;
    notifyListeners();
  }

  void _onMessage(Map<String, dynamic> message) {
    switch (message['type']) {
      case 'hello':
        hostname = message['hostname'] as String? ?? hostname;
        apps = (message['apps'] as List?)?.cast<String>() ?? const [];
        shortcuts = [for (final s in (message['shortcuts'] as List?) ?? const []) Shortcut.fromJson((s as Map).cast())];
        if (!shortcuts.any((s) => s.name == active)) active = null;
        readState();
      case 'state':
        final state = (message['state'] as Map?) ?? const {};
        volume = (state['volume'] as num?)?.round();
        muted = state['muted'] as bool?;
      case 'error':
        refused = message['message'] as String?;
      default:
        return;
    }
    notifyListeners();
  }

  @override
  void dispose() {
    _socket?.close();
    super.dispose();
  }
}
