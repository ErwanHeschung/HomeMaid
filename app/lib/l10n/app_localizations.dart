import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:intl/intl.dart' as intl;

import 'app_localizations_en.dart';
import 'app_localizations_fr.dart';

// ignore_for_file: type=lint

/// Callers can lookup localized strings with an instance of AppLocalizations
/// returned by `AppLocalizations.of(context)`.
///
/// Applications need to include `AppLocalizations.delegate()` in their app's
/// `localizationDelegates` list, and the locales they support in the app's
/// `supportedLocales` list. For example:
///
/// ```dart
/// import 'l10n/app_localizations.dart';
///
/// return MaterialApp(
///   localizationsDelegates: AppLocalizations.localizationsDelegates,
///   supportedLocales: AppLocalizations.supportedLocales,
///   home: MyApplicationHome(),
/// );
/// ```
///
/// ## Update pubspec.yaml
///
/// Please make sure to update your pubspec.yaml to include the following
/// packages:
///
/// ```yaml
/// dependencies:
///   # Internationalization support.
///   flutter_localizations:
///     sdk: flutter
///   intl: any # Use the pinned version from flutter_localizations
///
///   # Rest of dependencies
/// ```
///
/// ## iOS Applications
///
/// iOS applications define key application metadata, including supported
/// locales, in an Info.plist file that is built into the application bundle.
/// To configure the locales supported by your app, you’ll need to edit this
/// file.
///
/// First, open your project’s ios/Runner.xcworkspace Xcode workspace file.
/// Then, in the Project Navigator, open the Info.plist file under the Runner
/// project’s Runner folder.
///
/// Next, select the Information Property List item, select Add Item from the
/// Editor menu, then select Localizations from the pop-up menu.
///
/// Select and expand the newly-created Localizations item then, for each
/// locale your application supports, add a new item and select the locale
/// you wish to add from the pop-up menu in the Value field. This list should
/// be consistent with the languages listed in the AppLocalizations.supportedLocales
/// property.
abstract class AppLocalizations {
  AppLocalizations(String locale)
    : localeName = intl.Intl.canonicalizedLocale(locale.toString());

  final String localeName;

  static AppLocalizations of(BuildContext context) {
    return Localizations.of<AppLocalizations>(context, AppLocalizations)!;
  }

  static const LocalizationsDelegate<AppLocalizations> delegate =
      _AppLocalizationsDelegate();

  /// A list of this localizations delegate along with the default localizations
  /// delegates.
  ///
  /// Returns a list of localizations delegates containing this delegate along with
  /// GlobalMaterialLocalizations.delegate, GlobalCupertinoLocalizations.delegate,
  /// and GlobalWidgetsLocalizations.delegate.
  ///
  /// Additional delegates can be added by appending to this list in
  /// MaterialApp. This list does not have to be used at all if a custom list
  /// of delegates is preferred or required.
  static const List<LocalizationsDelegate<dynamic>> localizationsDelegates =
      <LocalizationsDelegate<dynamic>>[
        delegate,
        GlobalMaterialLocalizations.delegate,
        GlobalCupertinoLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
      ];

  /// A list of this localizations delegate's supported locales.
  static const List<Locale> supportedLocales = <Locale>[
    Locale('en'),
    Locale('fr'),
  ];

  /// No description provided for @navHome.
  ///
  /// In en, this message translates to:
  /// **'Home'**
  String get navHome;

  /// No description provided for @navPc.
  ///
  /// In en, this message translates to:
  /// **'PC'**
  String get navPc;

  /// No description provided for @navSettings.
  ///
  /// In en, this message translates to:
  /// **'Settings'**
  String get navSettings;

  /// No description provided for @pairHomeTitle.
  ///
  /// In en, this message translates to:
  /// **'Pair your home'**
  String get pairHomeTitle;

  /// No description provided for @pairHomeHint.
  ///
  /// In en, this message translates to:
  /// **'On the PC running HomeMaid, open http://localhost:8123/pair and scan the code.'**
  String get pairHomeHint;

  /// No description provided for @pairPcTitle.
  ///
  /// In en, this message translates to:
  /// **'Pair a PC'**
  String get pairPcTitle;

  /// No description provided for @pairPcHint.
  ///
  /// In en, this message translates to:
  /// **'On the PC, open http://localhost:8124/pair and scan the code.'**
  String get pairPcHint;

  /// No description provided for @scanCode.
  ///
  /// In en, this message translates to:
  /// **'Scan the code'**
  String get scanCode;

  /// No description provided for @pairTitle.
  ///
  /// In en, this message translates to:
  /// **'Pair'**
  String get pairTitle;

