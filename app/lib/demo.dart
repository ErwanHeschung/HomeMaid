/// A synthetic home for demos and design review, built with `--dart-define=HOMEMAID_DEMO=true`.
/// Every model archetype and every state appears, and the UI labels it as demo data: none of
/// these devices exist. Commands are simulated: pending, then confirmed (or failed, for one).
library;

import 'dart:ui';

import 'clients.dart';
import 'connections.dart';
import 'model.dart';

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
    );

class DemoCore extends CoreClient {
  DemoCore() : super.detached() {
    connected = true;
    for (final e in _home(PlatformDispatcher.instance.locale.languageCode == 'fr')) {
      entities[e.id] = e;
    }
  }

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
      model: model == null ? e.model : (model.isEmpty ? null : model),
      caps: e.caps,
    );
    notifyListeners();
  }
}

Map<String, dynamic> _slot(Object? observed, {Object? desired, String state = 'confirmed', String? reason, Map<String, dynamic> meta = const {}}) => {
      'meta': meta,
      'observed': observed,
      'desired': desired ?? observed,
      'status': {'state': state, 'reason': ?reason},
    };

Entity _entity(String id, String name, String? room, Map<String, Map<String, dynamic>> caps,
        {String? device, bool available = true, String? model}) =>
    Entity.fromJson({'id': id, 'name': name, 'room': room, 'available': available, 'plugin': 'demo', 'device': device, 'model': model, 'caps': caps});

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
    _entity('demo-strip-1', t('Lampe d\'appoint', 'Side lamp'), living, _switch(true), device: 'demo-strip'),
    _entity('demo-strip-2', t('Télévision', 'TV'), living, _switch(false), device: 'demo-strip'),
    _entity('demo-strip-3', t('Console', 'Console'), living, _switch(false, desired: true, state: 'pending'), device: 'demo-strip'),
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
    }),
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
    // The entrance: security devices, sensors only (never amber).
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
