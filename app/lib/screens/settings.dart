import 'package:flutter/material.dart';

import '../connections.dart';
import '../l10n/app_localizations.dart';
import '../theme.dart';
import 'common.dart';

class SettingsScreen extends StatelessWidget {
  const SettingsScreen({super.key, required this.connections, required this.onPair});

  final Connections connections;
  final VoidCallback onPair;

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final theme = Theme.of(context);
    return ListenableBuilder(
      listenable: connections,
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
