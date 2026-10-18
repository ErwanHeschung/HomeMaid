import 'package:flutter/widgets.dart';

import '../l10n/app_localizations.dart';

/// The localized name of a 3D model of the catalog (lib/viewport/models.dart).
String modelLabel(BuildContext context, String id) {
  final l = AppLocalizations.of(context);
  return switch (id) {
    'plug' => l.modelPlug,
    'lamp' => l.modelLamp,
    'screen' => l.modelScreen,
    'module' => l.modelModule,
    'bulb' => l.modelBulb,
    'ceiling_light' => l.modelCeilingLight,
    'light_strip' => l.modelLightStrip,
    'floor_lamp' => l.modelFloorLamp,
    'thermostat' => l.modelThermostat,
    'radiator' => l.modelRadiator,
    'fan' => l.modelFan,
    'air_purifier' => l.modelAirPurifier,
    'air_conditioner' => l.modelAirConditioner,
    'tv' => l.modelTv,
    'speaker' => l.modelSpeaker,
    'camera' => l.modelCamera,
    'door_lock' => l.modelDoorLock,
    'contact_sensor' => l.modelContactSensor,
    'motion_sensor' => l.modelMotionSensor,
    'smoke_detector' => l.modelSmokeDetector,
    'doorbell' => l.modelDoorbell,
    'climate_sensor' => l.modelClimateSensor,
    'kettle' => l.modelKettle,
    'coffee_maker' => l.modelCoffeeMaker,
    'washing_machine' => l.modelWashingMachine,
    'fridge' => l.modelFridge,
    'robot_vacuum' => l.modelRobotVacuum,
    'shutter' => l.modelShutter,
    'strip' => l.modelStrip,
    _ => id,
  };
}

/// The localized name of a room kind (lib/viewport/rooms.dart).
String roomKindLabel(BuildContext context, String kind) {
  final l = AppLocalizations.of(context);
  return switch (kind) {
    'living' => l.roomKindLiving,
    'kitchen' => l.roomKindKitchen,
    'bedroom' => l.roomKindBedroom,
    'office' => l.roomKindOffice,
    'bathroom' => l.roomKindBathroom,
    'entrance' => l.roomKindEntrance,
    _ => l.roomKindGeneric,
  };
}
