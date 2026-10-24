# CLAUDE.md — HomeMaid (hub domotique)

> Brief de projet pour Claude Code. À lire en entier avant toute modification.

## Vision

Une alternative personnelle et open source à Home Assistant :
- **Core serveur en Rust**, léger, binaire unique, tourne 24/7 (Pi, mini-PC, VM, Docker).
- **UI mobile moderne et fluide**, style Tesla : visuelle, animée, pensée tactile. Ce n'est pas un dashboard de tuiles en WebView.
- **Télécommande PC native** : le téléphone sert de trackpad, clavier et télécommande média pour le PC.
- **Plugins à la demande**, téléchargeables et activables depuis un registre GitHub.

Le projet est d'abord fait pour l'usage de l'auteur. Seuls ses appareils sont supportés officiellement. L'architecture doit cependant permettre à des contributeurs d'étendre le projet jusqu'à un « Home Assistant like ».

## Principes non négociables

1. **Aucun chemin privilégié.** Les plugins officiels passent par la même API publique que les plugins tiers. Le core ne connaît **aucune marque**.
2. **Le core ne connaît que des capacités génériques.** L'UI se rend à partir des capacités, donc un nouveau plugin apparaît dans l'UI sans modification du front.
3. **L'API plugin est versionnée dès le premier plugin** (`plugin-api v1`). Pas de breaking change sans nouvelle version majeure.
4. **Le local d'abord.** Aucune dépendance au cloud pour le fonctionnement normal.
5. **Le push d'abord.** Les plugins émettent des événements. Le polling est un repli, à fréquence adaptative.

## Améliorations par rapport au modèle Home Assistant

| Problème HA | Notre approche |
|---|---|
| État en string, attributs en dict libre | **Capacités typées** avec schéma (`OnOff`, `Brightness { value: u8 }`, `Power { watts: f32 }`…) |
| Commande « fire and forget » | **État désiré + état observé** + réconciliateur. Chaque commande a un statut `pending` / `confirmed` / `failed`. Un appareil hors ligne reçoit l'état désiré à son retour. |
| Renommer `light.salon` casse les automatisations | **UUID interne stable**, nom d'affichage libre, références par **sélecteurs** (`room:salon & cap:OnOff`) |
| YAML + Jinja fragile, non testable | Automatisations **typées et validées au chargement**, avec un **mode simulation / dry-run** |
| Une intégration buguée impacte tout le core | **Plugins WASM sandboxés**, limites de ressources, redémarrage automatique en cas de crash |
| L'historique SQLite gonfle | Stockage time-series avec **downsampling** (brut sur N jours, puis agrégats) |
| Config éclatée entre YAML et `.storage` JSON | **Une seule source** : config déclarative versionnable dans git. L'UI écrit dans les mêmes fichiers. |
| Scènes = snapshot | Scène = un ensemble d'états désirés |

Priorités : **état désiré/observé** et **capacités typées** d'abord, car ils sont très coûteux à ajouter après coup.

## Architecture

```
┌──────────────┐   WebSocket/gRPC    ┌──────────────────────────────┐
│  App mobile  │ ◄────────────────► │            Core (Rust)        │
│  (Flutter)   │                     │  - Registre d'entités         │
└──────┬───────┘                     │  - Bus d'événements           │
       │ WS/UDP basse latence        │  - Réconciliateur             │
       ▼                             │  - Moteur d'automatisations   │
┌──────────────┐   plugin "pc"       │  - Stockage état + historique │
│  Agent PC    │ ◄────────────────► │  - Hôte WASM (wasmtime)       │
│   (Rust)     │                     └──────────────┬───────────────┘
└──────────────┘                                    │ WIT
                                       ┌────────────┴─────────────┐
                                       │ Plugins .wasm            │
                                       │ (tplink, ampoules, ...)  │
                                       └──────────────────────────┘
```

### Modes de déploiement
Un **seul binaire** `homemaid` dont les rôles sont activés par flags :
- `homemaid --core --agent` : **mode PC = runner**. Le core et l'agent PC tournent dans le même process sur le PC Windows. C'est le mode de démarrage du projet. Le plugin `pc` parle alors à l'agent en local (in-process ou loopback) au lieu du réseau.
- `homemaid --core` : le core seul sur un host toujours allumé (Pi, mini-PC, Docker). L'agent tourne à part sur le PC.
- `homemaid --agent` : l'agent seul, côté PC, quand le core est ailleurs.

Le passage d'un mode à l'autre ne doit demander **aucune modification de code**, seulement de configuration. Le core doit donc rester portable (Windows, Linux x86_64, Linux ARM64).

