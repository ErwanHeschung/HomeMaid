import 'package:flutter_test/flutter_test.dart';
import 'package:homemaid/flow.dart';

Map<String, dynamic> set(String id, [Map<String, dynamic> ports = const {}]) =>
    {'id': id, 'set': {'select': 'room:"Salon" & cap:on_off', 'cap': 'on_off', 'value': true}, ...ports};

void main() {
  test('a list of actions reads as a chain, and is written back as nodes', () {
    final g = FlowGraph.of({
      'action': [
        {'set': {'select': 'cap:on_off', 'cap': 'on_off', 'value': true}},
        {'invoke': {'select': 'plugin:pc', 'action': 'play_pause'}},
      ],
    });
    expect(g.start, ['action-1']);
    expect(g.node('action-1')!['next'], ['action-2']);
    expect(g.toConfig()['node'], hasLength(2));
  });

  test('a node goes between a port and what followed, or beside it', () {
    final g = FlowGraph(['a'], [set('a', {'next': ['b']}), set('b')]);
    g.insert('a', 'next', [{'id': g.newId('delay'), 'delay': '10m'}]);
    expect(g.targets('a', 'next'), ['delay-1']);
    expect(g.targets('delay-1', 'next'), ['b']);

    g.insert('a', 'next', [set('c')], parallel: true);
    expect(g.targets('a', 'next'), ['delay-1', 'c'], reason: 'a branch of its own');

    g.insert(null, 'start', [{'id': 'check', 'if': [{'days': ['mon']}]}]);
    expect(g.start, ['check']);
    expect(g.targets('check', 'then'), ['a'], reason: 'an if goes on through then');
  });

  test('joining a branch puts a merge before a node, never a cycle', () {
    final g = FlowGraph(['check'], [
      {'id': 'check', 'if': [{'days': ['mon']}], 'then': ['a'], 'else': ['b']},
      set('a', {'next': ['end']}),
      set('b'),
      set('end'),
    ]);
    expect(g.joinable('end', 'next'), isNot(contains('check')), reason: 'it leads back to end');
    expect(g.joinable('b', 'next'), containsAll(['end', 'a']));
    g.join('b', 'next', 'end');
    expect(g.targets('a', 'next'), ['merge-1']);
    expect(g.targets('b', 'next'), ['merge-1']);
    expect(g.targets('merge-1', 'next'), ['end']);
    g.join('check', 'else', 'merge-1');
    expect(g.inputs('merge-1'), hasLength(3), reason: 'a merge takes another input as it is');
  });

  test('a node taken out leaves what followed it, and its other branches go with it', () {
    final g = FlowGraph(['w'], [
      {'id': 'w', 'wait': {'until': [{'days': ['mon']}]}, 'next': ['a'], 'timeout': ['late']},
      set('a'),
      set('late', {'next': ['later']}),
      set('later'),
    ]);
    g.remove('w');
    expect(g.start, ['a']);
    expect(g.nodes.map((n) => n['id']), ['a'], reason: 'the timeout branch went with it');
  });

  test('the layout puts children under their parents and merges below every branch', () {
    final g = FlowGraph(['check'], [
      {'id': 'check', 'if': [{'days': ['mon']}], 'then': ['a'], 'else': ['b']},
      set('a', {'next': ['a2']}),
      set('a2', {'next': ['join']}),
      set('b', {'next': ['join']}),
      {'id': 'join', 'merge': 'all', 'next': ['end']},
      set('end'),
    ]);
    final at = g.layout();
    expect(at['check']!.layer, 0);
    expect(at['join']!.layer, 3, reason: 'below a2, the longer branch');
    expect(at['a']!.column, lessThan(at['b']!.column), reason: 'then on the left, else on the right');
    expect((at['a']!.column - at['b']!.column).abs(), greaterThanOrEqualTo(1), reason: 'no overlap');
    expect(at['end']!.column, at['join']!.column);
  });

  test('durations read and write as the core does', () {
    expect(durationSeconds('1h30m'), 5400);
    expect(durationSeconds('45s'), 45);
    expect(durationSeconds('10'), isNull);
    expect(durationSeconds('1h 30m'), isNull);
    expect(durationOf(5400), '1h30m');
    expect(durationOf(600), '10m');
  });

  test("a repeat's body is joined only from inside it", () {
    final g = FlowGraph(['r'], [
      {'id': 'r', 'repeat': {'count': 2}, 'body': ['pause'], 'next': ['after']},
      {'id': 'pause', 'delay': '1m'},
      set('after'),
    ]);
    expect(g.joinable('after', 'next'), isNot(contains('pause')));
    expect(g.joinable('pause', 'next'), isNot(contains('r')), reason: 'it leads back');
  });
}
