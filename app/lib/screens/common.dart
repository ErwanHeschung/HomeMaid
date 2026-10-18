import 'package:flutter/material.dart';

import '../clients.dart';
import '../l10n/app_localizations.dart';
import '../theme.dart';
import '../viewport/room_viewport.dart';

String describeError(BuildContext context, Object error) {
  final l = AppLocalizations.of(context);
  return switch (error) {
    LinkException(error: LinkError.unreachable) => l.errorUnreachable,
    LinkException(error: LinkError.lost) => l.errorLost,
    LinkException(error: LinkError.notConnected) => l.errorNotConnected,
    LinkException(error: LinkError.timeout) => l.errorTimeout,
    _ => '$error',
  };
}

void showError(BuildContext context, Object error) =>
    ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(describeError(context, error))));

/// Nothing paired yet: an empty construction grid waiting for its first model.
class EmptyState extends StatelessWidget {
  const EmptyState({super.key, required this.title, required this.hint, required this.onPair});

  final String title;
  final String hint;
  final VoidCallback onPair;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return LayoutBuilder(
      builder: (context, constraints) => SingleChildScrollView(
        child: ConstrainedBox(
          constraints: BoxConstraints(minHeight: constraints.maxHeight),
          child: Padding(
            padding: const EdgeInsets.fromLTRB(24, 24, 24, 32),
            child: Column(mainAxisAlignment: MainAxisAlignment.center, children: [
              SizedBox(
                height: 220,
                child: IgnorePointer(child: RoomViewport(kind: 'generic', entities: const [], selected: null, onSelect: (_) {})),
              ),
              const SizedBox(height: 24),
              Text(title, style: theme.textTheme.headlineSmall, textAlign: TextAlign.center),
              const SizedBox(height: 8),
              Text(
                hint,
                textAlign: TextAlign.center,
                style: theme.textTheme.bodyMedium?.copyWith(color: Hm.textDim),
              ),
              const SizedBox(height: 28),
              FilledButton.icon(
                onPressed: onPair,
                icon: const Icon(Icons.qr_code_scanner),
                label: Text(AppLocalizations.of(context).scanCode),
              ),
            ]),
          ),
        ),
      ),
    );
  }
}

class OfflineBanner extends StatelessWidget {
  const OfflineBanner(this.title, {super.key, this.detail});

  final String title;
  final String? detail;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Container(
      margin: const EdgeInsets.fromLTRB(16, 4, 16, 8),
      padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
      decoration: BoxDecoration(
        color: Hm.danger.withValues(alpha: 0.08),
        borderRadius: BorderRadius.circular(Hm.radius),
        border: Border.all(color: Hm.danger.withValues(alpha: 0.6)),
      ),
      child: Row(children: [
        const Icon(Icons.link_off, color: Hm.danger, size: 20),
        const SizedBox(width: 12),
        Expanded(
          child: Text.rich(TextSpan(children: [
            TextSpan(text: title, style: theme.textTheme.titleSmall),
            if (detail != null) TextSpan(text: ' · $detail', style: theme.textTheme.bodyMedium?.copyWith(color: Hm.textDim)),
          ])),
        ),
      ]),
    );
  }
}

class SectionHeader extends StatelessWidget {
  const SectionHeader(this.title, {super.key});

  final String title;

  @override
  Widget build(BuildContext context) => Padding(
        padding: const EdgeInsets.fromLTRB(20, 24, 20, 8),
        child: Text(title, style: Theme.of(context).textTheme.titleSmall?.copyWith(color: Hm.textDim)),
      );
}
