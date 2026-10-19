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
    this.actions = const [],
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
        actions: [
          for (final a in (json['actions'] as List?) ?? const [])
            ActionDecl(name: (a as Map)['name'] as String, params: (a['params'] as Map?)?.cast<String, dynamic>()),
        ],
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

  /// One-shot actions (media next, lock...): not states, nothing to confirm.
  final List<ActionDecl> actions;

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

/// An action an entity offers (plugin-api 1.1); [params] is its JSON schema, null for none.
class ActionDecl {
  const ActionDecl({required this.name, this.params});

  final String name;
  final Map<String, dynamic>? params;
}

/// An automation from the core's `homemaid.toml`. Triggers, conditions and actions stay in the
/// core's own JSON shape; the screen puts them into words.
class Automation {
  Automation({
    required this.id,
    required this.name,
    required this.enabled,
    this.triggers = const [],
    this.conditions = const [],
    this.actions = const [],
    this.nextRun,
    this.problems = const [],
    this.delegated = const [],
    this.runs = const [],
  });

  factory Automation.fromJson(Map<String, dynamic> j) => Automation(
        id: j['id'] as String,
        name: j['name'] as String? ?? j['id'] as String,
        enabled: j['enabled'] as bool? ?? true,
        triggers: _maps(j['triggers']),
        conditions: _maps(j['conditions']),
        actions: _maps(j['actions']),
        nextRun: j['next_run'] == null ? null : DateTime.tryParse(j['next_run'] as String)?.toLocal(),
        problems: (j['problems'] as List?)?.cast<String>() ?? const [],
        delegated: (j['delegated'] as List?)?.cast<String>() ?? const [],
        runs: [for (final r in (j['runs'] as List?) ?? const []) Run.fromJson((r as Map).cast())],
      );

  final String id;
  final String name;
  final bool enabled;
  final List<Map<String, dynamic>> triggers, conditions, actions;
  final DateTime? nextRun;
  final List<String> problems;

  /// The devices that run it themselves too, so it happens even with the hub off.
  final List<String> delegated;

  /// Newest first.
  final List<Run> runs;
}

List<Map<String, dynamic>> _maps(Object? list) => [for (final m in (list as List?) ?? const []) (m as Map).cast<String, dynamic>()];

/// What an automation did, or why it did nothing.
class Run {
  Run({required this.at, required this.trigger, this.skipped, this.results = const []});

  factory Run.fromJson(Map<String, dynamic> j) => Run(
        at: DateTime.tryParse(j['at'] as String? ?? '')?.toLocal() ?? DateTime.now(),
        trigger: j['trigger'] as String? ?? '',
        skipped: j['skipped'] as String?,
        results: [
          for (final r in (j['results'] as List?) ?? const [])
            RunResult(
              target: (r as Map)['target'] as String? ?? '',
              action: r['action'] as String? ?? '',
              ok: r['ok'] as bool? ?? false,
              outcome: r['outcome'] as String? ?? '',
            ),
        ],
      );

  final DateTime at;
  final String trigger;
  final String? skipped;
  final List<RunResult> results;

  bool get failed => skipped == null && results.any((r) => !r.ok);
}

class RunResult {
  const RunResult({required this.target, required this.action, required this.ok, required this.outcome});

  final String target, action, outcome;
  final bool ok;
}

/// What an automation would do now, without doing it.
class DryRun {
  DryRun({required this.wouldRun, this.conditions = const [], this.actions = const []});

  factory DryRun.fromJson(Map<String, dynamic> j) => DryRun(
        wouldRun: j['would_run'] as bool? ?? false,
        conditions: [
          for (final c in (j['conditions'] as List?) ?? const []) (ok: (c as Map)['ok'] as bool? ?? false, reason: c['reason'] as String? ?? ''),
        ],
        actions: [
          for (final a in (j['actions'] as List?) ?? const [])
            (
              action: ((a as Map)['action'] as Map).cast<String, dynamic>(),
              targets: [
                for (final t in (a['targets'] as List?) ?? const [])
                  (name: (t as Map)['name'] as String? ?? '', changes: t['changes'] as bool? ?? false, error: t['error'] as String?),
              ],
            ),
        ],
      );

  final bool wouldRun;
  final List<({bool ok, String reason})> conditions;
  final List<({Map<String, dynamic> action, List<({String name, bool changes, String? error})> targets})> actions;
}
