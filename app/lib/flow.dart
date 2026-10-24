/// An automation's nodes (`[[automation.node]]`) as a graph to draw and edit, in pure Dart. The
/// core checks the graph itself; the edits here keep it whole (one way into each node but a
/// merge, no cycle), and [FlowGraph.layout] places it top to bottom.
library;

const nodeKinds = ['set', 'invoke', 'delay', 'wait', 'if', 'repeat', 'merge', 'stop'];

String kindOf(Map<String, dynamic> node) => nodeKinds.firstWhere(node.containsKey, orElse: () => 'stop');

/// A kind's ports, in drawing order (left to right).
List<String> portsOf(String kind) => switch (kind) {
      'set' || 'invoke' => const ['next', 'error'],
      'delay' || 'merge' => const ['next'],
      'wait' => const ['next', 'timeout'],
      'if' => const ['then', 'else'],
      'repeat' => const ['body', 'next'],
      _ => const [],
    };

/// The port a node put on a link goes on through; none for a `stop`.
String? mainPort(String kind) => switch (kind) {
      'if' => 'then',
      'stop' => null,
      _ => 'next',
    };

/// Where a port leaves its node: under its middle (0, the main way out), its left (-1) or its right
/// (1). An `if`'s two ways go either side; any other way out (`error`, `timeout`, `body`) goes
/// right, so the main line runs straight down.
int portSide(String kind, String port) => switch ((kind, port)) {
      ('if', 'then') => -1,
      ('if', _) => 1,
      _ when port == 'start' || port == mainPort(kind) => 0,
      _ => 1,
    };

/// A link: from a node's port, or from `start` (`from` null).
typedef Link = ({String? from, String port, String to});

class FlowGraph {
  FlowGraph(this.start, this.nodes);

  /// From an automation's config: its nodes, or its list of actions (written before nodes) as a
  /// chain, as the core reads it.
  factory FlowGraph.of(Map<String, dynamic> config) {
    List<Map<String, dynamic>> maps(Object? list) => [for (final m in (list as List?) ?? const []) _copy((m as Map).cast())];
    final nodes = maps(config['node']);
    if (nodes.isNotEmpty) return FlowGraph([...((config['start'] as List?) ?? const []).cast<String>()], nodes);
    final actions = maps(config['action']);
    String id(int i) => 'action-${i + 1}';
    return FlowGraph(actions.isEmpty ? [] : [id(0)], [
      for (final (i, a) in actions.indexed) {'id': id(i), ...a, if (i + 1 < actions.length) 'next': [id(i + 1)]},
    ]);
  }

  final List<String> start;
  final List<Map<String, dynamic>> nodes;

  Map<String, dynamic>? node(String id) => nodes.where((n) => n['id'] == id).firstOrNull;

  /// What the config keeps of it.
  Map<String, dynamic> toConfig() => {'start': start, 'node': nodes};

  List<String> targets(String? from, String port) =>
      from == null ? start : ((node(from)?[port] as List?) ?? const []).cast<String>();

  void _setTargets(String? from, String port, List<String> to) {
    if (from == null) {
      start
        ..clear()
        ..addAll(to);
      return;
    }
    final n = node(from)!;
    to.isEmpty ? n.remove(port) : n[port] = to;
  }

  /// Every link, `start`'s first.
  List<Link> get links => [
        for (final to in start) (from: null, port: 'start', to: to),
        for (final n in nodes)
          for (final port in portsOf(kindOf(n)))
            for (final to in targets(n['id'] as String, port)) (from: n['id'] as String, port: port, to: to),
      ];

  List<Link> inputs(String id) => [for (final l in links) if (l.to == id) l];

  /// `delay-2` when `delay-1` is taken.
  String newId(String kind) {
    var i = 1;
    while (node('$kind-$i') != null) {
      i++;
    }
    return '$kind-$i';
  }

