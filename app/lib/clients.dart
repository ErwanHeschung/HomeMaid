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
      // Not left half-open on either side: a late connection would linger, unused.
      ws.sink.close().ignore();
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

  /// Where the home is, for sunrise and sunset; null until set (`home.location`).
  ({double latitude, double longitude})? location;

  Future<void> refreshLocation() async {
    final at = await call('home.location') as Map?;
    location = at == null ? null : (latitude: (at['latitude'] as num).toDouble(), longitude: (at['longitude'] as num).toDouble());
    notifyListeners();
  }

  /// Saved in homemaid.toml; sunrise and sunset work at once.
  Future<void> setLocation(double latitude, double longitude) async {
    await call('home.set_location', {'latitude': latitude, 'longitude': longitude});
    location = (latitude: latitude, longitude: longitude);
    notifyListeners();
  }

  Future<Object?> call(String method, [Object? params]) => _call(method, params, const Duration(seconds: 10));

  Future<Object?> _call(String method, Object? params, Duration timeout) {
    final id = ++_nextId;
    final completer = Completer<Object?>();
    _calls[id] = completer;
    if (!(_socket?.send({'jsonrpc': '2.0', 'id': id, 'method': method, 'params': params}) ?? false)) {
      _calls.remove(id);
      return Future.error(const LinkException(LinkError.notConnected));
    }
    return completer.future.timeout(timeout, onTimeout: () {
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

  /// What an automation not saved yet would do now.
  Future<DryRun> dryRunConfig(Map<String, dynamic> config) async =>
      DryRun.fromJson((await call('automation.dry_run', {'automation': config}) as Map).cast());

  /// Created or replaced (by `id`) in homemaid.toml, in force at once. The core refuses an
  /// automation it would refuse at startup, with why.
  Future<void> saveAutomation(Map<String, dynamic> config) async {
    await call('automation.save', {'automation': config});
    await refreshAutomations();
  }

  Future<void> deleteAutomation(String id) async {
    await call('automation.delete', {'id': id});
    await refreshAutomations();
  }

  Future<Run> runAutomation(String id) async {
    final run = Run.fromJson((await call('automation.run', {'id': id}) as Map).cast());
    await refreshAutomations();
    return run;
  }

  /// The hub's plugins; read when the Plugins screen opens, and again on `plugins.changed`.
  List<InstalledPlugin> plugins = const [];

  Future<void> refreshPlugins() async {
    final list = await call('plugins.list') as List;
    plugins = [for (final p in list) InstalledPlugin.fromJson((p as Map).cast())];
    notifyListeners();
  }

  /// What the registry offers: the hub reads it on the Internet.
  Future<List<RegistryPlugin>> registry() async {
    final list = await _call('registry.list', null, const Duration(seconds: 70)) as List;
    return [for (final p in list) RegistryPlugin.fromJson((p as Map).cast())];
  }

  /// Installs or updates `plugin` at the version and with the permissions the user was shown.
  /// The hub downloads it and checks its signature; settings left out keep their value.
  Future<void> installPlugin(RegistryPlugin plugin, Map<String, String> settings) async {
    await _call(
      'plugin.install',
      {'id': plugin.id, 'version': plugin.version, 'permissions': plugin.permissions, 'settings': settings},
      const Duration(minutes: 3),
    );
    await refreshPlugins();
  }

  /// New settings, the plugin restarts with them; an empty secret keeps the old one.
  Future<void> configurePlugin(String id, Map<String, String> settings) async {
    await _call('plugin.configure', {'id': id, 'settings': settings}, const Duration(seconds: 90));
    await refreshPlugins();
  }

  /// Uninstalls it; its devices go with it.
  Future<void> removePlugin(String id) async {
    await _call('plugin.remove', {'id': id}, const Duration(seconds: 90));
    await refreshPlugins();
  }

  /// Adds a device to `plugin` from its form; the plugin checks it first (its refusal is the
  /// error). An empty secret takes the plugin's last added device's.
  Future<void> addDevice(String plugin, Map<String, String> settings) async {
    await _call('device.add', {'plugin': plugin, 'settings': settings}, const Duration(seconds: 90));
    await refreshPlugins();
  }

  /// Removes a device added with [addDevice], and its entities.
  Future<void> removeDevice(String plugin, String id) async {
    await _call('device.remove', {'plugin': plugin, 'id': id}, const Duration(seconds: 90));
    await refreshPlugins();
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
      await _reloadEntities();
      final rooms = await call('rooms.list').catchError((Object _) => <Object?>[]) as List;
      roomKinds
        ..clear()
        ..addEntries(rooms.map((r) => MapEntry((r as Map)['name'] as String, r['kind'] as String)));
      // An older core has no automations, nor a location: none.
      await refreshAutomations().catchError((Object _) {});
      await refreshLocation().catchError((Object _) {});
      notifyListeners();
    } catch (_) {
      // Lost again: the socket reconnects and resyncs.
    }
  }

  Future<void> _reloadEntities() async {
    final list = await call('entities.list') as List;
    // Applied at once: the changes notified from here on are newer than this list.
    entities
      ..clear()
      ..addEntries(list.map((json) {
        final entity = Entity.fromJson((json as Map).cast<String, dynamic>());
        return MapEntry(entity.id, entity);
      }));
    notifyListeners();
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
      case 'automations.changed':
        // Edited, maybe from another phone, or the location changed.
        refreshAutomations().catchError((Object _) {});
        refreshLocation().catchError((Object _) {});
        return;
      case 'plugins.changed':
        // Installed, updated, removed (its devices with it), started or failed.
        refreshPlugins().catchError((Object _) {});
        _reloadEntities().catchError((Object _) {});
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

/// A service on the remote (`[[agent.shortcut]]`): its logo, what it opens, the keys of its
/// player, and the playback actions it has keys for.
class Shortcut {
  const Shortcut({required this.name, this.icon, this.open, this.keys = const {}, this.actions = const {}});

  Shortcut.fromJson(Map<String, dynamic> j)
      : name = j['name'] as String,
        icon = j['icon'] as String?,
        open = j['open'] as String?,
        keys = {...?(j['keys'] as Map?)?.cast<String, String>()},
        actions = {...?(j['actions'] as List?)?.cast<String>()};

  final String name;
  final String? icon;
  final String? open;
  final Map<String, String> keys;
  final Set<String> actions;

  Map<String, Object> toJson() => {'name': name, 'icon': ?icon, 'open': ?open, if (keys.isNotEmpty) 'keys': keys};
}

/// The PC remote: requests are fire-and-forget, only errors come back.
class AgentClient extends ChangeNotifier {
  AgentClient(Connection connection, {required Future<void> Function(Connection) onMoved, required Locate locate})
      : hostname = connection.id {
    _socket = HostSocket(connection, onMessage: _onMessage, onState: _onState, onMoved: onMoved, locate: locate)..start();
  }

  /// No socket: requests go nowhere, the changes stay in memory (the demo).
  AgentClient.detached({
    required this.hostname,
    this.apps = const [],
    this.shortcuts = const [],
    this.volume,
    this.muted,
    List<String> startApps = const [],
    this.active,
    this.playing,
  })  : connected = true,
        discover = startApps.isNotEmpty,
        _startApps = startApps;

  List<String> _startApps = const [];

  HostSocket? _socket;
  String hostname;

  /// Written in `[agent.apps]` or pinned: the remote's strip.
  List<String> apps = const [];
  List<Shortcut> shortcuts = const [];

  /// The PC offers its Start menu's apps (`[agent] discover_apps`); they come with [listApps].
  bool discover = false;
  List<String>? discovered;

  /// The launcher (a service or an app) the remote is on: the one in front on the PC, as the PC
  /// reads it, or the last one opened. The playback buttons send its keys when it has some (a
  /// service), otherwise media requests the PC sends to what plays.
  String? active;

  /// What the PC's media sessions say plays; null when nothing does.
  NowPlaying? playing;
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

  /// Launches an app: the playback buttons send it the media keys and arrows.
  void launch(String app) {
    active = app;
    send({'type': 'launch', 'app': app});
    notifyListeners();
  }

  Shortcut? get activeShortcut => shortcuts.where((s) => s.name == active).firstOrNull;

  /// Asks for the PC's state (volume, mute): it comes back as a `state` message.
  void readState() => send({'type': 'state'});

  /// Where the PC draws an app's Start-menu icon, and the header it wants; none in the demo.
  ({Uri uri, Map<String, String> headers})? iconOf(String app) {
    final socket = _socket;
    return socket == null ? null : iconRequest(socket.connection, app);
  }

  /// Asks for the Start menu's apps: they come back as an `apps` message.
  void listApps() {
    if (_socket == null) {
      discovered = _startApps;
      notifyListeners();
    }
    send({'type': 'list_apps'});
  }

  // The PC writes these to its homemaid.toml, then its agent restarts to read them: the socket
  // reconnects and the hello brings the new strip.

  /// A site (a web address), created or replaced by name.
  void saveShortcut(Shortcut shortcut) => _change({'type': 'save_shortcut', 'shortcut': shortcut.toJson()}, () {
        shortcuts = [...shortcuts.where((s) => s.name != shortcut.name), shortcut];
      });

  void deleteShortcut(String name) => _change({'type': 'delete_shortcut', 'name': name}, () {
        shortcuts = shortcuts.where((s) => s.name != name).toList();
      });

  /// An app of the Start menu on the strip, or off it.
  void pinApp(String name) => _change({'type': 'pin_app', 'name': name}, () => apps = {...apps, name}.toList());

  void unpinApp(String name) => _change({'type': 'unpin_app', 'name': name}, () => apps = apps.where((a) => a != name).toList());

  void _change(Map<String, Object> request, void Function() inMemory) {
    refused = null;
    if (_socket == null) {
      inMemory();
      notifyListeners();
    }
    send(request);
  }

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
        discover = message['discover'] == true;
        shortcuts = [for (final s in (message['shortcuts'] as List?) ?? const []) Shortcut.fromJson((s as Map).cast())];
        if (!shortcuts.any((s) => s.name == active)) active = null;
        readState();
      case 'state':
        final state = (message['state'] as Map?) ?? const {};
        volume = (state['volume'] as num?)?.round();
        muted = state['muted'] as bool?;
        // The remote follows the PC: a window none of the launchers shows gets the generic keys.
        if (state.containsKey('launcher')) active = state['launcher'] as String?;
        final now = state['playing'] as Map?;
        playing = now == null ? null : NowPlaying(now['title'] as String? ?? '', now['artist'] as String? ?? '', playing: now['playing'] == true);
      case 'apps':
        discovered = (message['apps'] as List?)?.cast<String>() ?? const [];
      case 'saved':
        // The agent restarts to read it; the new hello follows.
        return;
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

/// A media session on the PC: Spotify's song, a browser tab's video.
class NowPlaying {
  NowPlaying(this.title, this.artist, {required this.playing});

  final String title;
  final String artist;

  /// Flipped at once on a press; the PC's next state says what really is.
  bool playing;
}

/// `GET /icon?app=` on the PC's agent, with its token in the header (never in the address).
({Uri uri, Map<String, String> headers}) iconRequest(Connection agent, String app) => (
      uri: Uri(scheme: 'http', host: agent.host, port: agent.port, path: '/icon', queryParameters: {'app': app}),
      headers: {'Authorization': 'Bearer ${agent.token}'},
    );