  /// No description provided for @pairInstructions.
  ///
  /// In en, this message translates to:
  /// **'On the PC, open http://localhost:8123/pair (home) or http://localhost:8124/pair (PC), then aim at the code.'**
  String get pairInstructions;

  /// No description provided for @notHomemaidCode.
  ///
  /// In en, this message translates to:
  /// **'This is not a HomeMaid code.'**
  String get notHomemaidCode;

  /// No description provided for @pairedHome.
  ///
  /// In en, this message translates to:
  /// **'Home ({id}) paired.'**
  String pairedHome(String id);

  /// No description provided for @pairedPc.
  ///
  /// In en, this message translates to:
  /// **'PC {id} paired.'**
  String pairedPc(String id);

  /// No description provided for @alsoScanPc.
  ///
  /// In en, this message translates to:
  /// **'For the remote, also scan http://localhost:8124/pair.'**
  String get alsoScanPc;

  /// No description provided for @alsoScanHome.
  ///
  /// In en, this message translates to:
  /// **'For the home, also scan http://localhost:8123/pair.'**
  String get alsoScanHome;

  /// No description provided for @homeUnreachable.
  ///
  /// In en, this message translates to:
  /// **'Home unreachable'**
  String get homeUnreachable;

  /// No description provided for @noDevices.
  ///
  /// In en, this message translates to:
  /// **'No devices yet. They appear here as soon as a plugin discovers them.'**
  String get noDevices;

  /// No description provided for @noRoom.
  ///
  /// In en, this message translates to:
  /// **'No room'**
  String get noRoom;

  /// No description provided for @offline.
  ///
  /// In en, this message translates to:
  /// **'Offline'**
  String get offline;

  /// No description provided for @stateOn.
  ///
  /// In en, this message translates to:
  /// **'On'**
  String get stateOn;

  /// No description provided for @stateOff.
  ///
  /// In en, this message translates to:
  /// **'Off'**
  String get stateOff;

  /// No description provided for @muted.
  ///
  /// In en, this message translates to:
  /// **'Muted'**
  String get muted;

  /// No description provided for @soundOn.
  ///
  /// In en, this message translates to:
  /// **'Sound on'**
  String get soundOn;

  /// No description provided for @pending.
  ///
  /// In en, this message translates to:
  /// **'Waiting for the device'**
  String get pending;

  /// No description provided for @failed.
  ///
  /// In en, this message translates to:
  /// **'Failed: {reason}'**
  String failed(String reason);

  /// No description provided for @failedNoReason.
  ///
  /// In en, this message translates to:
  /// **'Command failed'**
  String get failedNoReason;

  /// No description provided for @deviceCount.
  ///
  /// In en, this message translates to:
  /// **'{count, plural, =0{No devices} =1{1 device} other{{count} devices}}'**
  String deviceCount(int count);

  /// No description provided for @onCount.
  ///
  /// In en, this message translates to:
  /// **'{count} on'**
  String onCount(int count);

  /// No description provided for @editDevice.
  ///
  /// In en, this message translates to:
  /// **'Rename or move'**
  String get editDevice;

  /// No description provided for @name.
  ///
  /// In en, this message translates to:
  /// **'Name'**
  String get name;

  /// No description provided for @room.
  ///
  /// In en, this message translates to:
  /// **'Room'**
  String get room;

  /// No description provided for @roomHint.
  ///
  /// In en, this message translates to:
  /// **'Empty = no room'**
  String get roomHint;

  /// No description provided for @cancel.
  ///
  /// In en, this message translates to:
  /// **'Cancel'**
  String get cancel;

  /// No description provided for @save.
  ///
  /// In en, this message translates to:
  /// **'Save'**
  String get save;

  /// No description provided for @capOnOff.
  ///
  /// In en, this message translates to:
  /// **'Power'**
  String get capOnOff;

  /// No description provided for @capBrightness.
  ///
  /// In en, this message translates to:
  /// **'Brightness'**
  String get capBrightness;

  /// No description provided for @capVolume.
  ///
  /// In en, this message translates to:
  /// **'Volume'**
  String get capVolume;

  /// No description provided for @capMute.
  ///
  /// In en, this message translates to:
  /// **'Mute'**
  String get capMute;

  /// No description provided for @capPower.
  ///
  /// In en, this message translates to:
  /// **'Power draw'**
  String get capPower;

  /// No description provided for @capCpu.
  ///
  /// In en, this message translates to:
  /// **'CPU'**
  String get capCpu;

  /// No description provided for @capMemory.
  ///
  /// In en, this message translates to:
  /// **'Memory'**
  String get capMemory;

  /// No description provided for @capActiveWindow.
  ///
  /// In en, this message translates to:
  /// **'Active window'**
  String get capActiveWindow;