  /// Puts `added` (a node, then the nodes of its body) on the link from (`from`, `port`): between
  /// it and what followed, or beside it (`parallel`), as a branch of its own.
  void insert(String? from, String port, List<Map<String, dynamic>> added, {bool parallel = false}) {
    final first = added.first;
    final old = [...targets(from, port)];
    final main = mainPort(kindOf(first));
    if (!parallel && main != null && old.isNotEmpty) {
      first[main] = old;
      _setTargets(from, port, [first['id'] as String]);
    } else {
      _setTargets(from, port, [...old, first['id'] as String]);
    }
    nodes.addAll(added);
  }

  /// What `id` reaches, itself included.
  Set<String> reach(String id) {
    final seen = <String>{};
    final todo = [id];
    while (todo.isNotEmpty) {
      final n = todo.removeLast();
      if (seen.add(n)) {
        final kind = node(n) == null ? '' : kindOf(node(n)!);
        for (final port in portsOf(kind)) {
          todo.addAll(targets(n, port));
        }
      }
    }
    return seen;
  }

  /// The nodes the link from (`from`, `port`) may also lead to: none that leads back to `from`,
  /// none it leads to already.
  /// A body is closed: nothing outside it leads in.
  List<String> joinable(String? from, String port) {
    final already = targets(from, port).toSet();
    final closed = <String>{
      for (final n in nodes)
        if (kindOf(n) == 'repeat')
          if (targets(n['id'] as String, 'body').expand(reach).toSet() case final body when !body.contains(from)) ...body,
    };
    return [
      for (final n in nodes)
        if (!already.contains(n['id']) && !closed.contains(n['id']) && (from == null || !reach(n['id'] as String).contains(from)))
          n['id'] as String,
    ];
  }

  /// Links (`from`, `port`) to `id` too, through a merge before it when it is not one.
  void join(String? from, String port, String id) {
    var to = id;
    if (kindOf(node(id)!) != 'merge') {
      to = newId('merge');
      for (final l in inputs(id)) {
        _setTargets(l.from, l.port, [for (final t in targets(l.from, l.port)) t == id ? to : t]);
      }
      nodes.add({'id': to, 'merge': 'all', 'next': [id]});
    }
    _setTargets(from, port, [...targets(from, port), to]);
  }

  /// Takes `id` out: what followed it on its main port takes its place; its other branches (else,
  /// error, timeout, a body) go with it.
  void remove(String id) {
    final n = node(id);
    if (n == null) return;
    final main = mainPort(kindOf(n));
    final follow = main == null ? const <String>[] : targets(id, main);
    for (final (i, l) in inputs(id).indexed) {
      _setTargets(l.from, l.port, [
        for (final t in targets(l.from, l.port)) ...(t == id ? (i == 0 ? follow : const <String>[]) : [t]),
      ]);
    }
    nodes.remove(n);
    prune();
  }

  /// Drops what nothing leads to any more, and the links to nodes that are gone.
  void prune() {
    final reached = <String>{for (final s in start) ...reach(s)};
    nodes.removeWhere((n) => !reached.contains(n['id']));
    start.removeWhere((s) => node(s) == null);
    for (final n in nodes) {
      for (final port in portsOf(kindOf(n))) {
        final kept = [for (final t in targets(n['id'] as String, port)) if (node(t) != null) t];
        kept.isEmpty ? n.remove(port) : n[port] = kept;
      }
    }
  }

