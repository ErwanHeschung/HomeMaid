import 'package:flutter/material.dart';

import '../clients.dart';
import '../l10n/app_localizations.dart';
import '../model.dart';
import '../theme.dart';
import 'common.dart';

Future<void> showPlugins(BuildContext context, CoreClient core) =>
    Navigator.of(context).push<void>(MaterialPageRoute(builder: (_) => PluginsScreen(core: core)));

/// The form of a device to add to [plugin] (its address, its login); true once the plugin took it.
/// A secret left empty is the previous device's, so a second strip needs no login.
Future<bool> showDeviceForm(BuildContext context, CoreClient core, InstalledPlugin plugin) async {
  final l = AppLocalizations.of(context);
  final added = await showModalBottomSheet<bool>(
    context: context,
    isScrollControlled: true,
    useSafeArea: true,
    builder: (_) => PluginSheet(
      name: plugin.name,
      settings: plugin.deviceSettings,
      secretsSet: plugin.devices.lastOrNull?.secrets ?? const [],
      keepHint: l.deviceSameLogin,
      action: l.addDevice,
      save: (values) => core.addDevice(plugin.id, values),
    ),
  );
  if (added == true && context.mounted) {
    ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(l.deviceAdded)));
  }
  return added == true;
}

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
      builder: (_) => PluginSheet(
        name: plugin.name,
        permissions: plugin.permissions,
        // An update keeps its settings: nothing to ask.
        settings: plugin.installed == null ? plugin.settings : const [],
        confirm: plugin.installed == null ? null : plugin.version,
        save: (values) => widget.core.installPlugin(plugin, values),
      ),
    );
    if (done != true) return;
    _loadRegistry();
    // Installed: its first device right away (a plugin with devices does nothing without one).
    final installed = widget.core.plugins.where((p) => p.id == plugin.id).firstOrNull;
    if (plugin.installed == null && installed != null && installed.deviceSettings.isNotEmpty && mounted) {
      await showDeviceForm(context, widget.core, installed);
    }
  }

  Future<void> _configure(InstalledPlugin plugin) => showModalBottomSheet<bool>(
    context: context,
    isScrollControlled: true,
    useSafeArea: true,
    builder: (_) => PluginSheet(
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
                    onAdd: p.deviceSettings.isNotEmpty ? () => showDeviceForm(context, widget.core, p) : null,
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
  const _InstalledTile({required this.plugin, this.onAdd, this.onConfigure, this.onRemove});

  final InstalledPlugin plugin;
  final VoidCallback? onAdd;
  final VoidCallback? onConfigure;
  final VoidCallback? onRemove;

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final theme = Theme.of(context);
    final p = plugin;
    final failing = p.devices.map((d) => d.error).nonNulls.firstOrNull;
    final error = p.error ?? failing;
    final state = error ?? (p.version == null ? l.pluginStarting : l.pluginRunning);
    return ListTile(
      contentPadding: const EdgeInsets.only(left: 20, right: 8),
      leading: Icon(Icons.extension_outlined, color: error != null ? Hm.danger : null),
      title: Text(p.name, style: theme.textTheme.titleMedium),
      subtitle: Text(
        [
          if (p.version != null) p.version!,
          if (p.deviceSettings.isNotEmpty) l.deviceCount(p.devices.length),
          state,
          if (!p.fromRegistry) l.pluginLocal,
        ].join(' · '),
        maxLines: 3,
        overflow: TextOverflow.ellipsis,
        style: theme.textTheme.bodyMedium?.copyWith(color: error != null ? Hm.danger : Hm.textDim),
      ),
      trailing: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (onAdd != null) IconButton(icon: const Icon(Icons.add), tooltip: l.addDevice, onPressed: onAdd),
          if (onConfigure != null) IconButton(icon: const Icon(Icons.tune), tooltip: l.pluginSettings, onPressed: onConfigure),
          if (onRemove != null) IconButton(icon: const Icon(Icons.delete_outline), tooltip: l.removeItem, onPressed: onRemove),
        ],
      ),
    );
  }
}

/// Installing (permissions, then settings), updating (permissions only) or setting up a plugin,
/// or adding one of its devices. Pops true once the hub accepted; its refusal stays in the
/// sheet, with what was typed.
class PluginSheet extends StatefulWidget {
  const PluginSheet({
    super.key,
    required this.name,
    this.permissions = const [],
    required this.settings,
    this.current = const {},
    this.secretsSet = const [],
    this.keepHint,
    this.confirm,
    this.action,
    required this.save,
  });

  final String name;

  /// Shown when installing or updating: what the user accepts.
  final List<String> permissions;
  final List<PluginSetting> settings;
  final Map<String, String> current;
  final List<String> secretsSet;

  /// What an empty secret of [secretsSet] does; "leave empty to keep" by default.
  final String? keepHint;

  /// The version an update goes to; null to install or set up.
  final String? confirm;

  /// The button's label, when it is none of install, update and save.
  final String? action;
  final Future<void> Function(Map<String, String> values) save;

  @override
  State<PluginSheet> createState() => _PluginSheetState();
}

class _PluginSheetState extends State<PluginSheet> {
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
                    hintText: s.secret && widget.secretsSet.contains(s.key) ? widget.keepHint ?? l.pluginKeep : null,
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
                          widget.action ??
                          (widget.confirm != null
                              ? l.pluginUpdate(widget.confirm!)
                              : installing
                              ? l.pluginInstall
                              : l.save),
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