  /// No description provided for @trackpadHint.
  ///
  /// In en, this message translates to:
  /// **'Tap to click · two fingers to right-click or scroll'**
  String get trackpadHint;

  /// No description provided for @connecting.
  ///
  /// In en, this message translates to:
  /// **'Connecting…'**
  String get connecting;

  /// No description provided for @pcUnreachable.
  ///
  /// In en, this message translates to:
  /// **'PC unreachable'**
  String get pcUnreachable;

  /// No description provided for @pairedSection.
  ///
  /// In en, this message translates to:
  /// **'Paired'**
  String get pairedSection;

  /// No description provided for @nonePaired.
  ///
  /// In en, this message translates to:
  /// **'Nothing paired yet.'**
  String get nonePaired;

  /// No description provided for @homeLabel.
  ///
  /// In en, this message translates to:
  /// **'Home · {id}'**
  String homeLabel(String id);

  /// No description provided for @pcLabel.
  ///
  /// In en, this message translates to:
  /// **'PC · {id}'**
  String pcLabel(String id);

  /// No description provided for @forget.
  ///
  /// In en, this message translates to:
  /// **'Forget'**
  String get forget;

  /// No description provided for @pair.
  ///
  /// In en, this message translates to:
  /// **'Pair'**
  String get pair;

  /// No description provided for @remoteSection.
  ///
  /// In en, this message translates to:
  /// **'Remote'**
  String get remoteSection;

  /// No description provided for @pointerSpeed.
  ///
  /// In en, this message translates to:
  /// **'Pointer speed'**
  String get pointerSpeed;

  /// No description provided for @invertScroll.
  ///
  /// In en, this message translates to:
  /// **'Invert scrolling'**
  String get invertScroll;

  /// No description provided for @errorUnreachable.
  ///
  /// In en, this message translates to:
  /// **'Unreachable, or the token was refused'**
  String get errorUnreachable;

  /// No description provided for @errorLost.
  ///
  /// In en, this message translates to:
  /// **'Connection lost'**
  String get errorLost;

  /// No description provided for @errorNotConnected.
  ///
  /// In en, this message translates to:
  /// **'Not connected'**
  String get errorNotConnected;

  /// No description provided for @errorTimeout.
  ///
  /// In en, this message translates to:
  /// **'No answer'**
  String get errorTimeout;

  /// No description provided for @demoData.
  ///
  /// In en, this message translates to:
  /// **'Demo data'**
  String get demoData;

  /// No description provided for @modelPlug.
  ///
  /// In en, this message translates to:
  /// **'Smart plug'**
  String get modelPlug;

  /// No description provided for @modelLamp.
  ///
  /// In en, this message translates to:
  /// **'Desk lamp'**
  String get modelLamp;

  /// No description provided for @modelScreen.
  ///
  /// In en, this message translates to:
  /// **'Monitor'**
  String get modelScreen;

  /// No description provided for @modelModule.
  ///
  /// In en, this message translates to:
  /// **'Module'**
  String get modelModule;

  /// No description provided for @modelBulb.
  ///
  /// In en, this message translates to:
  /// **'Bulb'**
  String get modelBulb;

  /// No description provided for @modelCeilingLight.
  ///
  /// In en, this message translates to:
  /// **'Ceiling light'**
  String get modelCeilingLight;

  /// No description provided for @modelLightStrip.
  ///
  /// In en, this message translates to:
  /// **'Light strip'**
  String get modelLightStrip;

  /// No description provided for @modelFloorLamp.
  ///
  /// In en, this message translates to:
  /// **'Floor lamp'**
  String get modelFloorLamp;

  /// No description provided for @modelThermostat.
  ///
  /// In en, this message translates to:
  /// **'Thermostat'**
  String get modelThermostat;

  /// No description provided for @modelRadiator.
  ///
  /// In en, this message translates to:
  /// **'Radiator'**
  String get modelRadiator;

  /// No description provided for @modelFan.
  ///
  /// In en, this message translates to:
  /// **'Fan'**
  String get modelFan;

  /// No description provided for @modelAirPurifier.
  ///
  /// In en, this message translates to:
  /// **'Air purifier'**
  String get modelAirPurifier;

  /// No description provided for @modelAirConditioner.
  ///
  /// In en, this message translates to:
  /// **'Air conditioner'**
  String get modelAirConditioner;

  /// No description provided for @modelTv.
  ///
  /// In en, this message translates to:
  /// **'TV'**
  String get modelTv;

  /// No description provided for @modelSpeaker.
  ///
  /// In en, this message translates to:
  /// **'Speaker'**
  String get modelSpeaker;

