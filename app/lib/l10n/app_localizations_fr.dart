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
  String get addDevice => 'Ajouter un appareil';

  @override
  String get addPc => 'Un PC';

  @override
  String get addPcHint => 'Avec son code d\'appairage';

  @override
  String get addOther => 'Un autre appareil';

  @override
  String get addOtherHint => 'Installer le plugin de sa marque';

  @override
  String get deviceSameLogin => 'Vide : comme l\'appareil précédent';

  @override
  String get deviceAdded => 'Appareil ajouté.';

  @override
  String get removeDevice => 'Retirer cet appareil';

  @override
  String get removeDeviceBody =>
      'Il quitte la maison, avec toutes ses parties. Tu pourras l\'ajouter à nouveau avec +.';

  @override
  String roomWholeDevice(int count) {
    return 'Déplace tout l\'appareil ($count éléments)';
  }

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

  @override
  String get navAutomations => 'Scénarios';

  @override
  String automationCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count scénarios',
      one: '1 scénario',
      zero: 'Aucun scénario',
    );
    return '$_temp0';
  }

  @override
  String get noAutomations =>
      'Les scénarios s\'écrivent dans homemaid.toml, en [[automation]] ; homemaid.example.toml montre comment. Ils apparaissent ici au redémarrage du core.';

  @override
  String get everyDay => 'Tous les jours';

  @override
  String get weekdays => 'Du lundi au vendredi';

  @override
  String get weekend => 'Le week-end';

  @override
  String atTime(String days, String time) {
    return '$days à $time';
  }

  @override
  String runsWithoutHub(String devices) {
    return 'Continue même hub éteint, sur : $devices';
  }

  @override
  String atSun(String days, String when) {
    return '$days $when';
  }

  @override
  String get atSunrise => 'au lever du soleil';

  @override
  String get atSunset => 'au coucher du soleil';

  @override
  String get sunrise => 'le lever du soleil';

  @override
  String get sunset => 'le coucher du soleil';

  @override
  String sunBefore(int minutes, String event) {
    return '$minutes min avant $event';
  }

  @override
  String sunAfter(int minutes, String event) {
    return '$minutes min après $event';
  }

  @override
  String whenIs(String target, String cap, String value) {
    return 'Quand $target · $cap passe à $value';
  }

  @override
  String whenAbove(String target, String cap, String value) {
    return 'Quand $target · $cap dépasse $value';
  }

  @override
  String whenBelow(String target, String cap, String value) {
    return 'Quand $target · $cap passe sous $value';
  }

  @override
  String whenChanges(String target, String cap) {
    return 'Quand $target · $cap change';
  }

  @override
  String ifBetween(String from, String to) {
    return 'Entre $from et $to';
  }

  @override
  String ifIs(String target, String cap, String value) {
    return 'Si $target · $cap est $value';
  }

  @override
  String ifAbove(String target, String cap, String value) {
    return 'Si $target · $cap est au-dessus de $value';
  }

  @override
  String ifBelow(String target, String cap, String value) {
    return 'Si $target · $cap est en dessous de $value';
  }

  @override
  String ifAll(String condition) {
    return '$condition (tous)';
  }

  @override
  String doSet(String target, String cap, String value) {
    return 'Mettre $target · $cap à $value';
  }

  @override
  String doInvoke(String action, String target) {
    return '$action sur $target';
  }

  @override
  String get sectionWhen => 'Quand';

  @override
  String get sectionIf => 'Seulement si';

  @override
  String get sectionThen => 'Alors';

  @override
  String get sectionRuns => 'Dernières exécutions';

  @override
  String get tryIt => 'Essayer';

  @override
  String get runNow => 'Lancer';

  @override
  String get dryWouldRun => 'S\'exécuterait maintenant';

  @override
  String get dryWouldNot => 'Ne s\'exécuterait pas';

  @override
  String get dryChanges => 'à faire';

  @override
  String get dryAlready => 'déjà fait';

  @override
  String get dryNoTarget => 'Aucun appareil ne correspond';

  @override
  String get today => 'Aujourd\'hui';

  @override
  String get tomorrow => 'Demain';

  @override
  String get neverRan => 'Jamais exécuté';

  @override
  String get runActed => 'A agi';

  @override
  String get runFailed => 'Échec';

  @override
  String get runSkipped => 'Sauté';

  @override
  String get ranByHand => 'Lancé à la main';

  @override
  String get automationDisabled => 'Désactivé';

  @override
  String problemCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count problèmes',
      one: '1 problème',
    );
    return '$_temp0';
  }

  @override
  String nextAt(String when) {
    return 'Prochaine : $when';
  }

  @override
  String get newAutomation => 'Nouvelle automatisation';

  @override
  String get editAutomation => 'Modifier l\'automatisation';

  @override
  String get automationEnabled => 'Active';

  @override
  String get addTrigger => 'Ajouter un déclencheur';

  @override
  String get addCondition => 'Ajouter une condition';

  @override
  String get addAction => 'Ajouter une action';

  @override
  String get kindMoment => 'Moment';

  @override
  String get kindDevice => 'Appareil';

  @override
  String get timeClock => 'Heure';

  @override
  String get timeSunrise => 'Lever';

  @override
  String get timeSunset => 'Coucher';

  @override
  String get kindTimeRange => 'Plage horaire';

  @override
  String get kindDays => 'Jours';

  @override
  String get kindState => 'État';

  @override
  String get rangeFrom => 'De';

  @override
  String get rangeTo => 'À';

  @override
  String get kindSet => 'Régler';

  @override
  String get kindInvoke => 'Lancer';

  @override
  String get where => 'Où';

  @override
  String get wholeHome => 'Toute la maison';

  @override
  String get what => 'Quoi';

  @override
  String get testBecomes => 'devient';

  @override
  String get testAbove => 'dépasse';

  @override
  String get testBelow => 'passe sous';

  @override
  String get testChanges => 'change';

  @override
  String get testIs => 'est';

  @override
  String get testOver => 'au-dessus';

  @override
  String get testUnder => 'en dessous';

  @override
  String get allMustPass => 'Tous les appareils';

  @override
  String get valueLabel => 'Valeur';

  @override
  String appliesTo(String devices) {
    return 'Concerne : $devices';
  }

  @override
  String get matchesNothing => 'Aucun appareil ne correspond pour l\'instant';

  @override
  String get advancedSelector => 'Sélecteur (avancé)';

  @override
  String get deleteAutomation => 'Supprimer';

  @override
  String deleteAutomationConfirm(String name) {
    return 'Supprimer « $name » ?';
  }

  @override
  String get deleteAutomationBody =>
      'Elle est retirée de homemaid.toml et ne se déclenchera plus.';

  @override
  String get removeItem => 'Retirer';

  @override
  String get actionToRun => 'Action';

  @override
  String get noActionHere => 'Aucun appareil ici n\'a d\'action';

  @override
  String get homeLocation => 'Emplacement de la maison';

  @override
  String get locationUnset =>
      'Pas défini : nécessaire pour le lever et le coucher du soleil';

  @override
  String get latitude => 'Latitude';

  @override
  String get longitude => 'Longitude';

  @override
  String get locationHint =>
      'Votre ville suffit, par exemple 48,86 et 2,35 pour Paris. Rien ne quitte votre PC.';

  @override
  String get locationInvalid => 'Latitude de -90 à 90, longitude de -180 à 180';

  @override
  String get sunNeedsLocation =>
      'Le lever et le coucher du soleil ont besoin de l\'emplacement de la maison.';

  @override
  String get setLocation => 'Définir l\'emplacement';

  @override
  String get paramApp => 'Application';

  @override
  String get paramShortcut => 'Service';

  @override
  String capNoDevice(String cap) {
    return '$cap (aucun appareil)';
  }

  @override
  String get addLauncher => 'Ajouter à la télécommande';

  @override
  String get launcherSites => 'Sites';

  @override
  String get launcherApps => 'Applis';

  @override
  String get siteAdded => 'Ajouté';

  @override
  String get otherSite => 'Autre site';

  @override
  String get siteAddress => 'Adresse';

  @override
  String get addSite => 'Ajouter le site';

  @override
  String get siteInvalid => 'Un nom, et une adresse web en https://…';

  @override
  String get appsOff =>
      'Pour voir les applis de ce PC, active « Applis de ce PC sur le téléphone » dans la fenêtre HomeMaid du PC.';

  @override
  String get searchApps => 'Chercher une appli';

  @override
  String get noAppsFound => 'Aucune appli';

  @override
  String pinApp(String name) {
    return 'Épingler $name';
  }

  @override
  String unpinApp(String name) {
    return 'Désépingler $name';
  }

  @override
  String removeLauncher(String name) {
    return 'Retirer « $name » de la télécommande ?';
  }

  @override
  String get removeLauncherBody => 'Il est retiré de homemaid.toml sur le PC.';

  @override
  String get plugins => 'Plugins';

  @override
  String get pluginsHint => 'Ajoutez des marques et des appareils à la maison.';

  @override
  String get pluginsInstalled => 'Installés';

  @override
  String get pluginsAvailable => 'Disponibles';

  @override
  String get pluginsNone => 'Aucun plugin pour l\'instant.';

  @override
  String get pluginsAllInstalled =>
      'Tout ce que propose le registre est installé.';

  @override
  String get pluginRunning => 'En marche';

  @override
  String get pluginStarting => 'Démarrage…';

  @override
  String get pluginLocal => 'Réglé dans homemaid.toml sur le hub';

  @override
  String get pluginInstall => 'Installer';

  @override
  String pluginUpdate(String version) {
    return 'Mettre à jour en $version';
  }

  @override
  String get pluginNeedsNewer => 'Demande un HomeMaid plus récent';

  @override
  String get pluginSettings => 'Réglages';

  @override
  String pluginRemove(String name) {
    return 'Retirer $name ?';
  }

  @override
  String get pluginRemoveBody =>
      'Ses appareils quittent la maison. Ses mots de passe restent sur le hub, pour une réinstallation.';

  @override
  String get pluginAllowed => 'Il pourra :';

  @override
  String get permLocalNetwork => 'Parler aux appareils de votre réseau local';

  @override
  String get permLoopback => 'Parler aux programmes du PC du hub';

  @override
  String get pluginSigned =>
      'Le hub le télécharge depuis le registre et vérifie la signature de son auteur.';

  @override
  String get pluginKeep => 'Laisser vide pour garder';

  @override
  String registryUnreachable(String error) {
    return 'Registre injoignable : $error';
  }

  @override
  String get retry => 'Réessayer';

  @override
  String get kindDelay => 'Patienter';

  @override
  String get kindWait => 'Attendre que';

  @override
  String get kindIf => 'Si';

  @override
  String get kindRepeat => 'Répéter';

  @override
  String get kindMerge => 'Rejoindre';

  @override
  String get kindStop => 'Arrêter';

  @override
  String get kindTrigger => 'Déclencheur';

  @override
  String get portNext => 'ensuite';

  @override
  String get portError => 'si échec';

  @override
  String get portTimeout => 'si trop long';

  @override
  String get portThen => 'oui';

  @override
  String get portElse => 'non';

  @override
  String get portBody => 'à chaque tour';

  @override
  String get flowStart => 'Départ';

  @override
  String nodeDelay(String duration) {
    return 'Attendre $duration';
  }

  @override
  String nodeWaitUntil(String condition) {
    return 'Attendre que : $condition';
  }

  @override
  String nodeWaitFor(String trigger) {
    return 'Attendre : $trigger';
  }

  @override
  String nodeAtMost(String text, String duration) {
    return '$text ($duration au plus)';
  }

  @override
  String conditionsAnd(String a, String b) {
    return '$a et $b';
  }

  @override
  String conditionsOr(String a, String b) {
    return '$a ou $b';
  }

  @override
  String repeatCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count fois',
      one: 'Une fois',
    );
    return '$_temp0';
  }

  @override
  String repeatWhile(String condition) {
    return 'Tant que : $condition';
  }

  @override
  String repeatUntil(String condition) {
    return 'Jusqu\'à : $condition';
  }

  @override
  String repeatEach(String target) {
    return 'Pour chaque : $target';
  }

  @override
  String get mergeAll => 'Quand toutes les branches ont fini';

  @override
  String get mergeAny => 'Dès qu\'une branche arrive';

  @override
  String get stopOk => 'Terminer ici';

  @override
  String stopError(String error) {
    return 'Échouer : $error';
  }

  @override
  String get targetTrigger => 'l\'appareil déclencheur';

  @override
  String get targetItem => 'cet appareil';

  @override
  String get valueFromTrigger => 'sa nouvelle valeur';

  @override
  String ifTrigger(String trigger) {
    return 'Déclenché par : $trigger';
  }

  @override
  String get modeLabel => 'S\'il se redéclenche pendant une attente';

  @override
  String get modeSingle => 'Laisser finir l\'exécution en cours';

  @override
  String get modeRestart => 'Recommencer depuis le début';

  @override
  String get modeQueued => 'Refaire ensuite';

  @override
  String get modeParallel => 'Les deux en même temps';

  @override
  String get addStep => 'Ajouter une étape';

  @override
  String get addParallel => 'En même temps que la suite';

  @override
  String get joinStep => 'Ou continuer vers une étape existante';

  @override
  String get removeStep => 'Retirer cette étape';

  @override
  String stepCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count étapes',
      one: '1 étape',
    );
    return '$_temp0';
  }

  @override
  String get durationLabel => 'Durée';

  @override
  String get durationInvalid => 'Une durée comme 45s, 10m ou 1h30m';

  @override
  String get waitConditions => 'Conditions';

  @override
  String get waitEvent => 'Événement';

  @override
  String get limitWait => 'Limiter l\'attente';

  @override
  String get anyCondition => 'Une seule condition suffit';

  @override
  String get repeatTimes => 'Fois';

  @override
  String get repeatWhileKind => 'Tant que';

  @override
  String get repeatUntilKind => 'Jusqu\'à';

  @override
  String get repeatEachKind => 'Chaque appareil';

  @override
  String get stopFails => 'Terminer en échec';

  @override
  String get stopWhy => 'Pourquoi';

  @override
  String get fromTrigger => 'La valeur du déclencheur';

  @override
  String get showFlow => 'Voir le schéma';

  @override
  String runningSince(String when) {
    return 'En cours depuis $when';
  }

  @override
  String waitingAt(String steps) {
    return 'En attente : $steps';
  }

  @override
  String get stopRuns => 'Arrêter';

  @override
  String get runStopped => 'Arrêté';

  @override
  String get stoppedByHand => 'à la main';

  @override
  String get stoppedRestarted => 'relancé';

  @override
  String get stoppedEdited => 'l\'automatisation a été modifiée';

  @override
  String runOf(String when) {
    return 'Exécution de $when';
  }

  @override
  String get runInProgress => 'Exécution en cours';

  @override
  String get notReached => 'Pas atteinte';

  @override
  String stateIs(String target, String cap, String value) {
    return '$target · $cap est $value';
  }

  @override
  String stateAbove(String target, String cap, String value) {
    return '$target · $cap est au-dessus de $value';
  }

  @override
  String stateBelow(String target, String cap, String value) {
    return '$target · $cap est en dessous de $value';
  }

  @override
  String get modeSingleHint => 'Le nouveau déclenchement est ignoré.';

  @override
  String get modeRestartHint =>
      'L\'attente repart de zéro : une lumière qui reste allumée tant que quelqu\'un bouge.';

  @override
  String get modeQueuedHint =>
      'Chaque déclenchement passe, l\'un après l\'autre : une sonnette.';

  @override
  String get modeParallelHint =>
      'Chaque déclenchement tourne de son côté, en même temps.';

  @override
  String get addThen => 'Ensuite…';

  @override
  String get addOnError => 'Si ça échoue…';

  @override
  String get addOnTimeout => 'Si l\'attente est trop longue…';

  @override
  String get addEachTurn => 'À chaque tour…';

  @override
  String get addIfYes => 'Si oui…';

  @override
  String get addIfNo => 'Sinon…';

  @override
  String get kindSetHint => 'Allumer, éteindre ou régler un appareil';

  @override
  String get kindInvokeHint =>
      'Une action ponctuelle : pause, verrouiller, ouvrir une appli';

  @override
  String get kindDelayHint => 'Patienter un temps donné';

  @override
  String get kindWaitHint =>
      'Jusqu\'à ce qu\'un appareil change ou qu\'une condition soit vraie';

  @override
  String get kindIfHint => 'Faire une chose ou une autre, selon la maison';

  @override
  String get kindRepeatHint =>
      'Refaire des étapes : plusieurs fois, ou pour chaque appareil';

  @override
  String get kindStopHint => 'Terminer le scénario ici';

  @override
  String get flowHint =>
      'Touchez une carte pour la modifier, + pour ajouter une étape.';

  @override
  String get scenes => 'Scènes';

  @override
  String get newScene => 'Nouvelle scène';

  @override
  String get editScene => 'Modifier la scène';

  @override
  String get addScene => 'Scène';

  @override
  String get sceneRoom => 'Affichée dans';

  @override
  String get captureState => 'Prendre l\'état actuel';

  @override
  String get sceneDevicesHint =>
      'Choisissez les appareils qu\'elle règle, et l\'état de chacun.';

  @override
  String get sceneEmpty => 'Aucun appareil dans cette scène';

  @override
  String get sceneAlso => 'Aussi, tel qu\'écrit';

  @override
  String get deleteSceneBody => 'Elle est retirée de homemaid.toml.';

  @override
  String get sceneInForce => 'En place';

  @override
  String get kindScene => 'Scène';

  @override
  String get kindSceneHint => 'Plusieurs appareils d\'un coup';

  @override
  String nodeScene(String name) {
    return 'Mettre la scène $name';
  }

  @override
  String get noScenes => 'Aucune scène : créez-en une depuis l\'accueil';

  @override
  String get sceneActions => 'Puis, des actions';

  @override
  String get sceneActionsHint =>
      'Lancées après les états, dans l\'ordre : ouvrir une appli, lancer la lecture…';

  @override
  String get addSceneAction => 'Ajouter une action';
}
