import 'package:flutter/material.dart';

import '../clients.dart';
import '../connections.dart';
import '../l10n/app_localizations.dart';
import '../theme.dart';
import 'common.dart';
import 'plugins.dart';

class SettingsScreen extends StatelessWidget {
  const SettingsScreen({super.key, required this.connections, required this.core, required this.onPair});

  final Connections connections;

  /// The home, when paired: where it is (for sunrise and sunset).
  final CoreClient? core;
  final VoidCallback onPair;

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final theme = Theme.of(context);
    final core = this.core;
    return ListenableBuilder(
      listenable: Listenable.merge([connections, ?core]),
      builder: (context, _) => ListView(padding: const EdgeInsets.only(bottom: 24), children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(20, 20, 20, 0),
          child: Text(l.navSettings, style: theme.textTheme.displaySmall),
        ),
        SectionHeader(l.pairedSection),
        if (connections.all.isEmpty)
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 20, vertical: 8),
            child: Text(l.nonePaired, style: theme.textTheme.bodyMedium?.copyWith(color: Hm.textDim)),
          ),
        for (final c in connections.all) ...[
          ListTile(
            contentPadding: const EdgeInsets.only(left: 20, right: 8),
            leading: Icon(c.kind == 'core' ? Icons.home_outlined : Icons.computer_outlined),
            title: Text(c.kind == 'core' ? l.homeLabel(c.id) : l.pcLabel(c.id), style: theme.textTheme.titleMedium),
            subtitle: Text(
              '${c.host}:${c.port}',
              style: theme.textTheme.bodyMedium?.copyWith(color: Hm.textDim, fontFeatures: Hm.tabular),
            ),
            trailing: IconButton(
              icon: const Icon(Icons.delete_outline),
              tooltip: l.forget,
              onPressed: () => connections.remove(c),
            ),
          ),
          const Divider(indent: 20),
        ],
        Padding(
          padding: const EdgeInsets.fromLTRB(20, 12, 20, 0),
          child: Align(
            alignment: Alignment.centerLeft,
            child: OutlinedButton.icon(onPressed: onPair, icon: const Icon(Icons.qr_code_scanner), label: Text(l.pair)),
          ),
        ),
        if (core != null && core.connected) ...[
          SectionHeader(l.homeLocation),
          ListTile(
            contentPadding: const EdgeInsets.only(left: 20, right: 8),
            leading: const Icon(Icons.place_outlined),
            title: Text(
              core.location == null
                  ? l.locationUnset
                  : '${core.location!.latitude.toStringAsFixed(2)}, ${core.location!.longitude.toStringAsFixed(2)}',
              style: theme.textTheme.bodyLarge?.copyWith(fontFeatures: Hm.tabular),
            ),
            trailing: const Icon(Icons.edit_outlined),
            onTap: () => editLocation(context, core),
          ),
          SectionHeader(l.plugins),
          ListTile(
            contentPadding: const EdgeInsets.only(left: 20, right: 8),
            leading: const Icon(Icons.extension_outlined),
            title: Text(l.plugins, style: theme.textTheme.bodyLarge),
            subtitle: Text(l.pluginsHint, style: theme.textTheme.bodyMedium?.copyWith(color: Hm.textDim)),
            trailing: const Icon(Icons.chevron_right),
            onTap: () => showPlugins(context, core),
          ),
        ],
        SectionHeader(l.remoteSection),
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 20),
          child: Row(children: [
            Expanded(child: Text(l.pointerSpeed, style: theme.textTheme.bodyLarge)),
            Text(
              '× ${connections.speed.toStringAsFixed(2)}',
              style: theme.textTheme.bodyLarge?.copyWith(color: Hm.textDim, fontFeatures: Hm.tabular),
            ),
          ]),
        ),
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 8),
          child: Slider(
            value: connections.speed,
            min: 0.5,
            max: 4,
            divisions: 14,
            onChanged: (v) => connections.setRemote(speed: v),
          ),
        ),
        SwitchListTile(
          contentPadding: const EdgeInsets.symmetric(horizontal: 20),
          title: Text(l.invertScroll, style: theme.textTheme.bodyLarge),
          value: connections.scrollSign < 0,
          onChanged: (v) => connections.setRemote(scrollSign: v ? -1 : 1),
        ),
        const SizedBox(height: 40),
        Column(children: [
          const Logo(size: 44),
          const SizedBox(height: 10),
          Text('HomeMaid', style: theme.textTheme.titleMedium),
        ]),
      ]),
    );
  }
}

/// Where the home is, typed in: the core computes sunrise and sunset from it. True once saved.
Future<bool> editLocation(BuildContext context, CoreClient core) async =>
    await showDialog<bool>(context: context, builder: (_) => _LocationDialog(core: core)) ?? false;

class _LocationDialog extends StatefulWidget {
  const _LocationDialog({required this.core});

  final CoreClient core;

  @override
  State<_LocationDialog> createState() => _LocationDialogState();
}

class _LocationDialogState extends State<_LocationDialog> {
  late final _latitude = TextEditingController(text: widget.core.location?.latitude.toString() ?? '');
  late final _longitude = TextEditingController(text: widget.core.location?.longitude.toString() ?? '');
  String? _error;
  bool _busy = false;

  @override
  void dispose() {
    _latitude.dispose();
    _longitude.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    final l = AppLocalizations.of(context);
    double? parse(TextEditingController c) => double.tryParse(c.text.trim().replaceAll(',', '.'));
    final (latitude, longitude) = (parse(_latitude), parse(_longitude));
    if (latitude == null || longitude == null || latitude.abs() > 90 || longitude.abs() > 180) {
      setState(() => _error = l.locationInvalid);
      return;
    }
    setState(() => _busy = true);
    try {
      await widget.core.setLocation(latitude, longitude);
      if (mounted) Navigator.pop(context, true);
    } catch (e) {
      if (mounted) setState(() => _error = describeError(context, e));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final theme = Theme.of(context);
    const number = TextInputType.numberWithOptions(decimal: true, signed: true);
    return AlertDialog(
      title: Text(l.homeLocation),
      content: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        Text(l.locationHint, style: theme.textTheme.bodyMedium?.copyWith(color: Hm.textDim)),
        const SizedBox(height: 16),
        TextField(controller: _latitude, keyboardType: number, decoration: InputDecoration(labelText: l.latitude)),
        const SizedBox(height: 12),
        TextField(controller: _longitude, keyboardType: number, decoration: InputDecoration(labelText: l.longitude)),
        if (_error != null)
          Padding(
            padding: const EdgeInsets.only(top: 12),
            child: Text(_error!, style: theme.textTheme.bodyMedium?.copyWith(color: Hm.danger)),
          ),
      ]),
      actions: [
        TextButton(onPressed: () => Navigator.pop(context, false), child: Text(l.cancel)),
        FilledButton(onPressed: _busy ? null : _save, child: Text(l.save)),
      ],
    );
  }
}