  /// No description provided for @modelCamera.
  ///
  /// In en, this message translates to:
  /// **'Camera'**
  String get modelCamera;

  /// No description provided for @modelDoorLock.
  ///
  /// In en, this message translates to:
  /// **'Door lock'**
  String get modelDoorLock;

  /// No description provided for @modelContactSensor.
  ///
  /// In en, this message translates to:
  /// **'Contact sensor'**
  String get modelContactSensor;

  /// No description provided for @modelMotionSensor.
  ///
  /// In en, this message translates to:
  /// **'Motion sensor'**
  String get modelMotionSensor;

  /// No description provided for @modelSmokeDetector.
  ///
  /// In en, this message translates to:
  /// **'Smoke detector'**
  String get modelSmokeDetector;

  /// No description provided for @modelDoorbell.
  ///
  /// In en, this message translates to:
  /// **'Doorbell'**
  String get modelDoorbell;

  /// No description provided for @modelClimateSensor.
  ///
  /// In en, this message translates to:
  /// **'Climate sensor'**
  String get modelClimateSensor;

  /// No description provided for @modelKettle.
  ///
  /// In en, this message translates to:
  /// **'Kettle'**
  String get modelKettle;

  /// No description provided for @modelCoffeeMaker.
  ///
  /// In en, this message translates to:
  /// **'Coffee maker'**
  String get modelCoffeeMaker;

  /// No description provided for @modelWashingMachine.
  ///
  /// In en, this message translates to:
  /// **'Washing machine'**
  String get modelWashingMachine;

  /// No description provided for @modelFridge.
  ///
  /// In en, this message translates to:
  /// **'Fridge'**
  String get modelFridge;

  /// No description provided for @modelRobotVacuum.
  ///
  /// In en, this message translates to:
  /// **'Robot vacuum'**
  String get modelRobotVacuum;

  /// No description provided for @modelShutter.
  ///
  /// In en, this message translates to:
  /// **'Shutter'**
  String get modelShutter;

  /// No description provided for @modelStrip.
  ///
  /// In en, this message translates to:
  /// **'Power strip'**
  String get modelStrip;

  /// No description provided for @modelSection.
  ///
  /// In en, this message translates to:
  /// **'3D model'**
  String get modelSection;

  /// No description provided for @modelAuto.
  ///
  /// In en, this message translates to:
  /// **'Automatic'**
  String get modelAuto;

  /// No description provided for @moreInList.
  ///
  /// In en, this message translates to:
  /// **'+{count} in the list'**
  String moreInList(int count);

  /// No description provided for @roomType.
  ///
  /// In en, this message translates to:
  /// **'Room type'**
  String get roomType;

  /// No description provided for @roomKindLiving.
  ///
  /// In en, this message translates to:
  /// **'Living room'**
  String get roomKindLiving;

  /// No description provided for @roomKindKitchen.
  ///
  /// In en, this message translates to:
  /// **'Kitchen'**
  String get roomKindKitchen;

  /// No description provided for @roomKindBedroom.
  ///
  /// In en, this message translates to:
  /// **'Bedroom'**
  String get roomKindBedroom;

  /// No description provided for @roomKindOffice.
  ///
  /// In en, this message translates to:
  /// **'Office'**
  String get roomKindOffice;

  /// No description provided for @roomKindBathroom.
  ///
  /// In en, this message translates to:
  /// **'Bathroom'**
  String get roomKindBathroom;

  /// No description provided for @roomKindEntrance.
  ///
  /// In en, this message translates to:
  /// **'Entrance'**
  String get roomKindEntrance;

  /// No description provided for @roomKindGeneric.
  ///
  /// In en, this message translates to:
  /// **'Plain room'**
  String get roomKindGeneric;

  /// No description provided for @connected.
  ///
  /// In en, this message translates to:
  /// **'Connected'**
  String get connected;

  /// No description provided for @power.
  ///
  /// In en, this message translates to:
  /// **'Power'**
  String get power;

  /// No description provided for @powerLock.
  ///
  /// In en, this message translates to:
  /// **'Lock'**
  String get powerLock;

  /// No description provided for @powerSleep.
  ///
  /// In en, this message translates to:
  /// **'Sleep'**
  String get powerSleep;

  /// No description provided for @powerShutdown.
  ///
  /// In en, this message translates to:
  /// **'Shut down'**
  String get powerShutdown;

  /// No description provided for @shutdownConfirm.
  ///
  /// In en, this message translates to:
  /// **'Shut down {name}?'**
  String shutdownConfirm(String name);

  /// No description provided for @shutdownBody.
  ///
  /// In en, this message translates to:
  /// **'The PC powers off right away. The remote loses the connection until it starts again.'**
  String get shutdownBody;

