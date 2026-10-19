/// The devices a selector matches, for the automation editor's preview and the demo. Only the
/// forms the editor writes (terms joined by `&`); the core checks every selector itself.
library;

import 'model.dart';

/// Null when the selector is not one of those forms (`|`, `!`, parentheses).
List<Entity>? matching(Iterable<Entity> entities, String select) {
  if (RegExp(r'[|!()]').hasMatch(select)) return null;
  final terms = <(String, String)>[];
  for (final term in select.split('&').map((t) => t.trim())) {
    final colon = term.indexOf(':');
    if (colon < 1) return null;
    var value = term.substring(colon + 1).trim();
    if (value.length > 1 && value.startsWith('"') && value.endsWith('"')) value = value.substring(1, value.length - 1);
    terms.add((term.substring(0, colon).trim(), value.toLowerCase()));
  }
  bool same(String? a, String b) => a?.toLowerCase() == b;
  bool matches(Entity e, (String, String) term) => switch (term) {
        ('room', final v) => same(e.room, v),
        ('cap', final v) => e.caps.keys.any((c) => same(c, v)),
        ('tag', final v) => e.tags.any((t) => same(t, v)),
        ('plugin', final v) => same(e.plugin, v),
        ('model', final v) => same(e.model, v),
        ('device', final v) => same(e.device, v),
        ('name', final v) => same(e.name, v),
        ('id', final v) => same(e.id, v),
        _ => false,
      };
  return [for (final e in entities) if (terms.every((t) => matches(e, t))) e];
}

/// The selector the editor writes: a room (or the whole home) and a capability.
String roomSelector(String? room, String cap) => room == null ? 'cap:$cap' : 'room:"$room" & cap:$cap';

/// The room and capability of a selector the editor wrote, or null for any other.
(String?, String)? parseRoomSelector(String select) {
  final m = RegExp(r'^(?:room:"([^"]*)"\s*&\s*)?cap:([a-z_:]+)$').firstMatch(select.trim());
  return m == null ? null : (m[1], m[2]!);
}
