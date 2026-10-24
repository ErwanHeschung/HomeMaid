/// A synthetic home for demos and design review, built with `--dart-define=HOMEMAID_DEMO=true`.
/// Every model archetype and every state appears, and the UI labels it as demo data: none of
/// these devices exist. Commands are simulated: pending, then confirmed (or failed, for one).
library;

import 'dart:ui';

import 'clients.dart';
import 'connections.dart';
import 'flow.dart';
import 'model.dart';
import 'selectors.dart';

const demoMode = bool.fromEnvironment('HOMEMAID_DEMO');

/// The device that never confirms, to show a failed command.
const _stubborn = 'demo-garage';

final demoConnections = [
  const Connection(kind: 'core', id: 'DEMO', host: '192.0.2.10', port: 8123, token: 'demo'),
  const Connection(kind: 'agent', id: 'DEMO-PC', host: '192.0.2.10', port: 8124, token: 'demo'),
];

AgentClient demoAgent() => AgentClient.detached(
      hostname: 'DEMO-PC',
      apps: const ['Firefox', 'Notepad', 'VS Code'],
      shortcuts: const [
        Shortcut(name: 'Netflix', icon: 'netflix', actions: {'play_pause', 'back', 'forward', 'skip_intro'}),
        Shortcut(name: 'YouTube', icon: 'youtube', actions: {'play_pause', 'back', 'forward', 'restart', 'next_episode'}),
        Shortcut(name: 'Spotify', icon: 'spotify', actions: {'play_pause', 'back', 'forward'}),
        Shortcut(name: 'Twitch', icon: 'twitch', actions: {'play_pause', 'back', 'forward'}),
        Shortcut(name: 'Crunchyroll', icon: 'crunchyroll', actions: {'play_pause', 'back', 'forward', 'restart', 'next_episode', 'skip_intro'}),
      ],
      volume: 35,
      muted: false,
      startApps: const ['Calculatrice', 'Firefox', 'Notepad', 'OBS Studio', 'Paint', 'Steam', 'VLC media player', 'VS Code'],
      active: 'Crunchyroll',
      playing: NowPlaying('Frieren – Épisode 12', 'Crunchyroll', playing: true),
    );

class DemoCore extends CoreClient {
  DemoCore() : super.detached() {
    connected = true;
    for (final e in _home(_fr)) {
      entities[e.id] = e;
    }
    automations = _automations(_fr);
    scenes = [for (final c in _scenes(_fr)) _scene(c)];
    location = (latitude: 48.86, longitude: 2.35);
  }

  final _fr = PlatformDispatcher.instance.locale.languageCode == 'fr';

  @override
  Connection get connection => demoConnections.first;

  @override
  bool get synthetic => true;

  @override
  Future<void> setDesired(String id, String cap, Object? value) async {
    final entity = entities[id];
    final slot = entity?.caps[cap];
    if (entity == null || slot == null || !entity.available) return;
    final wanted = capValue(cap, value);
    entity.caps[cap] = Slot(meta: slot.meta, observed: slot.observed, desired: wanted, state: 'pending');
    notifyListeners();
    final stubborn = id == _stubborn;
    await Future<void>.delayed(Duration(milliseconds: stubborn ? 2600 : 900));
    entity.caps[cap] = stubborn
        ? Slot(meta: slot.meta, observed: slot.observed, desired: wanted, state: 'failed', reason: 'not confirmed after 5 attempts')
        : Slot(meta: slot.meta, observed: wanted, desired: wanted, state: 'confirmed');
    notifyListeners();
  }

  @override
  Future<void> invoke(String id, String action, [Map<String, Object?>? params]) =>
      Future<void>.delayed(const Duration(milliseconds: 300));

  @override
  Future<void> refreshAutomations() async {}

  @override
  Future<DryRun> dryRun(String id) async {
    await Future<void>.delayed(const Duration(milliseconds: 250));
    final known = _dryRuns(_fr)[id];
    if (known != null) return known;
    final a = automations.where((a) => a.id == id).firstOrNull;
    return a == null ? DryRun(wouldRun: false) : _walk(FlowGraph.of(a.config));
  }

  @override
  Future<Run> runAutomation(String id) async {
    final dry = await dryRun(id);
    final run = Run(
      at: DateTime.now(),
      trigger: 'manual',
      skipped: dry.wouldRun ? null : dry.conditions.where((c) => !c.ok).map((c) => c.reason).firstOrNull,
      results: dry.wouldRun
          ? [for (final a in dry.actions) for (final t in a.targets) RunResult(target: t.name, action: a.action.keys.first, ok: t.error == null, outcome: t.error ?? 'pending')]
          : const [],
      steps: dry.wouldRun ? [for (final p in dry.path) (node: p.node, ok: true, outcome: p.outcome)] : const [],
    );
    automations = [for (final a in automations) a.id == id ? a.withRuns([run, ...a.runs].take(20).toList()) : a];
    notifyListeners();
    return run;
  }

  @override
  Future<void> refreshScenes() async {}