  /// No description provided for @keyboard.
  ///
  /// In en, this message translates to:
  /// **'Keyboard'**
  String get keyboard;

  /// No description provided for @volumeDown.
  ///
  /// In en, this message translates to:
  /// **'Volume down'**
  String get volumeDown;

  /// No description provided for @volumeUp.
  ///
  /// In en, this message translates to:
  /// **'Volume up'**
  String get volumeUp;

  /// No description provided for @mute.
  ///
  /// In en, this message translates to:
  /// **'Mute'**
  String get mute;

  /// No description provided for @unmute.
  ///
  /// In en, this message translates to:
  /// **'Unmute'**
  String get unmute;

  /// No description provided for @playPause.
  ///
  /// In en, this message translates to:
  /// **'Play or pause'**
  String get playPause;

  /// No description provided for @back5.
  ///
  /// In en, this message translates to:
  /// **'Back 5 seconds'**
  String get back5;

  /// No description provided for @forward5.
  ///
  /// In en, this message translates to:
  /// **'Forward 5 seconds'**
  String get forward5;

  /// No description provided for @previous.
  ///
  /// In en, this message translates to:
  /// **'Previous'**
  String get previous;

  /// No description provided for @next.
  ///
  /// In en, this message translates to:
  /// **'Next'**
  String get next;

  /// No description provided for @launchApp.
  ///
  /// In en, this message translates to:
  /// **'Open {app}'**
  String launchApp(String app);

  /// No description provided for @restart.
  ///
  /// In en, this message translates to:
  /// **'Restart'**
  String get restart;

  /// No description provided for @nextEpisode.
  ///
  /// In en, this message translates to:
  /// **'Next episode'**
  String get nextEpisode;

  /// No description provided for @skipIntro.
  ///
  /// In en, this message translates to:
  /// **'Skip intro'**
  String get skipIntro;

  /// No description provided for @stateLocked.
  ///
  /// In en, this message translates to:
  /// **'Locked'**
  String get stateLocked;

  /// No description provided for @stateUnlocked.
  ///
  /// In en, this message translates to:
  /// **'Unlocked'**
  String get stateUnlocked;

  /// No description provided for @stateOpen.
  ///
  /// In en, this message translates to:
  /// **'Open'**
  String get stateOpen;

  /// No description provided for @stateClosed.
  ///
  /// In en, this message translates to:
  /// **'Closed'**
  String get stateClosed;

  /// No description provided for @stateMotion.
  ///
  /// In en, this message translates to:
  /// **'Motion'**
  String get stateMotion;

  /// No description provided for @stateNoMotion.
  ///
  /// In en, this message translates to:
  /// **'No motion'**
  String get stateNoMotion;

  /// No description provided for @stateSmoke.
  ///
  /// In en, this message translates to:
  /// **'Smoke detected'**
  String get stateSmoke;

  /// No description provided for @stateNoSmoke.
  ///
  /// In en, this message translates to:
  /// **'No smoke'**
  String get stateNoSmoke;

  /// No description provided for @yes.
  ///
  /// In en, this message translates to:
  /// **'Yes'**
  String get yes;

  /// No description provided for @no.
  ///
  /// In en, this message translates to:
  /// **'No'**
  String get no;

  /// No description provided for @autoAs.
  ///
  /// In en, this message translates to:
  /// **'Auto · {kind}'**
  String autoAs(String kind);

  /// No description provided for @navAutomations.
  ///
  /// In en, this message translates to:
  /// **'Automations'**
  String get navAutomations;

  /// No description provided for @automationCount.
  ///
  /// In en, this message translates to:
  /// **'{count, plural, =0{No automations} =1{1 automation} other{{count} automations}}'**
  String automationCount(int count);

  /// No description provided for @noAutomations.
  ///
  /// In en, this message translates to:
  /// **'Automations are written in homemaid.toml, as [[automation]]; homemaid.example.toml shows how. They appear here once the core restarts.'**
  String get noAutomations;

  /// No description provided for @everyDay.
  ///
  /// In en, this message translates to:
  /// **'Every day'**
  String get everyDay;

  /// No description provided for @weekdays.
  ///
  /// In en, this message translates to:
  /// **'Weekdays'**
  String get weekdays;

  /// No description provided for @weekend.
  ///
  /// In en, this message translates to:
  /// **'Weekends'**
  String get weekend;

  /// No description provided for @atTime.
  ///
  /// In en, this message translates to:
  /// **'{days} at {time}'**
  String atTime(String days, String time);

  /// No description provided for @runsWithoutHub.
  ///
  /// In en, this message translates to:
  /// **'Keeps running with the hub off, on: {devices}'**
  String runsWithoutHub(String devices);

