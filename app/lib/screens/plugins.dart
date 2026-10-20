import 'package:flutter/material.dart';

import '../clients.dart';
import '../l10n/app_localizations.dart';
import '../model.dart';
import '../theme.dart';
import 'common.dart';

Future<void> showPlugins(BuildContext context, CoreClient core) =>
    Navigator.of(context).push<void>(MaterialPageRoute(builder: (_) => PluginsScreen(core: core)));

/// The hub's plugins, and what the registry offers: install, update, set up, remove.
class PluginsScreen extends StatefulWidget {
  const PluginsScreen({super.key, required this.core});

  final CoreClient core;

  @override
  State<PluginsScreen> createState() => _PluginsScreenState();
}

class _PluginsScreenState extends State<PluginsScreen> {
  List<RegistryPlugin>? _offered;
  Object? _registryError;

  @override
  void initState() {
    super.initState();
    widget.core.refreshPlugins().catchError((Object _) {});
    _loadRegistry();
  }

  Future<void> _loadRegistry() async {
    setState(() => _registryError = null);
    try {
      final offered = await widget.core.registry();
      if (mounted) setState(() => _offered = offered);
    } catch (e) {
      if (mounted) setState(() => _registryError = e);
    }
  }

  Future<void> _install(RegistryPlugin plugin) async {
    final done = await showModalBottomSheet<bool>(
      context: context,
      isScrollControlled: true,
      useSafeArea: true,
      builder: (_) => _PluginSheet(
        name: plugin.name,
        permissions: plugin.permissions,
        // An update keeps its settings: nothing to ask.
        settings: plugin.installed == null ? plugin.settings : const [],
        confirm: plugin.installed == null ? null : plugin.version,
        save: (values) => widget.core.installPlugin(plugin, values),
      ),
    );
    if (done == true) _loadRegistry();
  }

  Future<void> _configure(InstalledPlugin plugin) => showModalBottomSheet<bool>(
    context: context,
    isScrollControlled: true,
    useSafeArea: true,
    builder: (_) => _PluginSheet(
      name: plugin.name,
      settings: plugin.declared,
      current: plugin.settings,
      secretsSet: plugin.secrets,
      save: (values) => widget.core.configurePlugin(plugin.id, values),
    ),
  );

  Future<void> _remove(InstalledPlugin plugin) async {
    final l = AppLocalizations.of(context);
    final sure = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(l.pluginRemove(plugin.name)),
        content: Text(l.pluginRemoveBody),
        actions: [
          TextButton(onPressed: () => Navigator.pop(context, false), child: Text(l.cancel)),
          FilledButton(onPressed: () => Navigator.pop(context, true), child: Text(l.removeItem)),
        ],
      ),
    );
    if (sure != true) return;
    try {
      await widget.core.removePlugin(plugin.id);
      _loadRegistry();
    } catch (e) {
      if (mounted) showError(context, e);
    }
  }

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final theme = Theme.of(context);
    final dim = theme.textTheme.bodyMedium?.copyWith(color: Hm.textDim);
    return Scaffold(
      appBar: AppBar(title: Text(l.plugins)),
      body: ListenableBuilder(
        listenable: widget.core,
        builder: (context, _) {
          final installed = widget.core.plugins;
          final offered = _offered?.where((p) => p.installed == null || p.update).toList();
          return RefreshIndicator(
            onRefresh: () => Future.wait([widget.core.refreshPlugins(), _loadRegistry()]),
            child: ListView(
              padding: const EdgeInsets.only(bottom: 32),
              children: [
                SectionHeader(l.pluginsInstalled),
                if (installed.isEmpty)
                  Padding(
                    padding: const EdgeInsets.symmetric(horizontal: 20, vertical: 8),
                    child: Text(l.pluginsNone, style: dim),
                  ),
                for (final p in installed) ...[
                  _InstalledTile(
                    plugin: p,
                    onConfigure: p.fromRegistry && p.declared.isNotEmpty ? () => _configure(p) : null,
                    onRemove: p.fromRegistry ? () => _remove(p) : null,
                  ),
                  const Divider(indent: 20),
                ],
                SectionHeader(l.pluginsAvailable),
                if (_registryError != null)
                  Padding(
                    padding: const EdgeInsets.symmetric(horizontal: 20, vertical: 8),
                    child: Row(
                      children: [
                        Expanded(child: Text(l.registryUnreachable(describeError(context, _registryError!)), style: dim)),
                        TextButton(onPressed: _loadRegistry, child: Text(l.retry)),
                      ],
                    ),
                  )
                else if (offered == null)
                  const Padding(
                    padding: EdgeInsets.all(24),
                    child: Center(child: CircularProgressIndicator()),
                  )
                else if (offered.isEmpty)
                  Padding(
                    padding: const EdgeInsets.symmetric(horizontal: 20, vertical: 8),
                    child: Text(l.pluginsAllInstalled, style: dim),
                  )
                else
                  for (final p in offered) ...[
                    ListTile(
                      contentPadding: const EdgeInsets.only(left: 20, right: 12),
                      leading: const Icon(Icons.extension_outlined),
                      title: Text(p.name, style: theme.textTheme.titleMedium),
                      subtitle: Text(p.version == null ? l.pluginNeedsNewer : p.description, style: dim),
                      trailing: p.version == null
                          ? null
                          : FilledButton.tonal(
                              onPressed: () => _install(p),
                              child: Text(p.update ? l.pluginUpdate(p.version!) : l.pluginInstall),
                            ),
                    ),
                    const Divider(indent: 20),
                  ],
              ],
            ),
          );
        },
      ),
    );
  }
}

