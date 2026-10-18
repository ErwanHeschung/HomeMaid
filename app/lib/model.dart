/// Mirrors the core's JSON (`entities.list` and notifications). Capability values stay JSON,
/// like the plugin ABI: `{"on_off": true}`, `{"power": {"watts": 4.2}}`, `{"custom": 12}`.
library;

class Slot {
  Slot({required this.meta, this.observed, this.desired, required this.state, this.reason});

  factory Slot.fromJson(Map<String, dynamic> json) {
    final status = (json['status'] as Map?)?.cast<String, dynamic>() ?? const {};
    return Slot(
      meta: (json['meta'] as Map?)?.cast<String, dynamic>() ?? const {},
      observed: json['observed'],
      desired: json['desired'],
      state: status['state'] as String? ?? 'idle',
      reason: status['reason'] as String?,
    );
  }

  final Map<String, dynamic> meta;
  final Object? observed;
  final Object? desired;

  /// `idle`, `pending`, `confirmed` or `failed`.
  final String state;
  final String? reason;

  bool get readOnly => meta['read_only'] == true;
  bool get pending => state == 'pending';
  bool get failed => state == 'failed';
  num? get min => meta['min'] as num?;
  num? get max => meta['max'] as num?;
  String? get unit => meta['unit'] as String?;
  Map<String, dynamic>? get schema => (meta['schema'] as Map?)?.cast<String, dynamic>();

  /// What to display: the desired value while its command is pending, else the device's state.
  Object? get shown => inner(pending ? desired : (observed ?? desired));
}

/// `{"on_off": true}` → `true`. Every capability value is a single-key object.
Object? inner(Object? capValue) => capValue is Map && capValue.length == 1 ? capValue.values.first : null;

/// The JSON value the core expects for `kind`: `custom:x` capabilities share the `custom` key.
Map<String, Object?> capValue(String kind, Object? value) => {kind.startsWith('custom:') ? 'custom' : kind: value};

class Entity {
  Entity({
    required this.id,
    required this.name,
    this.room,
    this.tags = const [],
    required this.available,
    required this.plugin,
    this.device,
    this.model,
    required this.caps,
  });

  factory Entity.fromJson(Map<String, dynamic> json) => Entity(
        id: json['id'] as String,
        name: json['name'] as String? ?? '',
        room: json['room'] as String?,
        tags: (json['tags'] as List?)?.cast<String>() ?? const [],
        available: json['available'] as bool? ?? false,
        plugin: json['plugin'] as String? ?? '',
        device: json['device'] as String?,
        model: json['model'] as String?,
        caps: {
          for (final MapEntry(:key, :value) in ((json['caps'] as Map?) ?? const {}).entries)
            key as String: Slot.fromJson((value as Map).cast<String, dynamic>()),
        },
      );

  final String id;
  final String name;
  final String? room;
  final List<String> tags;
  final bool available;
  final String plugin;
  final String? device;

  /// The 3D model chosen by the user; null lets the app infer one from the capabilities.
  final String? model;
  final Map<String, Slot> caps;

  /// Controls first, then sensors, each group by name.
  List<MapEntry<String, Slot>> get orderedCaps => caps.entries.toList()
    ..sort((a, b) => a.value.readOnly == b.value.readOnly
        ? a.key.compareTo(b.key)
        : (a.value.readOnly ? 1 : -1));
}

/// Entities grouped by room, rooms alphabetically, those without one last (key `null`).
List<MapEntry<String?, List<Entity>>> byRoom(Iterable<Entity> entities) {
  final groups = <String?, List<Entity>>{};
  for (final entity in entities) {
    groups.putIfAbsent(entity.room, () => []).add(entity);
  }
  for (final list in groups.values) {
    list.sort((a, b) => a.name.toLowerCase().compareTo(b.name.toLowerCase()));
  }
  return groups.entries.toList()
    ..sort((a, b) => a.key == null
        ? 1
        : b.key == null
            ? -1
            : a.key!.toLowerCase().compareTo(b.key!.toLowerCase()));
}