  /// No description provided for @atSun.
  ///
  /// In en, this message translates to:
  /// **'{days} {when}'**
  String atSun(String days, String when);

  /// No description provided for @atSunrise.
  ///
  /// In en, this message translates to:
  /// **'at sunrise'**
  String get atSunrise;

  /// No description provided for @atSunset.
  ///
  /// In en, this message translates to:
  /// **'at sunset'**
  String get atSunset;

  /// No description provided for @sunrise.
  ///
  /// In en, this message translates to:
  /// **'sunrise'**
  String get sunrise;

  /// No description provided for @sunset.
  ///
  /// In en, this message translates to:
  /// **'sunset'**
  String get sunset;

  /// No description provided for @sunBefore.
  ///
  /// In en, this message translates to:
  /// **'{minutes} min before {event}'**
  String sunBefore(int minutes, String event);

  /// No description provided for @sunAfter.
  ///
  /// In en, this message translates to:
  /// **'{minutes} min after {event}'**
  String sunAfter(int minutes, String event);

  /// No description provided for @whenIs.
  ///
  /// In en, this message translates to:
  /// **'When {target} · {cap} becomes {value}'**
  String whenIs(String target, String cap, String value);

  /// No description provided for @whenAbove.
  ///
  /// In en, this message translates to:
  /// **'When {target} · {cap} goes above {value}'**
  String whenAbove(String target, String cap, String value);

  /// No description provided for @whenBelow.
  ///
  /// In en, this message translates to:
  /// **'When {target} · {cap} goes below {value}'**
  String whenBelow(String target, String cap, String value);

  /// No description provided for @whenChanges.
  ///
  /// In en, this message translates to:
  /// **'When {target} · {cap} changes'**
  String whenChanges(String target, String cap);

  /// No description provided for @ifBetween.
  ///
  /// In en, this message translates to:
  /// **'Between {from} and {to}'**
  String ifBetween(String from, String to);

  /// No description provided for @ifIs.
  ///
  /// In en, this message translates to:
  /// **'If {target} · {cap} is {value}'**
  String ifIs(String target, String cap, String value);

  /// No description provided for @ifAbove.
  ///
  /// In en, this message translates to:
  /// **'If {target} · {cap} is above {value}'**
  String ifAbove(String target, String cap, String value);

  /// No description provided for @ifBelow.
  ///
  /// In en, this message translates to:
  /// **'If {target} · {cap} is below {value}'**
  String ifBelow(String target, String cap, String value);

  /// No description provided for @ifAll.
  ///
  /// In en, this message translates to:
  /// **'{condition} (all of them)'**
  String ifAll(String condition);

  /// No description provided for @doSet.
  ///
  /// In en, this message translates to:
  /// **'Set {target} · {cap} to {value}'**
  String doSet(String target, String cap, String value);

  /// No description provided for @doInvoke.
  ///
  /// In en, this message translates to:
  /// **'{action} on {target}'**
  String doInvoke(String action, String target);

  /// No description provided for @sectionWhen.
  ///
  /// In en, this message translates to:
  /// **'When'**
  String get sectionWhen;

  /// No description provided for @sectionIf.
  ///
  /// In en, this message translates to:
  /// **'Only if'**
  String get sectionIf;

  /// No description provided for @sectionThen.
  ///
  /// In en, this message translates to:
  /// **'Then'**
  String get sectionThen;

  /// No description provided for @sectionRuns.
  ///
  /// In en, this message translates to:
  /// **'Last runs'**
  String get sectionRuns;

  /// No description provided for @tryIt.
  ///
  /// In en, this message translates to:
  /// **'Try it'**
  String get tryIt;

  /// No description provided for @runNow.
  ///
  /// In en, this message translates to:
  /// **'Run now'**
  String get runNow;

  /// No description provided for @dryWouldRun.
  ///
  /// In en, this message translates to:
  /// **'Would run now'**
  String get dryWouldRun;

  /// No description provided for @dryWouldNot.
  ///
  /// In en, this message translates to:
  /// **'Would not run'**
  String get dryWouldNot;

  /// No description provided for @dryChanges.
  ///
  /// In en, this message translates to:
  /// **'to do'**
  String get dryChanges;

  /// No description provided for @dryAlready.
  ///
  /// In en, this message translates to:
  /// **'already so'**
  String get dryAlready;

  /// No description provided for @dryNoTarget.
  ///
  /// In en, this message translates to:
  /// **'No device matches'**
  String get dryNoTarget;

  /// No description provided for @today.
  ///
  /// In en, this message translates to:
  /// **'Today'**
  String get today;