  /// Its targets: what its sets match among the demo's devices, as the core resolves them.
  Scene _scene(Map<String, dynamic> config) => Scene(
        id: config['id'] as String,
        name: config['name'] as String? ?? config['id'] as String,
        room: config['room'] as String?,
        config: config,
        targets: [
          for (final set in (config['set'] as List).cast<Map<String, dynamic>>())
            for (final e in matching(entities.values, set['select'] as String) ?? const <Entity>[])
              if (e.caps.containsKey(set['cap'])) (entity: e.id, cap: set['cap'] as String, value: set['value']),
        ],
      );

  @override
  Future<void> applyScene(String id) async {
    final scene = scenes.firstWhere((s) => s.id == id);
    for (final t in scene.targets) {
      // Side by side, as the core sends them: each device confirms in its own time.
      setDesired(t.entity, t.cap, t.value).ignore();
    }
    // Then its actions, in order.
    for (final a in ((scene.config['invoke'] as List?) ?? const []).cast<Map<String, dynamic>>()) {
      for (final e in matching(entities.values, a['select'] as String) ?? const <Entity>[]) {
        await invoke(e.id, a['action'] as String, (a['params'] as Map?)?.cast<String, Object?>());
      }
    }
  }

  /// Kept in memory only, like automations.
  @override
  Future<void> saveScene(Map<String, dynamic> config) async {
    await Future<void>.delayed(const Duration(milliseconds: 250));
    if ((config['set'] as List?)?.isNotEmpty != true && (config['invoke'] as List?)?.isNotEmpty != true) {
      throw 'scene `${config['id']}`: needs at least one [[scene.set]] or [[scene.invoke]]';
    }
    final saved = _scene(config);
    scenes = scenes.any((s) => s.id == saved.id) ? [for (final s in scenes) s.id == saved.id ? saved : s] : [...scenes, saved];
    notifyListeners();
  }

  @override
  Future<void> deleteScene(String id) async {
    scenes = scenes.where((s) => s.id != id).toList();
    notifyListeners();
  }

  @override
  Future<void> refreshLocation() async {}

  @override
  Future<void> setLocation(double latitude, double longitude) async {
    location = (latitude: latitude, longitude: longitude);
    notifyListeners();
  }

  /// Kept in memory only: the demo has no homemaid.toml.
  @override
  Future<void> saveAutomation(Map<String, dynamic> config) async {
    await Future<void>.delayed(const Duration(milliseconds: 250));
    if ((config['trigger'] as List?)?.isNotEmpty != true || FlowGraph.of(config).nodes.isEmpty) {
      throw 'automation `${config['id']}`: needs at least one [[automation.trigger]] and one [[automation.node]]';
    }
    final id = config['id'] as String;
    final old = automations.where((a) => a.id == id).firstOrNull;
    final saved = Automation.fromConfig(config, runs: old?.runs ?? const []);
    automations = old == null ? [...automations, saved] : [for (final a in automations) a.id == id ? saved : a];
    notifyListeners();
  }

  @override
  Future<void> stopAutomation(String id) async {
    final stopped = [
      for (final r in automations.where((a) => a.id == id).expand((a) => a.running))
        Run(at: r.at, trigger: r.trigger, stopped: 'by hand', steps: r.steps),
    ];
    automations = [for (final a in automations) a.id == id ? a.withRuns([...stopped, ...a.runs], running: const []) : a];
    notifyListeners();
  }

  @override
  Future<void> deleteAutomation(String id) async {
    automations = automations.where((a) => a.id != id).toList();
    notifyListeners();
  }

  @override
  Future<DryRun> dryRunConfig(Map<String, dynamic> config) async {
    await Future<void>.delayed(const Duration(milliseconds: 250));
    return _walk(FlowGraph.of(config));
  }

  /// A dry run of a flow, roughly: every `if` takes its first way, every wait goes on at once.
  DryRun _walk(FlowGraph graph) {
    final path = <String>[];
    final todo = [...graph.start.reversed];
    while (todo.isNotEmpty) {
      final id = todo.removeLast();
      final node = graph.node(id);
      if (node == null || path.contains(id)) continue;
      path.add(id);
      final kind = kindOf(node);
      final ports = switch (kind) {
        'if' => const ['then'],
        'repeat' => const ['body', 'next'],
        _ => const ['next'],
      };
      for (final port in ports.reversed) {
        todo.addAll(graph.targets(id, port).reversed);
      }
    }
    final actions = [for (final id in path) graph.node(id)!].where((n) => n.containsKey('set') || n.containsKey('invoke'));
    return DryRun(wouldRun: true, path: [for (final id in path) (node: id, ok: true, outcome: '')], actions: [
      for (final a in actions)
        (
          action: a,
          targets: [
            for (final e in matching(entities.values, ((a['set'] ?? a['invoke']) as Map)['select'] as String) ?? const <Entity>[])
              (name: e.name, changes: true, error: null),
          ],
        ),
    ]);
  }

  @override
  Future<void> updateRoom(String name, String kind) async {
    if (kind.isEmpty) {
      roomKinds.remove(name);
    } else {
      roomKinds[name] = kind;
    }
    notifyListeners();
  }