class _InstalledTile extends StatelessWidget {
  const _InstalledTile({required this.plugin, this.onConfigure, this.onRemove});

  final InstalledPlugin plugin;
  final VoidCallback? onConfigure;
  final VoidCallback? onRemove;

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final theme = Theme.of(context);
    final p = plugin;
    final state = p.error ?? (p.version == null ? l.pluginStarting : l.pluginRunning);
    return ListTile(
      contentPadding: const EdgeInsets.only(left: 20, right: 8),
      leading: Icon(Icons.extension_outlined, color: p.error != null ? Hm.danger : null),
      title: Text(p.name, style: theme.textTheme.titleMedium),
      subtitle: Text(
        [if (p.version != null) p.version!, state, if (!p.fromRegistry) l.pluginLocal].join(' · '),
        maxLines: 3,
        overflow: TextOverflow.ellipsis,
        style: theme.textTheme.bodyMedium?.copyWith(color: p.error != null ? Hm.danger : Hm.textDim),
      ),
      trailing: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (onConfigure != null) IconButton(icon: const Icon(Icons.tune), tooltip: l.pluginSettings, onPressed: onConfigure),
          if (onRemove != null) IconButton(icon: const Icon(Icons.delete_outline), tooltip: l.removeItem, onPressed: onRemove),
        ],
      ),
    );
  }
}

/// Installing (permissions, then settings), updating (permissions only) or setting up a plugin.
/// Pops true once the hub accepted; its refusal stays in the sheet, with what was typed.
class _PluginSheet extends StatefulWidget {
  const _PluginSheet({
    required this.name,
    this.permissions = const [],
    required this.settings,
    this.current = const {},
    this.secretsSet = const [],
    this.confirm,
    required this.save,
  });

  final String name;

  /// Shown when installing or updating: what the user accepts.
  final List<String> permissions;
  final List<PluginSetting> settings;
  final Map<String, String> current;
  final List<String> secretsSet;

  /// The version an update goes to; null to install or set up.
  final String? confirm;
  final Future<void> Function(Map<String, String> values) save;

  @override
  State<_PluginSheet> createState() => _PluginSheetState();
}

class _PluginSheetState extends State<_PluginSheet> {
  late final _fields = {for (final s in widget.settings) s.key: TextEditingController(text: s.secret ? '' : widget.current[s.key] ?? '')};
  String? _error;
  bool _busy = false;

  @override
  void dispose() {
    for (final c in _fields.values) {
      c.dispose();
    }
    super.dispose();
  }

  Future<void> _save() async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await widget.save({for (final e in _fields.entries) e.key: e.value.text.trim()});
      if (mounted) Navigator.pop(context, true);
    } catch (e) {
      if (mounted) setState(() => _error = describeError(context, e));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  String _permission(AppLocalizations l, String p) => switch (p) {
    'local-network' => l.permLocalNetwork,
    'loopback' => l.permLoopback,
    _ => p,
  };

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final theme = Theme.of(context);
    final dim = theme.textTheme.bodyMedium?.copyWith(color: Hm.textDim);
    final installing = widget.permissions.isNotEmpty || widget.confirm != null;
    return Padding(
      padding: EdgeInsets.only(bottom: MediaQuery.viewInsetsOf(context).bottom),
      child: SingleChildScrollView(
        padding: const EdgeInsets.fromLTRB(20, 20, 20, 16),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text(widget.name, style: theme.textTheme.headlineSmall),
            if (installing) ...[
              const SizedBox(height: 16),
              Text(l.pluginAllowed, style: theme.textTheme.titleSmall),
              for (final p in widget.permissions)
                Padding(
                  padding: const EdgeInsets.only(top: 8),
                  child: Row(
                    children: [
                      const Icon(Icons.lan_outlined, size: 20, color: Hm.textDim),
                      const SizedBox(width: 12),
                      Expanded(child: Text(_permission(l, p), style: theme.textTheme.bodyLarge)),
                    ],
                  ),
                ),
              const SizedBox(height: 12),
              Text(l.pluginSigned, style: dim),
            ],
            for (final s in widget.settings)
              Padding(
                padding: const EdgeInsets.only(top: 16),
                child: TextField(
                  controller: _fields[s.key],
                  obscureText: s.secret,
                  enabled: !_busy,
                  decoration: InputDecoration(
                    labelText: s.required ? '${s.label} *' : s.label,
                    hintText: s.secret && widget.secretsSet.contains(s.key) ? l.pluginKeep : null,
                    floatingLabelBehavior: s.secret && widget.secretsSet.contains(s.key) ? FloatingLabelBehavior.always : null,
                  ),
                ),
              ),
            if (_error != null)
              Padding(
                padding: const EdgeInsets.only(top: 16),
                child: Text(_error!, style: theme.textTheme.bodyMedium?.copyWith(color: Hm.danger)),
              ),
            const SizedBox(height: 20),
            Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                TextButton(onPressed: _busy ? null : () => Navigator.pop(context, false), child: Text(l.cancel)),
                const SizedBox(width: 8),
                FilledButton(
                  onPressed: _busy ? null : _save,
                  child: _busy
                      ? const SizedBox.square(dimension: 18, child: CircularProgressIndicator(strokeWidth: 2))
                      : Text(
                          widget.confirm != null
                              ? l.pluginUpdate(widget.confirm!)
                              : installing
                              ? l.pluginInstall
                              : l.save,
                        ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