  /// No description provided for @tomorrow.
  ///
  /// In en, this message translates to:
  /// **'Tomorrow'**
  String get tomorrow;

  /// No description provided for @neverRan.
  ///
  /// In en, this message translates to:
  /// **'Never ran'**
  String get neverRan;

  /// No description provided for @runActed.
  ///
  /// In en, this message translates to:
  /// **'Acted'**
  String get runActed;

  /// No description provided for @runFailed.
  ///
  /// In en, this message translates to:
  /// **'Failed'**
  String get runFailed;

  /// No description provided for @runSkipped.
  ///
  /// In en, this message translates to:
  /// **'Skipped'**
  String get runSkipped;

  /// No description provided for @ranByHand.
  ///
  /// In en, this message translates to:
  /// **'Run by hand'**
  String get ranByHand;

  /// No description provided for @automationDisabled.
  ///
  /// In en, this message translates to:
  /// **'Disabled'**
  String get automationDisabled;

  /// No description provided for @problemCount.
  ///
  /// In en, this message translates to:
  /// **'{count, plural, =1{1 problem} other{{count} problems}}'**
  String problemCount(int count);

  /// No description provided for @nextAt.
  ///
  /// In en, this message translates to:
  /// **'Next: {when}'**
  String nextAt(String when);

  /// No description provided for @newAutomation.
  ///
  /// In en, this message translates to:
  /// **'New automation'**
  String get newAutomation;

  /// No description provided for @editAutomation.
  ///
  /// In en, this message translates to:
  /// **'Edit automation'**
  String get editAutomation;

  /// No description provided for @automationEnabled.
  ///
  /// In en, this message translates to:
  /// **'Enabled'**
  String get automationEnabled;

  /// No description provided for @addTrigger.
  ///
  /// In en, this message translates to:
  /// **'Add a trigger'**
  String get addTrigger;

  /// No description provided for @addCondition.
  ///
  /// In en, this message translates to:
  /// **'Add a condition'**
  String get addCondition;

  /// No description provided for @addAction.
  ///
  /// In en, this message translates to:
  /// **'Add an action'**
  String get addAction;

  /// No description provided for @kindMoment.
  ///
  /// In en, this message translates to:
  /// **'Time'**
  String get kindMoment;

  /// No description provided for @kindDevice.
  ///
  /// In en, this message translates to:
  /// **'Device'**
  String get kindDevice;

  /// No description provided for @timeClock.
  ///
  /// In en, this message translates to:
  /// **'Clock'**
  String get timeClock;

  /// No description provided for @timeSunrise.
  ///
  /// In en, this message translates to:
  /// **'Sunrise'**
  String get timeSunrise;

  /// No description provided for @timeSunset.
  ///
  /// In en, this message translates to:
  /// **'Sunset'**
  String get timeSunset;

  /// No description provided for @kindTimeRange.
  ///
  /// In en, this message translates to:
  /// **'Time range'**
  String get kindTimeRange;

  /// No description provided for @kindDays.
  ///
  /// In en, this message translates to:
  /// **'Days'**
  String get kindDays;

  /// No description provided for @kindState.
  ///
  /// In en, this message translates to:
  /// **'State'**
  String get kindState;

  /// No description provided for @rangeFrom.
  ///
  /// In en, this message translates to:
  /// **'From'**
  String get rangeFrom;

  /// No description provided for @rangeTo.
  ///
  /// In en, this message translates to:
  /// **'To'**
  String get rangeTo;

  /// No description provided for @kindSet.
  ///
  /// In en, this message translates to:
  /// **'Set'**
  String get kindSet;

  /// No description provided for @kindInvoke.
  ///
  /// In en, this message translates to:
  /// **'Run'**
  String get kindInvoke;

  /// No description provided for @where.
  ///
  /// In en, this message translates to:
  /// **'Where'**
  String get where;

  /// No description provided for @wholeHome.
  ///
  /// In en, this message translates to:
  /// **'Whole home'**
  String get wholeHome;

  /// No description provided for @what.
  ///
  /// In en, this message translates to:
  /// **'What'**
  String get what;

  /// No description provided for @testBecomes.
  ///
  /// In en, this message translates to:
  /// **'becomes'**
  String get testBecomes;

  /// No description provided for @testAbove.
  ///
  /// In en, this message translates to:
  /// **'goes above'**
  String get testAbove;

  /// No description provided for @testBelow.
  ///
  /// In en, this message translates to:
  /// **'goes below'**
  String get testBelow;

  /// No description provided for @testChanges.
  ///
  /// In en, this message translates to:
  /// **'changes'**
  String get testChanges;

  /// No description provided for @testIs.
  ///
  /// In en, this message translates to:
  /// **'is'**
  String get testIs;

