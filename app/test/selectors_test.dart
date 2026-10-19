import 'package:flutter_test/flutter_test.dart';
import 'package:homemaid/demo.dart';
import 'package:homemaid/selectors.dart';

void main() {
  final entities = DemoCore().entities.values;

  test('a room and a capability match what the core would', () {
    final room = entities.firstWhere((e) => e.room != null && e.caps.containsKey('on_off')).room!;
    final lights = matching(entities, roomSelector(room, 'on_off'))!;
    expect(lights, isNotEmpty);
    expect(lights.every((e) => e.room == room && e.caps.containsKey('on_off')), isTrue);
    expect(matching(entities, roomSelector(null, 'on_off'))!.length, greaterThan(lights.length), reason: 'the whole home');
    expect(matching(entities, 'room:Salon | cap:volume'), isNull, reason: 'not a form the editor writes');
  });

  test('the editor reads back the selectors it writes, and only those', () {
    expect(parseRoomSelector(roomSelector('Salle de bain', 'brightness')), ('Salle de bain', 'brightness'));
    expect(parseRoomSelector(roomSelector(null, 'custom:open')), (null, 'custom:open'));
    expect(parseRoomSelector('tag:night & cap:on_off'), isNull);
  });
}
