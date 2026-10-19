/// What the remote's strip launches, managed from the phone: sites (from a catalogue, or any web
/// address) and the PC's apps (its Start menu, when the PC offers them). The PC writes them to
/// its homemaid.toml.
library;

import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_svg/flutter_svg.dart';

import '../clients.dart';
import '../l10n/app_localizations.dart';
import '../theme.dart';
import 'remote.dart' show logoFor;

/// The sites of `assets/shortcuts.json`: data, so the code knows no service.
Future<List<Shortcut>> loadCatalog() async {
  final list = jsonDecode(await rootBundle.loadString('assets/shortcuts.json')) as List;
  return [for (final s in list) Shortcut.fromJson((s as Map).cast())];
}

/// A web address the PC can open: http(s) with a host.
bool isWebAddress(String text) {
  final uri = Uri.tryParse(text.trim());
  return uri != null && (uri.scheme == 'https' || uri.scheme == 'http') && uri.host.contains('.');
}

Future<void> showLaunchers(BuildContext context, AgentClient agent) => showModalBottomSheet<void>(
      context: context,
      isScrollControlled: true,
      useSafeArea: true,
      builder: (_) => FractionallySizedBox(heightFactor: 0.85, child: LauncherSheet(agent: agent)),
    );

/// Removes a site or an app from the strip, once confirmed.
Future<void> removeLauncher(BuildContext context, AgentClient agent, String name, {required bool site}) async {
  final l = AppLocalizations.of(context);
  final sure = await showDialog<bool>(
    context: context,
    builder: (context) => AlertDialog(
      title: Text(l.removeLauncher(name)),
      content: Text(l.removeLauncherBody),
      actions: [
        TextButton(onPressed: () => Navigator.pop(context, false), child: Text(l.cancel)),
        FilledButton(onPressed: () => Navigator.pop(context, true), child: Text(l.removeItem)),
      ],
    ),
  );
  if (sure != true) return;
  site ? agent.deleteShortcut(name) : agent.unpinApp(name);
}

class LauncherSheet extends StatefulWidget {
  const LauncherSheet({super.key, required this.agent});

  final AgentClient agent;

  @override
  State<LauncherSheet> createState() => _LauncherSheetState();
}

class _LauncherSheetState extends State<LauncherSheet> {
  var _tab = 'sites';
  List<Shortcut>? _catalog;
  final _name = TextEditingController();
  final _address = TextEditingController();
  final _search = TextEditingController();
  String? _invalid;

  @override
  void initState() {
    super.initState();
    loadCatalog().then((c) => mounted ? setState(() => _catalog = c) : null);
    if (widget.agent.discover) widget.agent.listApps();
  }

  @override
  void dispose() {
    _name.dispose();
    _address.dispose();
    _search.dispose();
    super.dispose();
  }

