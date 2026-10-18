// ignore: unused_import
import 'package:intl/intl.dart' as intl;

import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for French (`fr`).
class AppLocalizationsFr extends AppLocalizations {
  AppLocalizationsFr([String locale = 'fr']) : super(locale);

  @override
  String get navHome => 'Maison';

  @override
  String get navPc => 'PC';

  @override
  String get navSettings => 'Réglages';

  @override
  String get pairHomeTitle => 'Appairer la maison';

  @override
  String get pairHomeHint =>
      'Sur le PC qui fait tourner HomeMaid, ouvre http://localhost:8123/pair puis scanne le code.';

  @override
  String get pairPcTitle => 'Appairer un PC';

  @override
  String get pairPcHint =>
      'Sur le PC, ouvre http://localhost:8124/pair puis scanne le code.';

  @override
  String get scanCode => 'Scanner le code';

  @override
  String get pairTitle => 'Appairer';

  @override
  String get pairInstructions =>
      'Sur le PC, ouvre http://localhost:8123/pair (maison) ou http://localhost:8124/pair (PC), puis vise le code.';

  @override
  String get notHomemaidCode => 'Ce n\'est pas un code HomeMaid.';

  @override
  String pairedHome(String id) {
    return 'Maison ($id) appairée.';
  }

  @override
  String pairedPc(String id) {
    return 'PC $id appairé.';
  }

  @override
  String get alsoScanPc =>
      'Pour la télécommande, scanne aussi http://localhost:8124/pair.';

  @override
  String get alsoScanHome =>
      'Pour la maison, scanne aussi http://localhost:8123/pair.';

  @override
  String get homeUnreachable => 'Maison injoignable';

  @override
  String get noDevices =>
      'Aucun appareil pour l\'instant. Ils apparaissent ici dès qu\'un plugin les découvre.';

  @override
  String get noRoom => 'Sans pièce';

  @override
  String get offline => 'Hors ligne';

  @override
  String get stateOn => 'Allumé';

  @override
  String get stateOff => 'Éteint';

  @override
  String get muted => 'Muet';

  @override
  String get soundOn => 'Son actif';

  @override
  String get pending => 'En attente de l\'appareil';

  @override
  String failed(String reason) {
    return 'Échec : $reason';
  }

  @override
  String get failedNoReason => 'Commande échouée';