  @override
  Future<void> update(String id, {String? name, String? room, String? model}) async {
    final e = entities[id];
    if (e == null) return;
    entities[id] = Entity(
      id: e.id,
      name: name == null || name.isEmpty ? e.name : name,
      room: room == null ? e.room : (room.isEmpty ? null : room),
      tags: e.tags,
      available: e.available,
      plugin: e.plugin,
      device: e.device,
      added: e.added,
      model: model == null ? e.model : (model.isEmpty ? null : model),
      caps: e.caps,
      actions: e.actions,
    );
    notifyListeners();
  }

  // Plugins, kept in memory: what the registry offers and what is installed.
  late final _offered = [
    RegistryPlugin(
      id: 'tplink',
      name: 'TP-Link Tapo',
      description: _fr ? 'Multiprises et prises Tapo, en local.' : 'Tapo power strips and plugs, locally.',
      deviceSettings: [
        PluginSetting(key: 'host', label: _fr ? 'Adresse IP de la multiprise' : 'IP address of the strip', required: true),
        PluginSetting(key: 'username', label: _fr ? 'E-mail du compte TP-Link' : 'TP-Link account e-mail', secret: true, required: true),
        PluginSetting(key: 'password', label: _fr ? 'Mot de passe TP-Link' : 'TP-Link password', secret: true, required: true),
      ],
      version: '0.2.0',
      permissions: const ['local-network'],
    ),
    RegistryPlugin(
      id: 'wake-on-lan',
      name: 'Wake-on-LAN',
      description: _fr ? 'Allume les PC du réseau.' : 'Turns on the PCs of the network.',
      settings: [PluginSetting(key: 'macs', label: _fr ? 'Adresses MAC' : 'MAC addresses', required: true)],
      version: '1.0.0',
      permissions: const ['local-network'],
    ),
    RegistryPlugin(id: 'future', name: _fr ? 'Plugin du futur' : 'Plugin from the future', description: _fr ? 'Fait pour une version plus récente.' : 'Built for a newer version.'),
  ];

  late final _installed = <String, InstalledPlugin>{
    'tplink': InstalledPlugin(
      id: 'tplink',
      name: 'TP-Link Tapo',
      fromRegistry: true,
      version: '0.1.0',
      permissions: const ['local-network'],
      deviceSettings: _offered[0].deviceSettings,
      devices: const [AddedDevice(id: 'demo-strip', settings: {'host': '192.168.1.50'}, secrets: ['username', 'password'])],
    ),
    'pc': const InstalledPlugin(id: 'pc', name: 'pc', fromRegistry: false, version: '0.1.0', permissions: ['local-network', 'loopback']),
  };

  @override
  Future<void> refreshPlugins() async {
    plugins = _installed.values.toList();
    notifyListeners();
  }

  @override
  Future<List<RegistryPlugin>> registry() async {
    await Future<void>.delayed(const Duration(milliseconds: 600));
    return [
      for (final p in _offered)
        RegistryPlugin(
          id: p.id,
          name: p.name,
          description: p.description,
          settings: p.settings,
          deviceSettings: p.deviceSettings,
          version: p.version,
          permissions: p.permissions,
          installed: _installed[p.id]?.version,
          update: _installed[p.id] != null && _installed[p.id]!.version != p.version,
        ),
    ];
  }

  @override
  Future<void> installPlugin(RegistryPlugin plugin, Map<String, String> settings) async {
    await Future<void>.delayed(const Duration(milliseconds: 1200));
    final old = _installed[plugin.id];
    for (final s in plugin.settings.where((s) => s.required)) {
      final kept = s.secret ? old?.secrets.contains(s.key) == true : old?.settings[s.key] != null;
      if ((settings[s.key] ?? '').trim().isEmpty && !kept) throw '${s.label} ${_fr ? 'est obligatoire' : 'is required'}';
    }
    _installed[plugin.id] = InstalledPlugin(
      id: plugin.id,
      name: plugin.name,
      description: plugin.description,
      fromRegistry: true,
      version: plugin.version,
      permissions: plugin.permissions,
      settings: {...?old?.settings, for (final s in plugin.settings.where((s) => !s.secret && settings[s.key] != null)) s.key: settings[s.key]!},
      secrets: {...?old?.secrets, for (final s in plugin.settings.where((s) => s.secret && settings[s.key] != null)) s.key}.toList(),
      declared: plugin.settings,
      deviceSettings: plugin.deviceSettings,
      devices: old?.devices ?? const [],
    );
    await refreshPlugins();
  }

  @override
  Future<void> configurePlugin(String id, Map<String, String> settings) async {
    final p = _installed[id]!;
    await installPlugin(
      RegistryPlugin(
        id: id,
        name: p.name,
        description: p.description ?? '',
        settings: p.declared,
        deviceSettings: p.deviceSettings,
        version: p.version,
        permissions: p.permissions,
      ),
      settings,
    );
  }