  void _addOther() {
    final l = AppLocalizations.of(context);
    final (name, address) = (_name.text.trim(), _address.text.trim());
    if (name.isEmpty || !isWebAddress(address)) {
      setState(() => _invalid = l.siteInvalid);
      return;
    }
    widget.agent.saveShortcut(Shortcut(name: name, icon: logoFor(null, name), open: address));
    setState(() {
      _invalid = null;
      _name.clear();
      _address.clear();
    });
  }

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final theme = Theme.of(context);
    return ListenableBuilder(
      listenable: widget.agent,
      builder: (context, _) => Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(20, 0, 20, 12),
          child: Text(l.addLauncher, style: theme.textTheme.headlineSmall),
        ),
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 20),
          child: SegmentedButton<String>(
            showSelectedIcon: false,
            segments: [
              ButtonSegment(value: 'sites', label: Text(l.launcherSites)),
              ButtonSegment(value: 'apps', label: Text(l.launcherApps)),
            ],
            selected: {_tab},
            onSelectionChanged: (s) => setState(() => _tab = s.first),
          ),
        ),
        if (widget.agent.refused != null)
          Padding(
            padding: const EdgeInsets.fromLTRB(20, 12, 20, 0),
            child: Text(widget.agent.refused!, style: theme.textTheme.bodyMedium?.copyWith(color: Hm.danger)),
          ),
        const SizedBox(height: 8),
        Expanded(child: _tab == 'sites' ? _sites(context) : _apps(context)),
      ]),
    );
  }

  Widget _sites(BuildContext context) {
    final l = AppLocalizations.of(context);
    final theme = Theme.of(context);
    final added = widget.agent.shortcuts.map((s) => s.name.toLowerCase()).toSet();
    return ListView(padding: const EdgeInsets.only(bottom: 24), children: [
      for (final site in _catalog ?? const <Shortcut>[]) ...[
        ListTile(
          contentPadding: const EdgeInsets.symmetric(horizontal: 20),
          leading: _Logo(site.icon, site.name),
          title: Text(site.name, style: theme.textTheme.titleMedium),
          subtitle: Text(Uri.parse(site.open!).host, style: theme.textTheme.bodyMedium?.copyWith(color: Hm.textDim)),
          trailing: added.contains(site.name.toLowerCase())
              ? Text(l.siteAdded, style: theme.textTheme.labelMedium?.copyWith(color: Hm.textDim))
              : const Icon(Icons.add, color: Hm.select),
          onTap: added.contains(site.name.toLowerCase()) || !widget.agent.connected ? null : () => widget.agent.saveShortcut(site),
        ),
        const Divider(indent: 72),
      ],
      Padding(
        padding: const EdgeInsets.fromLTRB(20, 24, 20, 8),
        child: Text(l.otherSite, style: theme.textTheme.titleSmall?.copyWith(color: Hm.textDim)),
      ),
      Padding(
        padding: const EdgeInsets.symmetric(horizontal: 20),
        child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          TextField(controller: _name, decoration: InputDecoration(labelText: l.name), textCapitalization: TextCapitalization.words),
          const SizedBox(height: 12),
          TextField(
            controller: _address,
            decoration: InputDecoration(labelText: l.siteAddress, hintText: 'https://…'),
            keyboardType: TextInputType.url,
            autocorrect: false,
          ),
          if (_invalid != null)
            Padding(
              padding: const EdgeInsets.only(top: 8),
              child: Text(_invalid!, style: theme.textTheme.bodyMedium?.copyWith(color: Hm.danger)),
            ),
          const SizedBox(height: 12),
          FilledButton(onPressed: widget.agent.connected ? _addOther : null, child: Text(l.addSite)),
        ]),
      ),
    ]);
  }

  Widget _apps(BuildContext context) {
    final l = AppLocalizations.of(context);
    final theme = Theme.of(context);
    if (!widget.agent.discover) {
      return Padding(
        padding: const EdgeInsets.all(20),
        child: Text(l.appsOff, style: theme.textTheme.bodyMedium?.copyWith(color: Hm.textDim)),
      );
    }
    final all = widget.agent.discovered;
    if (all == null) return const Center(child: CircularProgressIndicator());
    final query = _search.text.trim().toLowerCase();
    final shown = all.where((a) => a.toLowerCase().contains(query)).toList();
    final pinned = widget.agent.apps.toSet();
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      Padding(
        padding: const EdgeInsets.fromLTRB(20, 4, 20, 8),
        child: TextField(
          controller: _search,
          decoration: InputDecoration(labelText: l.searchApps, prefixIcon: const Icon(Icons.search)),
          onChanged: (_) => setState(() {}),
        ),
      ),
      Expanded(
        child: shown.isEmpty
            ? Padding(
                padding: const EdgeInsets.all(20),
                child: Text(l.noAppsFound, style: theme.textTheme.bodyMedium?.copyWith(color: Hm.textDim)),
              )
            : ListView.separated(
                padding: const EdgeInsets.only(bottom: 24),
                itemCount: shown.length,
                separatorBuilder: (_, _) => const Divider(indent: 72),
                itemBuilder: (context, i) {
                  final app = shown[i];
                  final on = pinned.contains(app);
                  return ListTile(
                    contentPadding: const EdgeInsets.only(left: 20, right: 8),
                    leading: _AppIcon(widget.agent, app),
                    title: Text(app, style: theme.textTheme.bodyLarge),
                    onTap: widget.agent.connected ? () => widget.agent.send({'type': 'launch', 'app': app}) : null,
                    trailing: IconButton(
                      icon: Icon(on ? Icons.push_pin : Icons.push_pin_outlined, color: on ? Hm.select : Hm.textDim),
                      tooltip: on ? l.unpinApp(app) : l.pinApp(app),
                      onPressed: !widget.agent.connected ? null : () => on ? widget.agent.unpinApp(app) : widget.agent.pinApp(app),
                    ),
                  );
                },
              ),
      ),
    ]);
  }
}

/// An app's own icon, as the PC's Start menu shows it, in colour: in this list it helps find an
/// app among many (the strip keeps its ink logos). The placeholder while it comes, or without one.
class _AppIcon extends StatelessWidget {
  const _AppIcon(this.agent, this.app);

  final AgentClient agent;
  final String app;

  @override
  Widget build(BuildContext context) {
    final placeholder = _Logo(null, app);
    final icon = agent.iconOf(app);
    if (icon == null) return placeholder;
    return SizedBox.square(
      dimension: 32,
      child: Center(
        child: Image.network(
          icon.uri.toString(),
          headers: icon.headers,
          width: 28,
          height: 28,
          filterQuality: FilterQuality.medium,
          frameBuilder: (context, child, frame, _) => frame == null ? placeholder : child,
          errorBuilder: (_, _, _) => placeholder,
        ),
      ),
    );
  }
}

/// A service's logo in ink, or a dim placeholder of the same size.
class _Logo extends StatelessWidget {
  const _Logo(this.icon, this.name);

  final String? icon;
  final String name;

  @override
  Widget build(BuildContext context) {
    final logo = logoFor(icon, name);
    return SizedBox.square(
      dimension: 32,
      child: Center(
        child: logo == null
            ? const Icon(Icons.apps, color: Hm.textDim, size: 22)
            : SvgPicture.asset('assets/logos/$logo.svg', width: 22, height: 22, colorFilter: const ColorFilter.mode(Hm.text, BlendMode.srcIn)),
      ),
    );
  }
}
