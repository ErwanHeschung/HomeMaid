// ignore: unused_import
import 'package:intl/intl.dart' as intl;

import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for English (`en`).
class AppLocalizationsEn extends AppLocalizations {
  AppLocalizationsEn([String locale = 'en']) : super(locale);

  @override
  String get navHome => 'Home';

  @override
  String get navPc => 'PC';

  @override
  String get navSettings => 'Settings';

  @override
  String get pairHomeTitle => 'Pair your home';

  @override
  String get pairHomeHint =>
      'On the PC running HomeMaid, open http://localhost:8123/pair and scan the code.';

  @override
  String get pairPcTitle => 'Pair a PC';

  @override
  String get pairPcHint =>
      'On the PC, open http://localhost:8124/pair and scan the code.';

  @override
  String get scanCode => 'Scan the code';

  @override
  String get pairTitle => 'Pair';

  @override
  String get pairInstructions =>
      'On the PC, open http://localhost:8123/pair (home) or http://localhost:8124/pair (PC), then aim at the code.';

  @override
  String get notHomemaidCode => 'This is not a HomeMaid code.';

  @override
  String pairedHome(String id) {
    return 'Home ($id) paired.';
  }

  @override
  String pairedPc(String id) {
    return 'PC $id paired.';
  }

  @override
  String get alsoScanPc =>
      'For the remote, also scan http://localhost:8124/pair.';

  @override
  String get alsoScanHome =>
      'For the home, also scan http://localhost:8123/pair.';

  @override
  String get homeUnreachable => 'Home unreachable';

  @override
  String get noDevices =>
      'No devices yet. They appear here as soon as a plugin discovers them.';

  @override
  String get noRoom => 'No room';

  @override
  String get offline => 'Offline';

  @override
  String get stateOn => 'On';

  @override
  String get stateOff => 'Off';

  @override
  String get muted => 'Muted';

  @override
  String get soundOn => 'Sound on';

  @override
  String get pending => 'Waiting for the device';

  @override
  String failed(String reason) {
    return 'Failed: $reason';
  }

  @override
  String get failedNoReason => 'Command failed';