  @override
  Future<void> removePlugin(String id) async {
    await Future<void>.delayed(const Duration(milliseconds: 400));
    _installed.remove(id);
    await refreshPlugins();
  }

  /// A strip of three outlets, in no room yet; the address 0.0.0.0 stands for one that does not answer.
  @override
  Future<void> addDevice(String plugin, Map<String, String> settings) async {
    await Future<void>.delayed(const Duration(milliseconds: 900));
    final p = _installed[plugin]!;
    for (final s in p.deviceSettings.where((s) => s.required)) {
      final reused = s.secret && p.devices.isNotEmpty;
      if ((settings[s.key] ?? '').trim().isEmpty && !reused) throw '${s.label} ${_fr ? 'est obligatoire' : 'is required'}';
    }
    if (settings['host'] == '0.0.0.0') throw _fr ? '0.0.0.0 ne répond pas' : '0.0.0.0 does not answer';
    final id = 'demo-added-${p.devices.length + 1}';
    for (var i = 1; i <= 3; i++) {
      final e = Entity.fromJson({
        'id': '$id-$i', 'name': '${_fr ? 'Prise' : 'Outlet'} $i', 'available': true, 'plugin': plugin, 'device': id, 'added': id,
        'caps': _switch(false),
      });
      entities[e.id] = e;
    }
    _installed[plugin] = InstalledPlugin(
      id: p.id,
      name: p.name,
      description: p.description,
      fromRegistry: true,
      version: p.version,
      permissions: p.permissions,
      deviceSettings: p.deviceSettings,
      devices: [...p.devices, AddedDevice(id: id, settings: {'host': settings['host']!.trim()}, secrets: const ['username', 'password'])],
    );
    await refreshPlugins();
  }

  @override
  Future<void> removeDevice(String plugin, String id) async {
    await Future<void>.delayed(const Duration(milliseconds: 400));
    entities.removeWhere((_, e) => e.plugin == plugin && e.added == id);
    final p = _installed[plugin]!;
    _installed[plugin] = InstalledPlugin(
      id: p.id,
      name: p.name,
      description: p.description,
      fromRegistry: true,
      version: p.version,
      permissions: p.permissions,
      deviceSettings: p.deviceSettings,
      devices: p.devices.where((d) => d.id != id).toList(),
    );
    await refreshPlugins();
  }
}

Map<String, dynamic> _slot(Object? observed, {Object? desired, String state = 'confirmed', String? reason, Map<String, dynamic> meta = const {}}) => {
      'meta': meta,
      'observed': observed,
      'desired': desired ?? observed,
      'status': {'state': state, 'reason': ?reason},
    };

Entity _entity(String id, String name, String? room, Map<String, Map<String, dynamic>> caps,
        {String? device, String? added, bool available = true, String? model, String plugin = 'demo', List<String> actions = const []}) =>
    Entity.fromJson({
      'id': id, 'name': name, 'room': room, 'available': available, 'plugin': plugin, 'device': device, 'added': added, 'model': model,
      'caps': caps, 'actions': [for (final a in actions) {'name': a}],
    });

const _level = {'min': 0.0, 'max': 100.0, 'unit': '%'};
const _watts = {'read_only': true, 'unit': 'W'};

Map<String, Map<String, dynamic>> _switch(bool on, {String state = 'confirmed', bool? desired, String? reason}) =>
    {'on_off': _slot({'on_off': on}, desired: desired == null ? null : {'on_off': desired}, state: state, reason: reason)};