  /// No description provided for @testOver.
  ///
  /// In en, this message translates to:
  /// **'above'**
  String get testOver;

  /// No description provided for @testUnder.
  ///
  /// In en, this message translates to:
  /// **'below'**
  String get testUnder;

  /// No description provided for @allMustPass.
  ///
  /// In en, this message translates to:
  /// **'All devices'**
  String get allMustPass;

  /// No description provided for @valueLabel.
  ///
  /// In en, this message translates to:
  /// **'Value'**
  String get valueLabel;

  /// No description provided for @appliesTo.
  ///
  /// In en, this message translates to:
  /// **'Applies to: {devices}'**
  String appliesTo(String devices);

  /// No description provided for @matchesNothing.
  ///
  /// In en, this message translates to:
  /// **'No device matches for now'**
  String get matchesNothing;

  /// No description provided for @advancedSelector.
  ///
  /// In en, this message translates to:
  /// **'Selector (advanced)'**
  String get advancedSelector;

  /// No description provided for @deleteAutomation.
  ///
  /// In en, this message translates to:
  /// **'Delete'**
  String get deleteAutomation;

  /// No description provided for @deleteAutomationConfirm.
  ///
  /// In en, this message translates to:
  /// **'Delete “{name}”?'**
  String deleteAutomationConfirm(String name);

  /// No description provided for @deleteAutomationBody.
  ///
  /// In en, this message translates to:
  /// **'It is removed from homemaid.toml and will not run again.'**
  String get deleteAutomationBody;

  /// No description provided for @removeItem.
  ///
  /// In en, this message translates to:
  /// **'Remove'**
  String get removeItem;

  /// No description provided for @actionToRun.
  ///
  /// In en, this message translates to:
  /// **'Action'**
  String get actionToRun;

  /// No description provided for @noActionHere.
  ///
  /// In en, this message translates to:
  /// **'No device here has actions'**
  String get noActionHere;

  /// No description provided for @homeLocation.
  ///
  /// In en, this message translates to:
  /// **'Home location'**
  String get homeLocation;

  /// No description provided for @locationUnset.
  ///
  /// In en, this message translates to:
  /// **'Not set: needed for sunrise and sunset'**
  String get locationUnset;

  /// No description provided for @latitude.
  ///
  /// In en, this message translates to:
  /// **'Latitude'**
  String get latitude;

  /// No description provided for @longitude.
  ///
  /// In en, this message translates to:
  /// **'Longitude'**
  String get longitude;

  /// No description provided for @locationHint.
  ///
  /// In en, this message translates to:
  /// **'Your town is enough, e.g. 48.86 and 2.35 for Paris. Nothing leaves your PC.'**
  String get locationHint;

  /// No description provided for @locationInvalid.
  ///
  /// In en, this message translates to:
  /// **'Latitude -90 to 90, longitude -180 to 180'**
  String get locationInvalid;

  /// No description provided for @sunNeedsLocation.
  ///
  /// In en, this message translates to:
  /// **'Sunrise and sunset need the home\'s location.'**
  String get sunNeedsLocation;

  /// No description provided for @setLocation.
  ///
  /// In en, this message translates to:
  /// **'Set the location'**
  String get setLocation;

  /// No description provided for @paramApp.
  ///
  /// In en, this message translates to:
  /// **'App'**
  String get paramApp;

  /// No description provided for @paramShortcut.
  ///
  /// In en, this message translates to:
  /// **'Service'**
  String get paramShortcut;

  /// No description provided for @capNoDevice.
  ///
  /// In en, this message translates to:
  /// **'{cap} (no device)'**
  String capNoDevice(String cap);
}

class _AppLocalizationsDelegate
    extends LocalizationsDelegate<AppLocalizations> {
  const _AppLocalizationsDelegate();

  @override
  Future<AppLocalizations> load(Locale locale) {
    return SynchronousFuture<AppLocalizations>(lookupAppLocalizations(locale));
  }

  @override
  bool isSupported(Locale locale) =>
      <String>['en', 'fr'].contains(locale.languageCode);

  @override
  bool shouldReload(_AppLocalizationsDelegate old) => false;
}

AppLocalizations lookupAppLocalizations(Locale locale) {
  // Lookup logic when only language code is specified.
  switch (locale.languageCode) {
    case 'en':
      return AppLocalizationsEn();
    case 'fr':
      return AppLocalizationsFr();
  }

  throw FlutterError(
    'AppLocalizations.delegate failed to load unsupported locale "$locale". This is likely '
    'an issue with the localizations generation tool. Please file an issue '
    'on GitHub with a reproducible sample app and the gen-l10n configuration '
    'that was used.',
  );
}