  @override
  String deviceCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count appareils',
      one: '1 appareil',
      zero: 'Aucun appareil',
    );
    return '$_temp0';
  }

  @override
  String onCount(int count) {
    return '$count allumé(s)';
  }

  @override
  String get editDevice => 'Renommer ou déplacer';

  @override
  String get name => 'Nom';

  @override
  String get room => 'Pièce';

  @override
  String get roomHint => 'Vide = sans pièce';

  @override
  String get cancel => 'Annuler';

  @override
  String get save => 'Enregistrer';

  @override
  String get capOnOff => 'Alimentation';

  @override
  String get capBrightness => 'Luminosité';

  @override
  String get capVolume => 'Volume';

  @override
  String get capMute => 'Muet';

  @override
  String get capPower => 'Consommation';

  @override
  String get capCpu => 'Processeur';

  @override
  String get capMemory => 'Mémoire';

  @override
  String get capActiveWindow => 'Fenêtre active';

  @override
  String get trackpadHint =>
      'Tap pour cliquer · deux doigts pour clic droit ou défilement';

  @override
  String get connecting => 'Connexion…';

  @override
  String get pcUnreachable => 'PC injoignable';

  @override
  String get pairedSection => 'Appairés';

  @override
  String get nonePaired => 'Rien d\'appairé pour l\'instant.';

  @override
  String homeLabel(String id) {
    return 'Maison · $id';
  }

  @override
  String pcLabel(String id) {
    return 'PC · $id';
  }

  @override
  String get forget => 'Oublier';

  @override
  String get pair => 'Appairer';

  @override
  String get remoteSection => 'Télécommande';

  @override
  String get pointerSpeed => 'Vitesse du pointeur';

  @override
  String get invertScroll => 'Inverser le défilement';

  @override
  String get errorUnreachable => 'Injoignable, ou token refusé';

  @override
  String get errorLost => 'Connexion perdue';

  @override
  String get errorNotConnected => 'Pas connecté';

  @override
  String get errorTimeout => 'Pas de réponse';

  @override
  String get demoData => 'Données de démonstration';

  @override
  String get modelPlug => 'Prise connectée';

  @override
  String get modelLamp => 'Lampe de bureau';

  @override
  String get modelScreen => 'Écran';

  @override
  String get modelModule => 'Module';

  @override
  String get modelBulb => 'Ampoule';

  @override
  String get modelCeilingLight => 'Plafonnier';

  @override
  String get modelLightStrip => 'Ruban LED';

  @override
  String get modelFloorLamp => 'Lampadaire';

  @override
  String get modelThermostat => 'Thermostat';

  @override
  String get modelRadiator => 'Radiateur';

  @override
  String get modelFan => 'Ventilateur';

  @override
  String get modelAirPurifier => 'Purificateur d\'air';

  @override
  String get modelAirConditioner => 'Climatiseur';

  @override
  String get modelTv => 'Télévision';

  @override
  String get modelSpeaker => 'Enceinte';

  @override
  String get modelCamera => 'Caméra';

  @override
  String get modelDoorLock => 'Serrure';

  @override
  String get modelContactSensor => 'Détecteur d\'ouverture';

  @override
  String get modelMotionSensor => 'Détecteur de mouvement';

  @override
  String get modelSmokeDetector => 'Détecteur de fumée';

  @override
  String get modelDoorbell => 'Sonnette';

  @override
  String get modelClimateSensor => 'Thermo-hygromètre';

  @override
  String get modelKettle => 'Bouilloire';

  @override
  String get modelCoffeeMaker => 'Cafetière';

  @override
  String get modelWashingMachine => 'Lave-linge';

  @override
  String get modelFridge => 'Réfrigérateur';

  @override
  String get modelRobotVacuum => 'Robot aspirateur';

  @override
  String get modelShutter => 'Volet roulant';

  @override
  String get modelStrip => 'Multiprise';

  @override
  String get modelSection => 'Modèle 3D';

  @override
  String get modelAuto => 'Automatique';

  @override
  String moreInList(int count) {
    return '+$count dans la liste';
  }

  @override
  String get roomType => 'Type de pièce';

  @override
  String get roomKindLiving => 'Salon';

  @override
  String get roomKindKitchen => 'Cuisine';

  @override
  String get roomKindBedroom => 'Chambre';

  @override
  String get roomKindOffice => 'Bureau';

  @override
  String get roomKindBathroom => 'Salle de bain';

  @override
  String get roomKindEntrance => 'Entrée';

  @override
  String get roomKindGeneric => 'Pièce simple';

  @override
  String get connected => 'Connecté';

  @override
  String get power => 'Alimentation';

  @override
  String get powerLock => 'Verrouiller';

  @override
  String get powerSleep => 'Mettre en veille';

  @override
  String get powerShutdown => 'Éteindre';

  @override
  String shutdownConfirm(String name) {
    return 'Éteindre $name ?';
  }

  @override
  String get shutdownBody =>
      'Le PC s\'éteint tout de suite. La télécommande perd la connexion jusqu\'à son redémarrage.';

  @override
  String get keyboard => 'Clavier';

  @override
  String get volumeDown => 'Baisser le volume';

  @override
  String get volumeUp => 'Monter le volume';

  @override
  String get mute => 'Couper le son';

  @override
  String get unmute => 'Rétablir le son';

  @override
  String get playPause => 'Lecture ou pause';

  @override
  String get back5 => 'Reculer de 5 secondes';

  @override
  String get forward5 => 'Avancer de 5 secondes';

  @override
  String get previous => 'Précédent';

  @override
  String get next => 'Suivant';

  @override
  String launchApp(String app) {
    return 'Ouvrir $app';
  }

  @override
  String get restart => 'Revenir au début';

  @override
  String get nextEpisode => 'Épisode suivant';

  @override
  String get skipIntro => 'Passer l\'intro';

  @override
  String get stateLocked => 'Verrouillée';

  @override
  String get stateUnlocked => 'Déverrouillée';

  @override
  String get stateOpen => 'Ouverte';

  @override
  String get stateClosed => 'Fermée';

  @override
  String get stateMotion => 'Mouvement';

  @override
  String get stateNoMotion => 'Aucun mouvement';

  @override
  String get stateSmoke => 'Fumée détectée';

  @override
  String get stateNoSmoke => 'Aucune fumée';

  @override
  String get yes => 'Oui';

  @override
  String get no => 'Non';

  @override
  String autoAs(String kind) {
    return 'Auto · $kind';
  }
}