List<Entity> _home(bool fr) {
  String t(String french, String english) => fr ? french : english;
  final living = t('Salon', 'Living room'), office = t('Bureau', 'Office'), bedroom = t('Chambre', 'Bedroom');
  final kitchen = t('Cuisine', 'Kitchen'), entrance = t('Entrée', 'Entrance'), bathroom = t('Salle de bain', 'Bathroom');
  return [
    // A three-outlet strip: on, off, and a request still pending.
    _entity('demo-strip-1', t('Lampe d\'appoint', 'Side lamp'), living, _switch(true), device: 'demo-strip', added: 'demo-strip', plugin: 'tplink'),
    _entity('demo-strip-2', t('Télévision', 'TV'), living, _switch(false), device: 'demo-strip', added: 'demo-strip', plugin: 'tplink'),
    _entity('demo-strip-3', t('Console', 'Console'), living, _switch(false, desired: true, state: 'pending'), device: 'demo-strip', added: 'demo-strip', plugin: 'tplink'),
    // Lamps: lit at 70 %, and off.
    _entity('demo-ceiling', t('Plafonnier', 'Ceiling light'), living, {
      ..._switch(true),
      'brightness': _slot({'brightness': 70}, meta: _level),
    }, model: 'ceiling_light'),
    _entity('demo-floor', t('Lampadaire', 'Floor lamp'), living, {
      ..._switch(true),
      'brightness': _slot({'brightness': 45}, meta: _level),
    }, model: 'floor_lamp'),
    _entity('demo-speaker', t('Enceinte', 'Speaker'), living, {
      'volume': _slot({'volume': 20}, meta: _level),
      'mute': _slot({'mute': false}),
    }, model: 'speaker'),
    _entity('demo-tvset', t('Téléviseur', 'Television'), living, {
      'volume': _slot({'volume': 12}, meta: _level),
      'mute': _slot({'mute': true}),
    }, model: 'tv'),
    _entity('demo-reading', t('Liseuse', 'Reading lamp'), living, {
      ..._switch(false),
      'brightness': _slot({'brightness': 40}, meta: _level),
    }),
    // A PC behind its agent: volume, mute and sensors.
    _entity('demo-pc', 'DEMO-PC', office, {
      'volume': _slot({'volume': 35}, meta: _level),
      'mute': _slot({'mute': false}),
      'custom:cpu': _slot({'custom': 23.4}, meta: const {'read_only': true, 'unit': '%', 'schema': {'type': 'number'}}),
      'custom:memory': _slot({'custom': 61.0}, meta: const {'read_only': true, 'unit': '%', 'schema': {'type': 'number'}}),
      'custom:active_window': _slot({'custom': 'Visual Studio Code'}, meta: const {'read_only': true, 'schema': {'type': ['string', 'null']}}),
    }, plugin: 'pc', actions: const ['play_pause', 'previous', 'next', 'lock', 'sleep', 'shutdown']),
    _entity('demo-ac', t('Climatiseur', 'Air conditioner'), office, _switch(false), model: 'air_conditioner'),
    _entity('demo-thermostat', t('Thermostat', 'Thermostat'), office, {
      'custom:temperature': _slot({'custom': 20.5}, meta: const {'read_only': true, 'unit': '°C', 'schema': {'type': 'number'}}),
    }, model: 'thermostat'),
    _entity('demo-ledstrip', t('Ruban LED', 'LED strip'), office, {
      ..._switch(true),
      'brightness': _slot({'brightness': 80}, meta: _level),
    }, model: 'light_strip'),
    // A command the device never confirmed.
    _entity('demo-desk', t('Prise du bureau', 'Desk outlet'), office,
        _switch(false, desired: true, state: 'failed', reason: 'not confirmed after 5 attempts')),
    // Offline, and a heater drawing power.
    _entity('demo-bedside', t('Lampe de chevet', 'Bedside lamp'), bedroom, {
      ..._switch(false),
      'brightness': _slot({'brightness': 30}, meta: _level),
    }, available: false, model: 'bulb'),
    _entity('demo-heater', t('Radiateur d\'appoint', 'Space heater'), bedroom, {
      ..._switch(true),
      'power': _slot({'power': {'watts': 840.5}}, meta: _watts),
    }, model: 'radiator'),
    _entity('demo-fan', t('Ventilateur', 'Fan'), bedroom, _switch(false), model: 'fan'),
    _entity('demo-purifier', t('Purificateur', 'Air purifier'), bedroom, {
      ..._switch(true),
      'custom:pm25': _slot({'custom': 8}, meta: const {'read_only': true, 'unit': 'µg/m³', 'schema': {'type': 'number'}}),
    }, model: 'air_purifier'),
    _entity('demo-shutter', t('Volet', 'Shutter'), bedroom, {
      'custom:position': _slot({'custom': 60}, meta: const {'read_only': true, 'unit': '%', 'schema': {'type': 'number'}}),
    }, model: 'shutter'),
    // Seven devices: past what the viewport annotates, the list carries them.
    _entity('demo-kettle', t('Bouilloire', 'Kettle'), kitchen, {..._switch(true), 'power': _slot({'power': {'watts': 1850.0}}, meta: _watts)}, model: 'kettle'),
    _entity('demo-coffee', t('Cafetière', 'Coffee maker'), kitchen, _switch(false), model: 'coffee_maker'),
    _entity('demo-toaster', t('Grille-pain', 'Toaster'), kitchen, _switch(false)),
    _entity('demo-hood', t('Hotte', 'Range hood'), kitchen, _switch(true)),
    _entity('demo-dishwasher', t('Lave-vaisselle', 'Dishwasher'), kitchen, {
      ..._switch(true),
      'custom:program': _slot({'custom': 'Eco 50°'}, meta: const {'read_only': true, 'schema': {'type': 'string'}}),
      'custom:remaining': _slot({'custom': 42}, meta: const {'read_only': true, 'unit': 'min', 'schema': {'type': 'number'}}),
    }),
    _entity('demo-thermo', t('Thermomètre', 'Thermometer'), kitchen, {
      'custom:temperature': _slot({'custom': 21.5}, meta: const {'read_only': true, 'unit': '°C', 'schema': {'type': 'number'}}),
    }),
    _entity('demo-fridge', t('Réfrigérateur', 'Fridge'), kitchen, {..._switch(true), 'power': _slot({'power': {'watts': 96.2}}, meta: _watts)}, model: 'fridge'),
    // The entrance: security devices, sensors only (never pink).
    _entity('demo-lock', t('Serrure', 'Door lock'), entrance, {
      'custom:locked': _slot({'custom': true}, meta: const {'read_only': true, 'schema': {'type': 'boolean'}}),
    }, model: 'door_lock'),
    _entity('demo-doorbell', t('Sonnette', 'Doorbell'), entrance, {
      'custom:battery': _slot({'custom': 87}, meta: const {'read_only': true, 'unit': '%', 'schema': {'type': 'number'}}),
    }, model: 'doorbell'),
    _entity('demo-camera', t('Caméra', 'Camera'), entrance, _switch(true), model: 'camera'),
    _entity('demo-door', t('Porte d\'entrée', 'Front door'), entrance, {
      'custom:open': _slot({'custom': false}, meta: const {'read_only': true, 'schema': {'type': 'boolean'}}),
    }, model: 'contact_sensor'),
    _entity('demo-motion', t('Mouvement', 'Motion'), entrance, {
      'custom:motion': _slot({'custom': false}, meta: const {'read_only': true, 'schema': {'type': 'boolean'}}),
    }, model: 'motion_sensor'),
    _entity('demo-hub', t('Passerelle Zigbee', 'Zigbee hub'), entrance, {
      'custom:devices': _slot({'custom': 12}, meta: const {'read_only': true, 'schema': {'type': 'number'}}),
    }),
    // The bathroom.
    _entity('demo-washer', t('Lave-linge', 'Washing machine'), bathroom, {..._switch(true), 'power': _slot({'power': {'watts': 410.0}}, meta: _watts)}, model: 'washing_machine'),
    _entity('demo-smoke', t('Détecteur de fumée', 'Smoke alarm'), bathroom, {
      'custom:smoke': _slot({'custom': false}, meta: const {'read_only': true, 'schema': {'type': 'boolean'}}),
    }, model: 'smoke_detector'),
    _entity('demo-vacuum', t('Robot aspirateur', 'Robot vacuum'), bathroom, _switch(false), model: 'robot_vacuum'),
    _entity('demo-bulb', t('Ampoule', 'Bulb'), bathroom, {
      ..._switch(false),
      'brightness': _slot({'brightness': 60}, meta: _level),
    }, model: 'bulb'),
    // No room yet, and the stubborn one.
    _entity(_stubborn, t('Prise du garage', 'Garage outlet'), null, _switch(false)),
  ];
}