  /// Where each node goes: its layer (0 under `start`, children always below their parents) and
  /// its column, children under their parents (the mean of theirs), side by side without overlap.
  Map<String, ({int layer, double column})> layout() {
    final layer = <String, int>{};
    final parents = <String, List<String?>>{};
    for (final l in links) {
      (parents[l.to] ??= []).add(l.from);
    }
    // Longest path from `start`: a merge is below every branch that reaches it.
    final order = <String>[];
    final waiting = {for (final n in nodes) n['id'] as String: (parents[n['id']] ?? const []).length};
    final ready = [for (final s in start) s];
    while (ready.isNotEmpty) {
      final id = ready.removeAt(0);
      if (order.contains(id)) continue;
      order.add(id);
      layer[id] = [for (final p in parents[id] ?? const <String?>[]) p == null ? 0 : (layer[p] ?? 0) + 1].fold(0, (a, b) => a > b ? a : b);
      for (final l in links.where((l) => l.from == id)) {
        waiting[l.to] = (waiting[l.to] ?? 1) - 1;
        if (waiting[l.to]! <= 0) ready.add(l.to);
      }
    }
    // Not reached (a cycle the core will refuse): at the bottom, not lost.
    final bottom = layer.values.fold(0, (a, b) => a > b ? a : b) + 1;
    for (final n in nodes) {
      layer.putIfAbsent(n['id'] as String, () => bottom);
      if (!order.contains(n['id'])) order.add(n['id'] as String);
    }

    final column = <String, double>{};
    final layers = <int, List<String>>{};
    for (final id in order) {
      (layers[layer[id]!] ??= []).add(id);
    }
    // Where a child would like to be: under its first parent's port, else the mean of its parents.
    double wish(String id) {
      final from = links.where((l) => l.to == id).toList();
      if (from.isEmpty) return 0;
      if (from.length == 1) {
        final l = from.first;
        final siblings = targets(l.from, l.port);
        // Under its port, siblings side by side.
        final portShift = l.from == null ? 0.0 : portSide(kindOf(node(l.from!)!), l.port) * 0.3;
        final base = l.from == null ? 0.0 : column[l.from] ?? 0;
        return base + portShift + siblings.indexOf(id) - (siblings.length - 1) / 2;
      }
      final known = [for (final l in from) column[l.from] ?? 0.0];
      return known.reduce((a, b) => a + b) / known.length;
    }

    for (final key in layers.keys.toList()..sort()) {
      final row = layers[key]!..sort((a, b) => wish(a).compareTo(wish(b)));
      final wishes = [for (final id in row) wish(id)];
      final placed = [...wishes];
      for (var i = 1; i < placed.length; i++) {
        if (placed[i] < placed[i - 1] + 1) placed[i] = placed[i - 1] + 1;
      }
      // Pushed apart to the right only: shifted back so the row stays centred on its wishes.
      final shift = [for (var i = 0; i < placed.length; i++) placed[i] - wishes[i]].fold(0.0, (a, b) => a + b) / placed.length;
      for (final (i, id) in row.indexed) {
        column[id] = placed[i] - shift;
      }
    }
    return {for (final id in order) id: (layer: layer[id]!, column: column[id]!)};
  }
}

Map<String, dynamic> _copy(Map<String, dynamic> m) => {
      for (final MapEntry(:key, :value) in m.entries)
        key: switch (value) {
          final Map v => _copy(v.cast()),
          final List v => [for (final x in v) x is Map ? _copy(x.cast()) : x],
          _ => value,
        },
    };

/// `1h30m` in seconds; null when it is not a duration the core reads (numbers with h, m, s).
int? durationSeconds(String text) {
  final parts = RegExp(r'(\d+)([hms])').allMatches(text).toList();
  if (parts.isEmpty || parts.map((m) => m[0]).join() != text) return null;
  final total = parts.fold(0, (sum, m) => sum + int.parse(m[1]!) * switch (m[2]) { 'h' => 3600, 'm' => 60, _ => 1 });
  return total == 0 ? null : total;
}

/// Seconds as the core writes them: `1h30m`, `45s`.
String durationOf(int seconds) {
  final (h, m, s) = (seconds ~/ 3600, seconds ~/ 60 % 60, seconds % 60);
  return '${h > 0 ? '${h}h' : ''}${m > 0 ? '${m}m' : ''}${s > 0 ? '${s}s' : ''}';
}