Sous Windows, le binaire s'exécute nativement, pas dans WSL : WSL s'arrête quand plus rien ne l'utilise et complique le réseau.
- **Mode bureau** (le cas normal, client dans `core/crates/desktop`) : `homemaid.exe` **sans flag de rôle**, lancé par un double-clic.
  - Il fait tourner le hub et l'agent dans un seul process, avec `homemaid.toml` et `homemaid.log` à côté de l'exe.
  - **Premier lancement** (pas de `homemaid.toml`) : il l'écrit lui-même (`config::write_first`) avec le hub, l'agent, des tokens aléatoires (`uuid` v4, aléa de l'OS) et le plugin `pc` si `plugins/pc_plugin.wasm` est là. Le token de l'agent va aussi dans `data/secrets.toml` (`[pc] token`). Personne ne tape de token : l'app les lit dans les QR. Activer « Contrôle du PC » sans section `[agent]` la crée de la même façon, puis redémarre.
  - WebView2 range son cache dans `data/webview`.
  - Le **logo est dans la zone de notification** tant qu'il tourne (clic : la fenêtre ; menu : Ouvrir, Quitter).
  - La **fenêtre HomeMaid** est une page (`window.html`) dans une WebView2 (`wry` + `tao`, façon Tauri), servie depuis l'exe par le protocole `homemaid://`. Pas de serveur HTTP pour elle : elle parle au process par IPC.
  - Contenu : état du hub, QR d'appairage (maison et PC), « Contrôle du PC » (`[agent] enabled`, l'agent s'arrête et repart à chaud), « Démarrer avec Windows » (valeur `HomeMaid` de `HKCU\…\Run`, `--background`), Ouvrir le dossier, Redémarrer, Quitter.
  - Une seconde instance montre la fenêtre de la première (événement nommé `Local\HomeMaid.Show`). Les erreurs de démarrage s'affichent dans une boîte de dialogue.
  - **Déploiement** : `homemaid.exe` + `WebView2Loader.dll` (mingw ne peut pas l'intégrer à l'exe ; copiée depuis `target/x86_64-pc-windows-gnu/release/build/webview2-com-sys-*/out/x64/`) + `plugins/`. Le runtime WebView2 est fourni par Windows 10/11.
  - L'icône de l'exe et de la zone de notification vient de `crates/bin/assets/homemaid.ico` (généré par `app/tools/icon.py`, compilé par `crates/bin/build.rs` avec `embed-resource`).
- **Flags** (serveur, tests) : `--core`, `--agent`, `--config`, `--log-file` comme avant. Le binaire est en sous-système GUI ; il se rattache au terminal parent s'il y en a un.
- **Option avancée, hub au boot** : le core en **service Windows** (`--service`). L'agent doit alors tourner à part à l'ouverture de session, car un service tourne en session 0 et ne peut pas injecter souris ou clavier :
  ```
  sc.exe create HomeMaid binPath= "C:\HomeMaid\homemaid.exe --core --service --config C:\HomeMaid\homemaid.toml --log-file C:\HomeMaid\core.log" start= auto
  reg add HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v HomeMaidAgent /d "\"C:\HomeMaid\homemaid.exe\" --agent --config C:\HomeMaid\homemaid.toml --log-file C:\HomeMaid\agent.log"
  ```
- Ports : core 8123, agent 8124. Les autoriser dans le pare-feu Windows (réseau privé).

**Limite du mode PC = runner** : quand le PC est éteint ou en veille, le hub n'existe plus. Plus d'automatisations, plus de pilotage via le core, et pas de Wake-on-LAN possible. Parades à implémenter :
- **Délégation des plannings aux appareils** : quand un appareil supporte des plannings embarqués (la P300 le fait), le plugin y pousse les plannings simples, qui continuent alors PC éteint. Ça s'intègre au modèle état désiré : un planning est un état désiré daté, et le plugin choisit s'il l'exécute lui-même ou le délègue à l'appareil.
- **Mode dégradé dans l'app** (plus tard) : si le core est injoignable, l'app peut piloter directement les appareils en local.
- La cible long terme pour un usage fiable au quotidien reste un host toujours allumé.

### Core (Rust)
- Runtime : `tokio`.
- API : `axum` (REST + WebSocket), streaming d'état en **JSON-RPC 2.0 sur WebSocket** (décidé). Une seule API publique pour l'UI et les clients tiers.
- Modèle :
  - `Device` : un appareil physique (une multiprise).
  - `Entity` : un élément contrôlable ou mesurable (une prise de la multiprise). Elle a un UUID, un nom, une pièce, des tags et une liste de capacités.
  - `Capability` : un type, un état observé, un état désiré, des métadonnées (min/max, unités).
  - Les **capacités custom** déclarées par un plugin ont un schéma et un rendu générique dans l'UI.
- Réconciliateur : boucle qui compare désiré et observé, réessaie avec backoff et expose le statut des commandes.
- Découverte : mDNS (`mdns-sd`), et UDP broadcast selon les plugins.

### Plugins (WASM)
- `wasmtime` + Component Model, interfaces définies en **WIT** dans un crate/dossier `plugin-api/`. Liaisons générées avec `wit-bindgen`.
- Cible de compilation des plugins : `wasm32-wasip2`. Un seul `.wasm` par plugin et par version, valable pour toutes les plateformes du core.
- Ce que le core expose aux plugins (capabilities explicites, rien d'autre) : sockets réseau **local uniquement**, timers, logs, lecture/écriture de l'état de leurs entités, stockage clé-valeur privé, secrets.
- Ce que les plugins exportent : `init`, `discover`, `apply(desired_state)`, `refresh`, un manifest (id, version, version de `plugin-api` supportée, permissions demandées).
- Décisions v1 (implémentées dans `core/plugin-api/wit/plugin.wit`) :
  - **ABI** : capacités en chaînes (`kind = "on_off"`, valeur et meta en JSON). Le SDK les expose typées via le crate `caps`, et le core valide. Ajouter une capacité reste compatible avec v1. Tout changement de record ou de signature WIT impose une nouvelle version majeure ; un import ajouté est additif (1.x).
  - **Événements** : poll adaptatif. Un thread par plugin, `apply` dès qu'une commande arrive, sinon `refresh() -> délai en ms`. L'export s'appelle `refresh`, car `poll` entre en conflit avec la libc. Le vrai push arrivera en v2 avec WASIp3 async.
  - **Identité** : UUID v5 déterministe de `"<plugin>/<local_id>"`, donc rien à stocker. Le namespace ne doit jamais changer (test figé).
  - **Permissions** accordées dans `homemaid.toml` (`[[plugin]] permissions`). Les secrets sont dans `<data_dir>/secrets.toml`, le KV privé dans `<data_dir>/plugins/<id>.kv.json`.
  - **plugin-api 1.1** (additif, package WIT `@1.1.0`) : **actions ponctuelles**. Un plugin qui exporte le monde `plugin-with-actions` (`list-actions`, `invoke`) déclare des actions par entité (`play_pause`, `lock`…, paramètres en schéma JSON). Ce ne sont pas des états : le réconciliateur ne les renvoie jamais.
    - SDK : trait `Actions` et `export_with_actions!`. L'hôte détecte l'export `invoke` ; un plugin 1.0 se charge toujours (fixture `tests/fixtures/mock-plugin-api-1.0.wasm`).
    - API : `entity.invoke { id, action, params? }`.
  - **plugin-api 1.2** (additif, package WIT `@1.2.0`) : **plannings confiés aux appareils**, pour qu'ils continuent hub éteint. Un plugin qui exporte le monde `plugin-with-schedules` (celui de 1.1, plus `set-schedules(local-id, list<schedule>)`) reçoit, pour chaque entité au démarrage, les plannings que son appareil peut tenir seul. `schedule` = automatisation, capacité, valeur, `time-of-day` (`clock(minutes)`, `sunrise(décalage)`, `sunset(décalage)`), jours (lundi = bit 0).
    - Contrat : la liste remplace ce que le core avait confié avant (vide = tout retirer) ; le plugin ne touche jamais les règles faites dans l'app du fabricant ; `Ok` = l'appareil les tient (l'app l'affiche) ; `Err` = non (non supporté, injoignable) : le plugin peut les poser plus tard, le core les exécute de toute façon.
    - Le core (`Automations::schedules_for`) ne confie que les automatisations dont tous les déclencheurs sont des `at` (heure ou soleil) et dont les seules conditions sont des jours, et seulement leurs actions `set`. Il continue de les exécuter lui-même : les deux mettent le même état. `automations.list` donne `delegated` (les appareils qui les tiennent aussi).
    - Les sélecteurs sont résolus au démarrage du plugin, puis de nouveau quand les automatisations changent et après `entity.update` ou `room.update` (`Job::Reschedule`). SDK : trait `Schedules` et `export_with_schedules!`. Un plugin 1.0 ou 1.1 se charge toujours.
  - **plugin-api 1.3** (additif, package WIT `@1.3.0`) : **appareils ajoutés depuis l'app**. Le plugin est un pilote sans réglages ; chaque appareil (une multiprise) est ajouté à part, avec son formulaire (`device_settings` de l'index : adresse, identifiants). Un plugin qui exporte le monde `plugin-with-devices` (celui de 1.2, plus `add-device(id, settings) -> result<list<entity>, string>`) reçoit, après `init` et avant `discover`, chaque appareil enregistré (secrets compris), et celui que l'utilisateur ajoute à chaud.
    - Contrat : `add-device` renvoie les entités de l'appareil, donc le core sait à quel appareil ajouté chacune appartient (`Entity.added`) et les retire avec lui. Ajout : `Err` refuse l'appareil (le message s'affiche dans le formulaire), rien n'est enregistré. Au démarrage : `Err` le laisse de côté jusqu'au prochain démarrage (`PluginState.devices`, montré dans l'app), sans arrêter le plugin ; un appareil injoignable peut renvoyer ses dernières entités connues. Retirer un appareil redémarre le plugin sans lui (pas d'export `remove`).
    - Stockage : `[[plugin.device]]` (`id` aléatoire de 8 caractères, `[plugin.device.settings]`) dans `homemaid.toml`, secrets dans la table `["<plugin>/<id>"]` de `secrets.toml` (`config::device_secrets`) ; `plugins::take_secrets` les remet dans les réglages en mémoire. Un secret vide à l'ajout reprend celui du dernier appareil du plugin (même compte pour une seconde multiprise).
    - API : `device.add { plugin, settings }` (renvoie `{ id }`), `device.remove { plugin, id }`, puis `plugins.changed` ; `plugins.list` donne `device_settings` et `devices` (`id`, réglages, clés des secrets, `error`). SDK : trait `Devices` et `export_with_devices!`. Un plugin 1.0 à 1.2 se charge toujours (fixtures `mock-plugin-api-1.0.wasm`, `mock-plugin-api-1.2.wasm`).
    - App : « + » de l'écran Maison (un appareil d'un plugin installé, un PC par son QR, ou un autre en installant son plugin), « Retirer cet appareil » dans le panneau ; installer un plugin à appareils ouvre aussitôt le formulaire du premier.
  - **Sandbox** : pas de fichiers, d'env ni de DNS ; sockets vers le réseau local uniquement, **sans loopback** (le hub, l'agent et leurs pages `/pair` y écoutent) sauf permission `loopback` accordée dans la config (le plugin `pc` quand l'agent est sur le même PC) ; 64 MiB de mémoire, deux mémoires au plus ; KV de 1 MiB au plus, écrit une fois par appel ; 60 lignes de log par minute ; 60 s max par appel (temps réel, attente réseau comprise : ça attrape les plugins emballés, les timeouts réseau sont gérés par le plugin) ; redémarrage après crash avec un délai de 1 s à 60 s.
- SDK Rust pour écrire un plugin, plus un **template** `cargo generate` (dans le repo du registre, `template/`) donnant un plugin fonctionnel en quelques minutes.

### Registre de plugins
- Repo `ErwanHeschung/HomeMaid-plugins` (clone local `~/projects/HomeMaid-plugins`) : les **plugins officiels** (`plugins/<id>`, workspace Cargo), `index.json`, `check.py` + CI, `tools/release.py`, et le template (`cargo generate --git … --subfolder template`).
- `index.json` (`format: 1`) : par plugin `id`, `name`, `description`, `repo`, `key` (clé publique minisign de l'auteur), `settings` (`key`, `label`, `secret`, `required` : ce que l'app demande à l'installation), `device_settings` (même forme : ce qu'elle demande pour chaque appareil ajouté, plugin-api 1.3), et `versions` (`version`, `api` = plugin-api minimale `1.x`, `permissions`, `url` du `.wasm` en release GitHub, `sha256`, `signature` = le `.minisig` entier).
- Pour publier un plugin : une PR sur `index.json` (le template a un workflow de release sur tag `v*` qui produit `.wasm`, `.minisig` et `entry.json`).
- **Plugins officiels** : dans HomeMaid-plugins, publiés indépendamment du hub et de l'app. Chacun a un `registry.json` (sa fiche : nom, description, permissions, réglages). Ils épinglent le SDK sur un **tag de HomeMaid** (`plugin-api-v1.3.0`), et leurs tests les font tourner sur un vrai hub au même tag (crate `homemaid` en dépendance git). Publier = monter la version dans `Cargo.toml` et fusionner : `release.yml` liste les versions absentes de `index.json`, attend ton approbation (environnement GitHub `release`, `main` seulement), puis construit, signe avec la clé de la CI (sans mot de passe, secret `MINISIGN_KEY` ; clé publique `minisign.pub` ; copie de secours `~/.minisign/homemaid-ci.key`), publie la release `<id>-v<version>` et ajoute la version à l'index. HomeMaid ne garde que `pc` (le pont vers son propre agent, livré avec l'exe) et `mock` (tests du hub).
- **Confiance** : la signature garantit « publié par la CI de HomeMaid », donc par qui contrôle le compte GitHub (plus l'approbation de l'environnement). Clé de l'auteur **épinglée à l'installation** (`key` dans le `[[plugin]]`) ; une mise à jour signée par une autre clé est refusée même si l'index change. La CI du registre refuse un changement de clé sans le label `key-change`.
- Implémenté dans `core/crates/homemaid/src/registry.rs` (`ureq`, `minisign-verify`) et `api.rs`. **Le local d'abord** : Internet n'est lu que par `registry.list` et `plugin.install`, sur demande. URL par défaut `registry::DEFAULT_URL`, remplaçable par `[core] registry`.
  - `plugins.list` (sans réseau) : plugins du `homemaid.toml`, `source` (`registry` ou `local` = écrit à la main, géré à la main), version, erreur, réglages ; les secrets ne ressortent jamais (seulement leurs clés).
  - `registry.list` : la version la plus récente que ce core peut faire tourner (`API_MINOR` dans `plugins.rs`), ce qui est installé, `update`.
  - `plugin.install { id, version, permissions, settings? }` : la version et les permissions **que l'utilisateur a vues** (sinon refus). Téléchargé, sha256 et signature vérifiés, puis `<data_dir>/plugins/<id>.wasm` + `<id>.json` (l'entrée, pour le nom et les réglages hors ligne), secrets dans `secrets.toml`, `[[plugin]]` écrit (`config::save_plugin`, avec `version` et `key`), démarré **à chaud**. Une mise à jour garde les réglages.
  - `plugin.configure { id, settings }` (un secret vide est gardé), `plugin.remove { id }` (entités, `.wasm`, KV retirés ; secrets gardés). Notification `plugins.changed`, aussi quand un plugin démarre ou échoue.
  - Hôte : `Plugins::spawn` / `stop` / `remove` démarrent et arrêtent un plugin à chaud ; `stop` attend la fin de son thread (≤ 60 s) pour que deux instances n'écrivent jamais son KV en même temps.
- App : Réglages → Plugins (`lib/screens/plugins.dart`) : installés (état, réglages, retirer), disponibles (installer, mettre à jour), feuille avec les permissions en mots et les réglages ; la démo en a en mémoire. Aucun changement de `plugin-api`.

### Agent PC (Rust)
- Daemon léger. **Cible prioritaire : Windows** (le PC de l'auteur), Linux ensuite. Il s'annonce en mDNS.
- Il est embarqué dans le binaire `homemaid` (`--agent`) et peut aussi tourner seul.
- Fonctions : entrées souris/clavier (`enigo`), touches média, volume, verrouillage, veille, extinction, lancement d'apps, capteurs (CPU, RAM, fenêtre active).
- Implémenté (étape 4) dans `core/crates/homemaid/src/agent/` :
  - fonctions : souris, clavier, média, volume/muet (Core Audio), capteurs (`sysinfo`), fenêtre active, lancement des seules applis de `[agent.apps]` ;
  - le code Windows est dans `agent/windows.rs`. Ailleurs, `agent/fallback.rs` journalise et enregistre les requêtes (tests) ;
  - accès au bureau par un thread unique ;
  - routes : `/ws` (canal direct, token), `/api` (plugin `pc`, Bearer), `/remote` (page web de test, en attendant l'app), `/pair` et `/hello` comme le core. Couper l'agent ferme aussi les sessions ouvertes ;
  - alimentation : verrouillage, veille et extinction (`{"type":"power","action":"lock|sleep|shutdown"}`), derrière un bouton rouge de la télécommande ; l'extinction demande confirmation ;
  - **services** (`[[agent.shortcut]]`, façon VorTraum) : nom, logo (`app/assets/logos`, Simple Icons, CC0), ce qui s'ouvre (une URL va au navigateur par défaut, sinon un programme), et les raccourcis clavier du lecteur (`restart`, `next_episode`, `skip_intro`… ; lecture, −5 s et +5 s ont Espace et les flèches par défaut). Aucune marque dans le code : les préréglages Netflix, YouTube, Spotify, Twitch et Crunchyroll sont dans `homemaid.example.toml` ;
  - **applis du PC** (opt-in, `[agent] discover_apps`, interrupteur « Applis de ce PC sur le téléphone » de la fenêtre) : l'agent lit le menu Démarrer (`Get-StartApps` en UTF-8, sans fenêtre de console) au démarrage puis toutes les 10 min, sans désinstalleurs, lisez-moi, aides. `launch` cherche d'abord `[agent.apps]` (un programme, ou `shell:AppsFolder\<AppID>` une fois épinglée), puis le menu Démarrer, ouvert par le shell comme depuis le menu. `list_apps` les donne au téléphone, `GET /icon?app=` (token en en-tête) leur icône du menu Démarrer en PNG (IShellItemImageFactory + WIC, sur un thread STA, en cache ; seulement pour les applis connues, 404 sinon). Ailleurs que sous Windows, rien encore ;
  - **géré depuis le téléphone** : `save_shortcut` / `delete_shortcut` (un site : **une adresse web seulement**, jamais un programme) et `pin_app` / `unpin_app` (seulement une appli trouvée dans le menu Démarrer), écrits dans `[[agent.shortcut]]` et `[agent.apps]` par `edit_toml`. L'agent redémarre ensuite (le serveur relit `[agent]` à chaque démarrage) et le téléphone se reconnecte avec le nouveau hello ;
  - **contexte de la télécommande** : la réponse `state` donne `launcher`, le lanceur (service ou appli de la bande) au premier plan, et `playing` (`title`, `artist`, `playing`). L'agent le trouve, dans l'ordre : par l'adresse de l'onglet actif du navigateur par défaut (UI Automation, premier champ d'édition de la fenêtre, relu seulement quand la fenêtre ou son titre change ; masqué en plein écran) comparée à l'hôte de l'`open` d'un service ; par le programme (nom de l'exe, AppUserModelID) comparé à l'AppID d'une appli ; en dernier par le nom dans le titre (avant l'appli dans un navigateur). `playing` vient des sessions média de Windows (`GlobalSystemMediaTransportControlsSessionManager`) : celle de l'appli au premier plan, sinon celle du volet média. `media` (`play_pause`, `next`, `previous`, et `back`/`forward` = ±5 s) passe par cette session quand elle l'accepte, sinon par les touches média et les flèches. L'app relit l'état toutes les 3 s tant que l'onglet PC est affiché. Sonde sur un vrai bureau : `cargo test --target x86_64-pc-windows-gnu -p homemaid --lib -- --ignored --nocapture what_is_in_front` ;
  - app : touche « + » au bout de la bande (catalogue de sites `app/assets/shortcuts.json`, données et non code ; autre site par adresse ; applis du PC à chercher, lancer, épingler), appui long pour retirer ;
  - pas encore fait : Wake-on-LAN.
- Deux canaux :
  - **Télécommande directe** téléphone ↔ agent en WebSocket ou UDP pour une latence minimale (trackpad). Authentifiée par appairage.
  - **Intégration au core** via un plugin `pc` : le PC devient une entité avec ses capacités (`Power`, `MediaControl`, `InputDevice`).
    - Implémenté : `volume`, `mute`, `custom:cpu`, `custom:memory`, `custom:active_window`.
    - Les **actions ponctuelles** (média, lancer une appli) ne passent pas par le core : un « suivant » n'est pas un état, et le réconciliateur le renverrait. Le concept d'action sera conçu à l'étape 6, avec les automatisations.
- Allumage : Wake-on-LAN, géré par le core.
- Pas de partage d'écran dans le périmètre initial (on utilisera RustDesk, Moonlight ou RDP à côté).

### UI mobile
- **Flutter** (décision prise), une seule codebase mobile + desktop. Animations fluides, style Tesla.
  - Pourquoi : moteur de rendu propre (Impeller), idéal pour une UI très custom et animée, et pour un trackpad à faible latence avec un contrôle fin des gestes. `flutter_rust_bridge` permet de partager du code Rust avec le core (types, client du protocole).
  - Alternatives écartées : Tauri mobile (WebView, le défaut de l'app HA), React Native (moins adapté à l'UI très custom), Compose (Android uniquement ou écosystème plus petit).
- Cible prioritaire : Android (le téléphone de l'auteur).
- Le rendu est piloté par les capacités : chaque type de capacité a un widget, et les capacités inconnues ont un rendu générique.
- Écran télécommande PC : trackpad plein écran, clavier, contrôles média.
- Accès hors du domicile : Tailscale recommandé. Aucun port exposé par défaut.
- Implémenté (étape 5) dans `app/` (Android, `dev.homemaid.homemaid`) :
  - **choix** : pas de `flutter_rust_bridge` (le protocole est réécrit en Dart, `lib/clients.dart`), état via `ChangeNotifier` + `ListenableBuilder`, sans librairie de state management, et thème sombre sobre en attendant la passe de design « Tesla » ;
  - écrans : Maison (pièces, puis un widget par capacité, `lib/widgets/capability.dart`), PC (télécommande ; gestes dans `lib/trackpad.dart`, en Dart pur testé), Réglages ;
  - **appairage** : sur le PC, `http://localhost:8123/pair` (maison) et `http://localhost:8124/pair` (PC) affichent un QR `homemaid://pair?kind&id&host&port&token`, servi en loopback seulement (adresse du client et en-tête `Host`, contre le DNS rebinding). Les tokens de moins de 16 caractères et ceux de l'exemple sont refusés. L'app le scanne, et la connexion est gardée dans `flutter_secure_storage` ;
  - **découverte** : le core et l'agent s'annoncent en mDNS (`_homemaid-core._tcp`, `_homemaid-agent._tcp`, TXT `id` = hostname, UDP 5353). Après deux échecs de connexion, l'app y cherche la nouvelle IP, et ne la garde que si l'hôte prouve connaître le token sans que l'app l'envoie (`/hello?nonce=` répond HMAC-SHA256(token, `homemaid-host:` + nonce)) ;
  - **API ajoutée** : notification `entity.changed`, et `entity.update { id, name?, room?, tags?, model? }`, réécrit dans `homemaid.toml` via `toml_edit` en gardant les commentaires. Puis `rooms.list` et `room.update { name, kind }`, avec la notification `room.changed` ;
  - `usesCleartextTraffic` : `ws://` sur le réseau local, sans TLS (l'accès distant passe par Tailscale).
- **Identité visuelle « Viewport »** : la maison comme une maquette CAO vivante. Contrat de direction dans `.impeccable/surfaces/`, contexte produit dans `PRODUCT.md`, système détaillé dans `DESIGN.md`.
  - **Grammaire d'état**, partout identique (viewport, lignes, panneau) :
    - **lavande** (le milieu du dégradé du logo, couleur principale) = physiquement allumé, d'après l'état **observé** uniquement : un souhait en attente n'allume jamais rien ;
    - **hachures et texte roses** (le rose du logo, couleur secondaire) = commande en attente, et rien d'autre ;
    - **fantôme ou contour** = hors ligne ;
    - **rouge** = échec ;
    - la lavande sert aussi à tout le reste : HomeMaid (onglet actif, bouton principal), interaction, sélection, télécommande. Il n'y a ni bleu ni ambre dans l'app.
  - **Icône** : le logo (`app/tools/logo.png`, 1024 px, fond transparent) sur le fond graphite `#15171B` (`res/values/colors.xml`, aussi l'écran de lancement). La démo porte une étiquette « DÉMO ». Régénérer depuis `app/` : `uv run --with pillow python tools/icon.py tools/logo.png`.
  - **Thème** : `app/lib/theme.dart`. Fond graphite, police Barlow embarquée (`assets/fonts`, licence OFL), sombre uniquement (usage le soir, en intérieur).
  - **Modèles 3D** : rendus studio pré-calculés, pas de 3D temps réel. `app/tools/render/render.py` (Blender 5.2.2 Cycles, dans `~/.local/opt`) modélise les archétypes génériques (barrettes de 2 à 6 prises, prise, lampe, écran, module). Il rend 24 vues de rotation, plus une couche de lumière par élément allumable, teintée en lavande par l'app. `pack.sh` convertit en WebP, estompe les bords des ombres et inscrit la provenance.
    - Commande, depuis `app/` : `~/.local/opt/blender-5.2.2-linux-x64/blender -b -P tools/render/render.py && IMPECCABLE=<chemin du script impeccable> ./tools/render/pack.sh`.
    - Les conventions de caméra (orthographique, inclinaison 0,62, `ORTHO_SCALE` 3,2) doivent rester identiques entre `render.py` et `lib/viewport/sprites.dart`.
    - Le modèle est choisi d'après les **capacités**, jamais la marque (`lib/viewport/models.dart`). L'utilisateur peut aussi le choisir dans l'app (`model` dans `[[entity]]`).
  - **Pièces meublées** : chaque pièce est une maquette « maison de poupée » (deux murs coupés, deux côtés ouverts). `app/tools/render/room.py` modélise 7 types (`living`, `kitchen`, `bedroom`, `office`, `bathroom`, `entrance`, `generic`) en 3 tailles. Chacun est rendu en 7 vues sur un arc de 90°, au pas des 15° des modèles.
    - `assets/rooms/rooms.json` décrit le cadrage et les **emplacements** (murs libres, dessus de meubles, fenêtres, porte, sol, suspensions), calculés d'après les meubles, ainsi que l'encombrement de chaque modèle.
    - L'app (`lib/viewport/rooms.dart`) pose les appareils selon leurs préférences d'emplacement et leur échelle « diorama » (les petits appareils sont agrandis pour rester lisibles). Elle choisit la plus petite taille qui les contient tous.
    - Le type de pièce est déduit du nom (FR/EN), ou choisi dans l'app : `[[room]] name, kind` dans `homemaid.toml`, via `rooms.list` et `room.update`.
    - Commande : `blender -b -P tools/render/room.py` (environ 30 min, ou `-- --json` pour régénérer seulement le JSON), puis `pack.sh`. Les plafonniers et ampoules suspendus ont leurs propres rendus (`*_hung`).
  - **Fluidité** : toujours juger animations et rotation sur un build **release** (`flutter build apk --release --flavor …`). Le debug est bien plus lent.

### Stockage
- Config déclarative en **TOML** (décidé), dans un dossier versionnable.
- État courant : en mémoire, plus un snapshot persistant.
- Historique : à trancher (SQLite avec tables d'agrégats, ou DuckDB). Prévoir le downsampling dès le départ.

### Sécurité
- Auth des clients par token, appairage des appareils (téléphone, agent PC) par QR code.
- Plugins sandboxés, sans accès au système de fichiers ni à Internet par défaut.
- Signature des plugins du registre.

## Matériel de l'auteur (cibles des plugins officiels)

- **TP-Link Tapo P300** (multiprise Wi-Fi, 3 prises commutables individuellement, USB non commutable).
  - Contrôle local. Les appareils Tapo récents utilisent le protocole chiffré **KLAP**, avec les identifiants du compte TP-Link pour l'auth locale. Référence d'implémentation : la librairie Python `python-kasa`, qui supporte la P300.
  - Une entité par prise, capacité `OnOff`.
  - Attention : certaines mises à jour firmware TP-Link ont déjà cassé l'accès local.
  - Implémenté dans `HomeMaid-plugins/plugins/tplink`, en **KLAP v2** (`lv: 2`). Depuis la 0.2.0 (plugin-api 1.3), chaque multiprise est un appareil ajouté depuis l'app (adresse IP, e-mail et mot de passe du compte TP-Link) ; le plugin n'a plus de réglages.
    - **Prérequis** : activer « Compatibilité tierce » dans l'app Tapo (Moi → Services tiers, FAQ TP-Link 4416). Sinon, les firmwares récents (1.4.x) parlent **TPAP** (SPAKE2+), non supporté. Seule référence connue : le crate Rust `tapo` 0.11.1.
    - Prévoir une réservation DHCP pour l'IP. La découverte UDP 20002 n'est pas faite.
- **Ampoules connectées** : modèle à préciser.
- **PC** : via l'agent PC.

### Automatisations (étape 6)
- Écrites dans `homemaid.toml` (`[[automation]]` avec `[[automation.trigger]]`, `[[automation.condition]]`, `[[automation.action]]`), exemples dans `homemaid.example.toml`. Le module est `core/crates/homemaid/src/automations.rs`.
- **Vérifiées au chargement** : une faute (heure, jour, capacité, valeur, sélecteur, champ inconnu) empêche le core de démarrer, avec un message précis. Ce qui dépend des appareils découverts (sélecteur vide, action absente) est signalé comme « problème », sans bloquer.
- **Déclencheurs** :
  - `at` (heure locale, `days`), via le crate `jiff`. Un `at` manqué de plus de 5 min (PC en veille, hub arrêté) est sauté, pas rattrapé ;
  - `at` accepte aussi le soleil : `"sunrise"`, `"sunset"`, décalés en minutes (`"sunset-30"`, `"sunrise+15"`, ±360 au plus, la limite des appareils). Calcul local par le crate `sunrise` (équation solaire, Soleil à 0,833° sous l'horizon), d'après `latitude` et `longitude` de `[core]`, obligatoires seulement si le soleil sert. L'app les règle (Réglages, ou depuis l'éditeur quand le soleil est choisi sans emplacement) par `home.set_location { latitude, longitude }` : écrit dans `[core]`, automatisations recompilées aussitôt ; `home.location` les lit. Un jour sans lever ni coucher (régions polaires) ne déclenche rien ;
  - `when` (un état **observé** franchit une valeur : `becomes`, `above`, `below`, `changes`). Seul le front compte, et un souhait ne déclenche jamais.
- **Conditions** : `between` (passe minuit, accepte aussi `sunrise`/`sunset` du jour), `days`, `state` (`is`, `above`, `below`, `all`).
- **Actions** : `set` (un état désiré, le réconciliateur fait le reste) ou `invoke` (une action ponctuelle).
- **Sélecteurs** (`selector.rs`) : `room:`, `tag:`, `cap:`, `plugin:`, `device:`, `model:`, `id:`, `name:"…"`, avec `&`, `|`, `!` et des parenthèses. Jamais d'UUID ni de nom fragile dans une automatisation.
- **Dry-run** (`automation.dry_run`) : conditions avec leur raison, et appareils visés avec « à faire » ou « déjà fait ». Rien n'est envoyé.
- **Journal** : les 20 dernières exécutions en mémoire (`automations.list`, notification `automation.ran`).
- **Garde-fou** : plus de 10 exécutions par minute arrêtent l'automatisation jusqu'au redémarrage (une boucle entre deux automatisations).
- **Modifiables à chaud** par l'API (l'app les édite) : `automation.save { automation }` (créée ou remplacée par id) et `automation.delete { id }` réécrivent leur `[[automation]]` dans `homemaid.toml` (`config::save_automation`, sur `edit_toml` : le reste du fichier et ses commentaires sont gardés ; une nouvelle va à la fin du fichier), mais seulement si **toutes** les automatisations du fichier ainsi modifié se compilent (mêmes erreurs qu'au démarrage, retouches à la main comprises). Puis `Core::set_automations` les met en vigueur sans redémarrer : historique gardé par id, moteur relancé à partir de maintenant (pas de rattrapage), plannings renvoyés aux appareils, notification `automations.changed`. `automations.list` donne aussi `config` (l'automatisation telle qu'écrite) ; `automation.dry_run { automation }` essaie une automatisation pas encore enregistrée. Modifier une automatisation la sort du garde-fou.
- **App** : onglet Scénarios (phrases, prochaine exécution, journal, Essayer et Lancer, interrupteur par ligne). Les actions apparaissent aussi en touches dans le panneau d'un appareil. Une automatisation confiée à des appareils l'indique (« Continue même hub éteint, sur : … »).
  - **Éditeur** (`lib/screens/automation_editor.dart`, plein écran, « + » de la liste ou crayon du panneau) : nom, active, puis Quand / Seulement si / Alors en cartes (type en `SegmentedButton`, champs, retirer). Couvre tout : moment (heure, lever, coucher avec décalage, jours) ou appareil (devient, dépasse, passe sous, change) ; plage horaire, jours, état (+ tous) ; régler (valeur selon les métadonnées : interrupteur, curseur, saisie) ou lancer une action (paramètres d'après son schéma).
  - **Cibles** : une pièce (ou toute la maison) et une capacité, `room:"Salon" & cap:on_off` (`lib/selectors.dart`), jamais un nom ni un UUID ; l'appli montre les appareils concernés. Un sélecteur écrit à la main reste modifiable en texte. L'id d'une nouvelle automatisation vient de son nom (`slug`) et ne change plus.
  - Paramètres d'une action : un menu quand son schéma a un `enum` (le plugin `pc` y met les applis et services de l'agent, que son état `{"type":"state"}` liste désormais), un champ sinon ; la phrase les dit (« Lancer notepad sur PC »). L'action proposée par défaut est une que la maison sait faire.
  - Essayer = `automation.dry_run { automation }` sur le brouillon ; Enregistrer = `automation.save`, dont le refus s'affiche dans le formulaire sans rien perdre. L'appli lit `config` (les automatisations telles qu'écrites) ; la démo enregistre en mémoire.
- **Phase B, faite** : soleil, et plannings confiés aux appareils (plugin-api 1.2). Le plugin `tplink` les pose en règles de planning sur chaque prise de la P300 (`control_child` + `add_schedule_rule`, format du crate `tapo`, mesuré sur P110). Il ne retire que ses propres règles (ids dans son KV), réessaie quand la multiprise revient, et la P300 calcule lever et coucher d'après la localisation et le fuseau réglés dans l'app Tapo. **Non vérifié sur une vraie P300** : lancer `cargo run -p tplink-plugin --example probe` dans HomeMaid-plugins (voir l'exemple) et ajuster si besoin.

## Roadmap MVP

1. **Core minimal** : modèle Entity/Capability, état désiré/observé, réconciliateur, API WebSocket, config déclarative. Binaire `homemaid` avec les flags `--core` et `--agent` dès le départ.
2. **API plugin v1** (WIT) + hôte wasmtime + SDK Rust.
3. **Plugin TP-Link** (P300), qui sert aussi de plugin de référence et d'exemple.
4. **Agent PC** (Windows) + écran télécommande dans l'app. Mode combiné `--core --agent`, et service Windows.
5. **App Flutter** : liste par pièce, contrôle des capacités de base.
6. **Automatisations** typées + dry-run, avec **délégation des plannings** aux appareils qui le supportent. *(Faite : automatisations, sélecteurs, dry-run, actions ponctuelles, soleil, plannings confiés aux appareils ; reste à vérifier sur la P300 réelle.)*
7. **Registre de plugins** (index GitHub, vérification de signature). *(Fait : HomeMaid-plugins avec `tplink` 0.1.0, installation depuis l'app, publication signée par la CI.)*
8. Historique avec downsampling.
9. (Plus tard) Mode dégradé de l'app sans core, et migration vers un host dédié.

## Organisation des repos

- `core` : serveur, `plugin-api` (WIT + SDK), plugins `pc` et `mock` dans `plugins/`.
- `app` : application Flutter.
- `agent` : agent PC.
- `HomeMaid-plugins` : les plugins officiels (`plugins/`), le registre (`index.json`) et le template `cargo generate` (`template/`).

**Release** du hub et de l'app : pousser un tag `vX.Y.Z` (`git tag v0.2.0 && git push origin v0.2.0`). La CI (`ci.yml`, job `release`) refait tout, puis publie la release GitHub : le dossier Windows en zip, les APK (vraie app et démo), et pour notes la section de la version dans `CHANGELOG.md` (format Keep a Changelog, écrite pour les utilisateurs, en anglais, une ligne par paragraphe ou puce car GitHub garde les retours à la ligne), suivie de l'installation et du lien de comparaison. Sans section `## [X.Y.Z]`, pas de release : la remplir depuis `[Unreleased]` avant le tag. Un suffixe (`v0.1.0-alpha.1`) en fait une pré-version. Les tags `plugin-api-v*` et `<plugin>-v*` ne sont pas concernés. Monter `version` dans `app/pubspec.yaml` (le `+N` est le versionCode Android) et dans les `Cargo.toml` avant le tag. Sans les secrets `ANDROID_KEYSTORE…`, les APK sont signés par une clé jetable et ne s'installent pas par-dessus une version précédente.

Un monorepo est acceptable au début. Les plugins et leur registre sont à part : leurs releases ne dépendent pas de celles du hub et de l'app, l'URL du registre est inscrite dans chaque core installé, et les PR de publication ne touchent pas le code du hub.

Disposition actuelle du monorepo :
- `core/` : workspace Cargo. Lancer `cargo` depuis ce dossier.
  - `crates/homemaid` : la **librairie serveur** (hub, agent, API, config). `server.rs` les démarre : `server::run` pour les flags, `server::Handle` (`status`, `set_agent`, `pairing_qr`, `stop`) pour un client. Aucune dépendance graphique.
  - `crates/desktop` (`homemaid-desktop`) : le **client PC** Windows (zone de notification, fenêtre WebView2). Il ne connaît du serveur que `Handle`.
  - `crates/bin` (`homemaid-bin`) : le binaire `homemaid`, qui analyse les flags et lance soit le serveur, soit le client. Il porte aussi l'icône de l'exe et le service Windows.
  - `crates/caps`, `plugin-api/`, `plugins/`.
- `app/` : application Flutter.

## Environnement de développement

Le développement se fait sous **WSL2 (Ubuntu)** sur le PC Windows de l'auteur.

- **Core + plugins** : développés et testés dans WSL. Plugins compilés en `wasm32-wasip2`.
- **Réseau WSL** : le mode mirrored est obligatoire pour que le téléphone et les appareils du LAN voient le core. Dans `%UserProfile%\.wslconfig` :
  ```ini
  [wsl2]
  networkingMode=mirrored
  ```
  Ensuite, `wsl --shutdown`, puis autoriser les ports entrants dans le pare-feu Windows (Hyper-V firewall). Le multicast (mDNS) peut rester capricieux sous WSL : toujours prévoir une configuration par **IP directe** en plus de la découverte.
- **Agent PC et mode PC = runner** : ils doivent tourner **côté Windows**, car l'injection souris/clavier est impossible depuis WSL. Deux options :
  - cross-compiler depuis WSL (`cargo-xwin` pour la cible `x86_64-pc-windows-msvc`, ou `x86_64-pc-windows-gnu` avec mingw) puis lancer le `.exe` sous Windows ;
    - **choisi** : mingw-w64 (`apt install mingw-w64`), puis `cargo build --release -p homemaid-bin --target x86_64-pc-windows-gnu`. L'exe ne dépend que de DLL système ;
    - vérification sans quitter WSL : `cargo clippy -p homemaid -p homemaid-desktop -p homemaid-bin --all-targets --target x86_64-pc-windows-gnu`. Les tests du bureau factice (`tests/agent.rs`, `tests/plugin_pc.rs`) sont exclus sous Windows ;
  - builder directement avec cargo sous Windows.
  Le code spécifique Windows est isolé derrière `#[cfg(windows)]` pour que le reste compile et se teste sous Linux.
- **App Flutter** : SDK dans WSL. Tests sur le téléphone physique via **débogage ADB Wi-Fi** (`adb pair` puis `adb connect`, Android 11+). Pas d'émulateur dans WSL. Alternative : Flutter installé côté Windows pour l'app seulement.
  - Installé : Flutter dans `~/flutter`, SDK Android dans `~/Android/Sdk` (PATH et `ANDROID_HOME` dans `~/.bashrc`), JDK `openjdk-25-jdk-headless` (LTS ; compatible avec Gradle 9.3.1 et AGP 9.1, alors que la 26, non LTS, risque de ne pas être supportée par Gradle).
  - Commandes, depuis `app/` : `flutter analyze`, `flutter test`, `flutter run --flavor real` (téléphone connecté via `adb connect`).
  - **Démo** : `flutter run --flavor demo --dart-define=HOMEMAID_DEMO=true` installe « HomeMaid démo » à côté de la vraie app (`lib/demo.dart`). C'est une maison synthétique couvrant tous les modèles et tous les états, étiquetée « Données de démonstration » dans l'interface. À utiliser pour le design, les captures et la découverte du projet.

## Consignes pour Claude Code

- **Vérifier les versions actuelles** des crates et outils (tokio, axum, wasmtime, wit-bindgen, Flutter…) avant de les ajouter, sans se fier à la mémoire.
- Ne jamais ajouter de logique spécifique à une marque dans le core.
- Toute évolution de `plugin-api` doit être explicitement versionnée et signalée, avec un tag `plugin-api-vX.Y.Z` sur HomeMaid (ce que les plugins épinglent).
- Tests : tests unitaires sur le réconciliateur et le modèle de capacités, et un plugin mock pour les tests d'intégration.
- Le code et les commentaires sont en anglais. La documentation utilisateur peut être en français.
- En cas de choix d'architecture non tranché (marqué « à trancher »), proposer les options et demander avant d'implémenter.