/// Scenes covering their states: one in force, one waiting for a device, one not.
List<Map<String, dynamic>> _scenes(bool fr) {
  String t(String french, String english) => fr ? french : english;
  final living = t('Salon', 'Living room');
  Map<String, dynamic> set(String id, String cap, Object value) => {'select': 'id:$id', 'cap': cap, 'value': value};
  return [
    {'id': 'evening', 'name': t('Soirée', 'Evening'), 'room': living, 'set': [set('demo-ceiling', 'on_off', true), set('demo-floor', 'on_off', true)]},
    {'id': 'games', 'name': t('Jeux', 'Games'), 'room': living, 'set': [set('demo-strip-3', 'on_off', true)]},
    {
      'id': 'movie',
      'name': t('Cinéma', 'Movie night'),
      'room': living,
      'set': [set('demo-ceiling', 'on_off', false), set('demo-floor', 'on_off', false)],
      'invoke': [{'select': 'plugin:pc', 'action': 'play_pause'}],
    },
    {'id': 'night', 'name': t('Nuit', 'Night'), 'set': [set('demo-ceiling', 'on_off', false), set('demo-floor', 'on_off', false), set('demo-reading', 'on_off', false)]},
  ];
}

/// Automations covering every case: a time trigger that acted, a state trigger, a run skipped by
/// its condition, a selector matching nothing (a problem), and a failed run.
List<Automation> _automations(bool fr) {
  String t(String french, String english) => fr ? french : english;
  final now = DateTime.now();
  DateTime next(int hour, int minute, {bool weekdays = false}) {
    var at = DateTime(now.year, now.month, now.day, hour, minute);
    while (!at.isAfter(now) || (weekdays && at.weekday > DateTime.friday)) {
      at = DateTime(at.year, at.month, at.day + 1, hour, minute);
    }
    return at;
  }

  // The last time a trigger at hour:minute fired (on a weekday when asked).
  DateTime last(int hour, int minute, {bool weekdays = false}) {
    var at = DateTime(now.year, now.month, now.day, hour, minute);
    while (!at.isBefore(now) || (weekdays && at.weekday > DateTime.friday)) {
      at = DateTime(at.year, at.month, at.day - 1, hour, minute);
    }
    return at;
  }

  DateTime ago(Duration d) => now.subtract(d);
  final living = t('Salon', 'Living room'), bedroom = t('Chambre', 'Bedroom');
  const week = ['mon', 'tue', 'wed', 'thu', 'fri'];
  return [
    Automation(
      id: 'evening',
      name: t('Lampe du soir', 'Evening lamp'),
      enabled: true,
      triggers: [{'at': '19:30', 'days': week}],
      conditions: [{'between': ['18:00', '01:00']}],
      actions: [{'set': {'select': 'room:"$living" & cap:brightness', 'cap': 'brightness', 'value': 40}}],
      nextRun: next(19, 30, weekdays: true),
      runs: [
        Run(at: last(19, 30, weekdays: true), trigger: '19:30', results: [
          RunResult(target: t('Plafonnier', 'Ceiling light'), action: 'set brightness 40', ok: true, outcome: 'confirmed'),
          RunResult(target: t('Lampadaire', 'Floor lamp'), action: 'set brightness 40', ok: true, outcome: 'confirmed'),
        ]),
      ],
    ),
    Automation(
      id: 'dusk',
      name: t('Lumière au crépuscule', 'Lights at dusk'),
      enabled: true,
      triggers: [{'at': 'sunset-15', 'days': [...week, 'sat', 'sun']}],
      actions: [{'set': {'select': 'room:"$living" & cap:on_off', 'cap': 'on_off', 'value': true}}],
      nextRun: next(18, 47),
      delegated: [t('Lampadaire', 'Floor lamp')],
      runs: [
        Run(at: last(18, 47), trigger: 'sunset-15 (18:47)', results: [
          RunResult(target: t('Lampadaire', 'Floor lamp'), action: 'set on_off true', ok: true, outcome: 'confirmed'),
        ]),
      ],
    ),
    Automation(
      id: 'pause-on-mute',
      name: t('Pause quand je coupe le son', 'Pause when muted'),
      enabled: true,
      triggers: [{'when': {'select': 'plugin:pc', 'cap': 'mute', 'becomes': true}}],
      actions: [{'invoke': {'select': 'plugin:pc', 'action': 'play_pause', 'params': null}}],
      runs: [
        Run(at: ago(const Duration(hours: 3, minutes: 12)), trigger: 'DEMO-PC mute', results: [
          const RunResult(target: 'DEMO-PC', action: 'invoke play_pause', ok: true, outcome: 'sent'),
        ]),
      ],
    ),
    Automation(
      id: 'heater-night',
      name: t('Radiateur la nuit', 'Heater at night'),
      enabled: true,
      triggers: [{'at': '22:00', 'days': [...week, 'sat', 'sun']}],
      conditions: [{'state': {'select': 'room:$bedroom & model:fan', 'cap': 'on_off', 'is': false}}],
      actions: [{'set': {'select': 'room:$bedroom & model:radiator', 'cap': 'on_off', 'value': true}}],
      nextRun: next(22, 0),
      runs: [
        Run(at: last(22, 0), trigger: '22:00', skipped: t('Ventilateur : allumé (ne passe pas)', 'Fan: on (does not pass)')),
      ],
    ),
    Automation(
      id: 'garage',
      name: t('Lumière du garage', 'Garage light'),
      enabled: true,
      triggers: [{'when': {'select': 'tag:garage-door', 'cap': 'custom:open', 'becomes': true}}],
      actions: [{'set': {'select': 'room:Garage & cap:on_off', 'cap': 'on_off', 'value': true}}],
      problems: const ['`tag:garage-door` matches no device', '`room:Garage & cap:on_off` matches no device'],
    ),
    // A workflow waiting now: on at sunset, then off once nobody moves, two hours at most.
    Automation(
      id: 'living-evening',
      name: t('Salon le soir', 'Living room at night'),
      enabled: true,
      triggers: [{'at': 'sunset'}],
      config: {
        'id': 'living-evening',
        'name': t('Salon le soir', 'Living room at night'),
        'mode': 'restart',
        'start': ['on'],
        'trigger': [{'at': 'sunset'}],
        'node': [
          {'id': 'on', 'set': {'select': 'room:"$living" & cap:on_off', 'cap': 'on_off', 'value': true}, 'next': ['still']},
          {
            'id': 'still',
            'wait': {'until': [{'state': {'select': 'cap:custom:motion', 'cap': 'custom:motion', 'is': false}}], 'timeout': '2h'},
            'next': ['off'],
            'timeout': ['off-late'],
          },
          {'id': 'off', 'set': {'select': 'room:"$living" & cap:on_off', 'cap': 'on_off', 'value': false}},
          {'id': 'off-late', 'set': {'select': 'room:"$living" & cap:brightness', 'cap': 'brightness', 'value': 10}},
        ],
      },
      running: [(at: ago(const Duration(minutes: 40)), trigger: 'sunset (18:47)', nodes: ['still'], steps: [(node: 'on', ok: true, outcome: 'pending')])],
      runs: [
        Run(at: ago(const Duration(days: 1, minutes: 30)), trigger: 'sunset (18:48)', results: [
          RunResult(target: t('Lampadaire', 'Floor lamp'), action: 'set on_off true', ok: true, outcome: 'confirmed'),
        ], steps: [
          (node: 'on', ok: true, outcome: 'confirmed'),
          (node: 'still', ok: true, outcome: 'passed'),
          (node: 'off', ok: true, outcome: 'confirmed'),
        ]),
      ],
    ),
    // Branches side by side, joined, then a choice and a loop; its last run took the failure way.
    Automation(
      id: 'leaving',
      name: t('Départ le matin', 'Leaving in the morning'),
      enabled: true,
      triggers: [{'at': '08:30', 'days': week}],
      config: {
        'id': 'leaving',
        'name': t('Départ le matin', 'Leaving in the morning'),
        'start': ['lights', 'pc'],
        'trigger': [{'at': '08:30', 'days': week}],
        'node': [
          {'id': 'lights', 'set': {'select': 'cap:on_off', 'cap': 'on_off', 'value': false}, 'next': ['join']},
          {'id': 'pc', 'invoke': {'select': 'plugin:pc', 'action': 'lock'}, 'next': ['join'], 'error': ['pc-failed']},
          {'id': 'pc-failed', 'stop': {'error': t('le PC ne répond pas', 'the PC does not answer')}},
          {'id': 'join', 'merge': 'all', 'next': ['heater']},
          {
            'id': 'heater',
            'if': [{'state': {'select': 'room:$bedroom & model:radiator', 'cap': 'on_off', 'is': true}}],
            'then': ['heater-off'],
            'else': ['check'],
          },
          {'id': 'heater-off', 'set': {'select': 'room:$bedroom & model:radiator', 'cap': 'on_off', 'value': false}},
          {
            'id': 'check',
            'repeat': {'until': [{'state': {'select': 'cap:on_off', 'cap': 'on_off', 'is': false, 'all': true}}]},
            'body': ['pause'],
          },
          {'id': 'pause', 'delay': '1m'},
        ],
      },
      nextRun: next(8, 30, weekdays: true),
      runs: [
        Run(at: last(8, 30, weekdays: true), trigger: '08:30', error: t('le PC ne répond pas', 'the PC does not answer'), results: [
          RunResult(target: t('Lampadaire', 'Floor lamp'), action: 'set on_off false', ok: true, outcome: 'confirmed'),
          const RunResult(target: 'DEMO-PC', action: 'invoke lock', ok: false, outcome: 'timed out'),
        ], steps: [
          (node: 'lights', ok: true, outcome: 'confirmed'),
          (node: 'pc', ok: false, outcome: 'DEMO-PC: timed out'),
          (node: 'pc-failed', ok: false, outcome: t('le PC ne répond pas', 'the PC does not answer')),
        ]),
      ],
    ),
    Automation(
      id: 'coffee',
      name: t('Café du matin', 'Morning coffee'),
      enabled: true,
      triggers: [{'at': '07:00', 'days': week}],
      actions: [{'set': {'select': 'model:coffee_maker', 'cap': 'on_off', 'value': true}}],
      nextRun: next(7, 0, weekdays: true),
      runs: [
        Run(at: last(7, 0, weekdays: true), trigger: '07:00', results: [
          RunResult(target: t('Cafetière', 'Coffee maker'), action: 'set on_off true', ok: false, outcome: t('non confirmée après 5 essais', 'not confirmed after 5 attempts')),
        ]),
      ],
    ),
  ];
}