  @override
  String deviceCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count devices',
      one: '1 device',
      zero: 'No devices',
    );
    return '$_temp0';
  }

  @override
  String onCount(int count) {
    return '$count on';
  }

  @override
  String get editDevice => 'Rename or move';

  @override
  String get name => 'Name';

  @override
  String get room => 'Room';

  @override
  String get roomHint => 'Empty = no room';

  @override
  String get cancel => 'Cancel';

  @override
  String get save => 'Save';

  @override
  String get capOnOff => 'Power';

  @override
  String get capBrightness => 'Brightness';

  @override
  String get capVolume => 'Volume';

  @override
  String get capMute => 'Mute';

  @override
  String get capPower => 'Power draw';

  @override
  String get capCpu => 'CPU';

  @override
  String get capMemory => 'Memory';

  @override
  String get capActiveWindow => 'Active window';

  @override
  String get trackpadHint =>
      'Tap to click · two fingers to right-click or scroll';

  @override
  String get connecting => 'Connecting…';

  @override
  String get pcUnreachable => 'PC unreachable';

  @override
  String get pairedSection => 'Paired';

  @override
  String get nonePaired => 'Nothing paired yet.';

  @override
  String homeLabel(String id) {
    return 'Home · $id';
  }

  @override
  String pcLabel(String id) {
    return 'PC · $id';
  }

  @override
  String get forget => 'Forget';

  @override
  String get pair => 'Pair';

  @override
  String get remoteSection => 'Remote';

  @override
  String get pointerSpeed => 'Pointer speed';

  @override
  String get invertScroll => 'Invert scrolling';

  @override
  String get errorUnreachable => 'Unreachable, or the token was refused';

  @override
  String get errorLost => 'Connection lost';

  @override
  String get errorNotConnected => 'Not connected';

  @override
  String get errorTimeout => 'No answer';

  @override
  String get demoData => 'Demo data';

  @override
  String get modelPlug => 'Smart plug';

  @override
  String get modelLamp => 'Desk lamp';

  @override
  String get modelScreen => 'Monitor';

  @override
  String get modelModule => 'Module';

  @override
  String get modelBulb => 'Bulb';

  @override
  String get modelCeilingLight => 'Ceiling light';

  @override
  String get modelLightStrip => 'Light strip';

  @override
  String get modelFloorLamp => 'Floor lamp';

  @override
  String get modelThermostat => 'Thermostat';

  @override
  String get modelRadiator => 'Radiator';

  @override
  String get modelFan => 'Fan';

  @override
  String get modelAirPurifier => 'Air purifier';

  @override
  String get modelAirConditioner => 'Air conditioner';

  @override
  String get modelTv => 'TV';

  @override
  String get modelSpeaker => 'Speaker';

  @override
  String get modelCamera => 'Camera';

  @override
  String get modelDoorLock => 'Door lock';

  @override
  String get modelContactSensor => 'Contact sensor';

  @override
  String get modelMotionSensor => 'Motion sensor';

  @override
  String get modelSmokeDetector => 'Smoke detector';

  @override
  String get modelDoorbell => 'Doorbell';

  @override
  String get modelClimateSensor => 'Climate sensor';

  @override
  String get modelKettle => 'Kettle';

  @override
  String get modelCoffeeMaker => 'Coffee maker';

  @override
  String get modelWashingMachine => 'Washing machine';

  @override
  String get modelFridge => 'Fridge';

  @override
  String get modelRobotVacuum => 'Robot vacuum';

  @override
  String get modelShutter => 'Shutter';

  @override
  String get modelStrip => 'Power strip';

  @override
  String get modelSection => '3D model';

  @override
  String get modelAuto => 'Automatic';

  @override
  String moreInList(int count) {
    return '+$count in the list';
  }

  @override
  String get roomType => 'Room type';

  @override
  String get roomKindLiving => 'Living room';

  @override
  String get roomKindKitchen => 'Kitchen';

  @override
  String get roomKindBedroom => 'Bedroom';

  @override
  String get roomKindOffice => 'Office';

  @override
  String get roomKindBathroom => 'Bathroom';

  @override
  String get roomKindEntrance => 'Entrance';

  @override
  String get roomKindGeneric => 'Plain room';

  @override
  String get connected => 'Connected';

  @override
  String get power => 'Power';

  @override
  String get powerLock => 'Lock';

  @override
  String get powerSleep => 'Sleep';

  @override
  String get powerShutdown => 'Shut down';

  @override
  String shutdownConfirm(String name) {
    return 'Shut down $name?';
  }

  @override
  String get shutdownBody =>
      'The PC powers off right away. The remote loses the connection until it starts again.';

  @override
  String get keyboard => 'Keyboard';

  @override
  String get volumeDown => 'Volume down';

  @override
  String get volumeUp => 'Volume up';

  @override
  String get mute => 'Mute';

  @override
  String get unmute => 'Unmute';

  @override
  String get playPause => 'Play or pause';

  @override
  String get back5 => 'Back 5 seconds';

  @override
  String get forward5 => 'Forward 5 seconds';

  @override
  String get previous => 'Previous';

  @override
  String get next => 'Next';

  @override
  String launchApp(String app) {
    return 'Open $app';
  }

  @override
  String get restart => 'Restart';

  @override
  String get nextEpisode => 'Next episode';

  @override
  String get skipIntro => 'Skip intro';

  @override
  String get stateLocked => 'Locked';

  @override
  String get stateUnlocked => 'Unlocked';

  @override
  String get stateOpen => 'Open';

  @override
  String get stateClosed => 'Closed';

  @override
  String get stateMotion => 'Motion';

  @override
  String get stateNoMotion => 'No motion';

  @override
  String get stateSmoke => 'Smoke detected';

  @override
  String get stateNoSmoke => 'No smoke';

  @override
  String get yes => 'Yes';

  @override
  String get no => 'No';

  @override
  String autoAs(String kind) {
    return 'Auto · $kind';
  }

  @override
  String get navAutomations => 'Automations';

  @override
  String automationCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count automations',
      one: '1 automation',
      zero: 'No automations',
    );
    return '$_temp0';
  }

  @override
  String get noAutomations =>
      'Automations are written in homemaid.toml, as [[automation]]; homemaid.example.toml shows how. They appear here once the core restarts.';

  @override
  String get everyDay => 'Every day';

  @override
  String get weekdays => 'Weekdays';

  @override
  String get weekend => 'Weekends';

  @override
  String atTime(String days, String time) {
    return '$days at $time';
  }

  @override
  String runsWithoutHub(String devices) {
    return 'Keeps running with the hub off, on: $devices';
  }

  @override
  String atSun(String days, String when) {
    return '$days $when';
  }

  @override
  String get atSunrise => 'at sunrise';

  @override
  String get atSunset => 'at sunset';

  @override
  String get sunrise => 'sunrise';

  @override
  String get sunset => 'sunset';

  @override
  String sunBefore(int minutes, String event) {
    return '$minutes min before $event';
  }

  @override
  String sunAfter(int minutes, String event) {
    return '$minutes min after $event';
  }

  @override
  String whenIs(String target, String cap, String value) {
    return 'When $target · $cap becomes $value';
  }

  @override
  String whenAbove(String target, String cap, String value) {
    return 'When $target · $cap goes above $value';
  }

  @override
  String whenBelow(String target, String cap, String value) {
    return 'When $target · $cap goes below $value';
  }

  @override
  String whenChanges(String target, String cap) {
    return 'When $target · $cap changes';
  }

  @override
  String ifBetween(String from, String to) {
    return 'Between $from and $to';
  }

  @override
  String ifIs(String target, String cap, String value) {
    return 'If $target · $cap is $value';
  }

  @override
  String ifAbove(String target, String cap, String value) {
    return 'If $target · $cap is above $value';
  }

  @override
  String ifBelow(String target, String cap, String value) {
    return 'If $target · $cap is below $value';
  }

  @override
  String ifAll(String condition) {
    return '$condition (all of them)';
  }

  @override
  String doSet(String target, String cap, String value) {
    return 'Set $target · $cap to $value';
  }

  @override
  String doInvoke(String action, String target) {
    return '$action on $target';
  }

  @override
  String get sectionWhen => 'When';

  @override
  String get sectionIf => 'Only if';

  @override
  String get sectionThen => 'Then';

  @override
  String get sectionRuns => 'Last runs';

  @override
  String get tryIt => 'Try it';

  @override
  String get runNow => 'Run now';

  @override
  String get dryWouldRun => 'Would run now';

  @override
  String get dryWouldNot => 'Would not run';

  @override
  String get dryChanges => 'to do';

  @override
  String get dryAlready => 'already so';

  @override
  String get dryNoTarget => 'No device matches';

  @override
  String get today => 'Today';

  @override
  String get tomorrow => 'Tomorrow';

  @override
  String get neverRan => 'Never ran';

  @override
  String get runActed => 'Acted';

  @override
  String get runFailed => 'Failed';

  @override
  String get runSkipped => 'Skipped';

  @override
  String get ranByHand => 'Run by hand';

  @override
  String get automationDisabled => 'Disabled';

  @override
  String problemCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count problems',
      one: '1 problem',
    );
    return '$_temp0';
  }

  @override
  String nextAt(String when) {
    return 'Next: $when';
  }

  @override
  String get newAutomation => 'New automation';

  @override
  String get editAutomation => 'Edit automation';

  @override
  String get automationEnabled => 'Enabled';

  @override
  String get addTrigger => 'Add a trigger';

  @override
  String get addCondition => 'Add a condition';

  @override
  String get addAction => 'Add an action';

  @override
  String get kindMoment => 'Time';

  @override
  String get kindDevice => 'Device';

  @override
  String get timeClock => 'Clock';

  @override
  String get timeSunrise => 'Sunrise';

  @override
  String get timeSunset => 'Sunset';

  @override
  String get kindTimeRange => 'Time range';

  @override
  String get kindDays => 'Days';

  @override
  String get kindState => 'State';

  @override
  String get rangeFrom => 'From';

  @override
  String get rangeTo => 'To';

  @override
  String get kindSet => 'Set';

  @override
  String get kindInvoke => 'Run';

  @override
  String get where => 'Where';

  @override
  String get wholeHome => 'Whole home';

  @override
  String get what => 'What';

  @override
  String get testBecomes => 'becomes';

  @override
  String get testAbove => 'goes above';

  @override
  String get testBelow => 'goes below';

  @override
  String get testChanges => 'changes';

  @override
  String get testIs => 'is';

  @override
  String get testOver => 'above';

  @override
  String get testUnder => 'below';

  @override
  String get allMustPass => 'All devices';

  @override
  String get valueLabel => 'Value';

  @override
  String appliesTo(String devices) {
    return 'Applies to: $devices';
  }

  @override
  String get matchesNothing => 'No device matches for now';

  @override
  String get advancedSelector => 'Selector (advanced)';

  @override
  String get deleteAutomation => 'Delete';

  @override
  String deleteAutomationConfirm(String name) {
    return 'Delete “$name”?';
  }

  @override
  String get deleteAutomationBody =>
      'It is removed from homemaid.toml and will not run again.';

  @override
  String get removeItem => 'Remove';

  @override
  String get actionToRun => 'Action';

  @override
  String get noActionHere => 'No device here has actions';
}
