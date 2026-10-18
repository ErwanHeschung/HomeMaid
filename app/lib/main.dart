import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'clients.dart';
import 'connections.dart';
import 'demo.dart';
import 'discovery.dart';
import 'l10n/app_localizations.dart';
import 'screens/home.dart';
import 'screens/pair.dart';
import 'screens/remote.dart';
import 'screens/settings.dart';
import 'theme.dart';
import 'viewport/rooms.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  SystemChrome.setEnabledSystemUIMode(SystemUiMode.edgeToEdge);
  final connections = demoMode ? Connections.memory(demoConnections) : Connections();
  await connections.load();
  await RoomCatalog.load();
  runApp(HomeMaidApp(connections: connections));
}

class HomeMaidApp extends StatelessWidget {
  const HomeMaidApp({super.key, required this.connections});

  final Connections connections;

  @override
  Widget build(BuildContext context) => MaterialApp(
        onGenerateTitle: (_) => 'HomeMaid',
        debugShowCheckedModeBanner: false,
        theme: Hm.theme(),
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Shell(connections: connections),
      );
}

/// Bottom navigation, and one live client per paired host, kept in sync with the pairings.
class Shell extends StatefulWidget {
  const Shell({super.key, required this.connections});

  final Connections connections;

  @override
  State<Shell> createState() => _ShellState();
}

class _ShellState extends State<Shell> {
  int _tab = 0;
  CoreClient? _core;
  final _agents = <String, AgentClient>{};

  Connections get _connections => widget.connections;

  @override
  void initState() {
    super.initState();
    _connections.addListener(_sync);
    _sync();
  }

  @override
  void dispose() {
    _connections.removeListener(_sync);
    _core?.dispose();
    for (final agent in _agents.values) {
      agent.dispose();
    }
    super.dispose();
  }

  /// (Re)creates clients whose pairing changed: new host or token after pairing again.
  void _sync() {
    if (demoMode) {
      _core ??= DemoCore();
      _agents.putIfAbsent('DEMO-PC', demoAgent);
      if (mounted) setState(() {});
      return;
    }
    bool same(Connection a, Connection b) => a.host == b.host && a.port == b.port && a.token == b.token;
    final core = _connections.core;
    if (core == null || _core == null || !same(core, _core!.connection)) {
      _core?.dispose();
      _core = core == null ? null : CoreClient(core, onMoved: _connections.put, locate: locate);
    }
    final wanted = {for (final a in _connections.agents) a.id: a};
    for (final id in _agents.keys.toList()) {
      if (wanted[id] == null || !same(wanted[id]!, _agents[id]!.connection)) _agents.remove(id)!.dispose();
    }
    for (final MapEntry(key: id, value: connection) in wanted.entries) {
      _agents.putIfAbsent(id, () => AgentClient(connection, onMoved: _connections.put, locate: locate));
    }
    if (mounted) setState(() {});
  }

  Future<void> _pair() async {
    final connection = await Navigator.push<Connection>(context, MaterialPageRoute(builder: (_) => const PairScreen()));
    if (connection == null) return;
    await _connections.put(connection);
    if (!mounted) return;
    final l = AppLocalizations.of(context);
    setState(() => _tab = connection.kind == 'core' ? 0 : 1);
    final message = connection.kind == 'core'
        ? [l.pairedHome(connection.id), if (_connections.agents.isEmpty) l.alsoScanPc]
        : [l.pairedPc(connection.id), if (_connections.core == null) l.alsoScanHome];
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(content: Text(message.join(' ')), duration: const Duration(seconds: 6)),
    );
  }

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    return Scaffold(
      body: SafeArea(
        bottom: false,
        child: IndexedStack(index: _tab, children: [
          HomeScreen(core: _core, onPair: _pair),
          RemoteScreen(agents: _agents.values.toList(), connections: _connections, onPair: _pair),
          SettingsScreen(connections: _connections, onPair: _pair),
        ]),
      ),
      bottomNavigationBar: NavigationBar(
        selectedIndex: _tab,
        onDestinationSelected: (i) => setState(() => _tab = i),
        destinations: [
          NavigationDestination(icon: const Icon(Icons.view_in_ar_outlined), selectedIcon: const Icon(Icons.view_in_ar), label: l.navHome),
          NavigationDestination(icon: const Icon(Icons.mouse_outlined), selectedIcon: const Icon(Icons.mouse), label: l.navPc),
          NavigationDestination(icon: const Icon(Icons.tune_outlined), selectedIcon: const Icon(Icons.tune), label: l.navSettings),
        ],
      ),
    );
  }
}