Map<String, DryRun> _dryRuns(bool fr) {
  String t(String french, String english) => fr ? french : english;
  final now = DateTime.now();
  final evening = now.hour >= 18 || now.hour < 1;
  final time = '${now.hour.toString().padLeft(2, '0')}:${now.minute.toString().padLeft(2, '0')}';
  return {
    'evening': DryRun(
      wouldRun: evening,
      conditions: [(ok: evening, reason: t('$time ${evening ? 'est' : "n'est pas"} entre 18:00 et 01:00', '$time is ${evening ? '' : 'not '}between 18:00 and 01:00'))],
      actions: [
        (
          action: {'set': {'select': 'room:"${t('Salon', 'Living room')}" & cap:brightness', 'cap': 'brightness', 'value': 40}},
          targets: [(name: t('Plafonnier', 'Ceiling light'), changes: true, error: null), (name: t('Lampadaire', 'Floor lamp'), changes: true, error: null)],
        ),
      ],
    ),
    'pause-on-mute': DryRun(wouldRun: true, actions: [
      (action: {'invoke': {'select': 'plugin:pc', 'action': 'play_pause'}}, targets: [(name: 'DEMO-PC', changes: true, error: null)]),
    ]),
    'heater-night': DryRun(
      wouldRun: false,
      conditions: [(ok: false, reason: t('Ventilateur : allumé (ne passe pas)', 'Fan: on (does not pass)'))],
      actions: [
        (action: {'set': {'select': 'room:${t('Chambre', 'Bedroom')} & model:radiator', 'cap': 'on_off', 'value': true}}, targets: [(name: t('Radiateur d\'appoint', 'Space heater'), changes: false, error: null)]),
      ],
    ),
    'garage': DryRun(wouldRun: true, actions: [
      (action: {'set': {'select': 'room:Garage & cap:on_off', 'cap': 'on_off', 'value': true}}, targets: const []),
    ]),
    'coffee': DryRun(wouldRun: true, actions: [
      (action: {'set': {'select': 'model:coffee_maker', 'cap': 'on_off', 'value': true}}, targets: [(name: t('Cafetière', 'Coffee maker'), changes: true, error: null)]),
    ]),
  };
}
