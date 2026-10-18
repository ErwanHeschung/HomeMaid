/// One control per capability. The UI is built from capabilities only: a new plugin shows up
/// here without app changes, its `custom:` capabilities rendered from their schema.
library;

import 'dart:convert';

import 'package:flutter/material.dart';

import '../l10n/app_localizations.dart';
import '../model.dart';
import '../theme.dart';
import 'state.dart';

class CapabilityTile extends StatelessWidget {
  const CapabilityTile({super.key, required this.kind, required this.slot, required this.enabled, required this.onSet});

  final String kind;
  final Slot slot;

  /// False when the entity is offline.
  final bool enabled;

  /// Sends a new desired value (the inner value, e.g. `true` or `30`).
  final ValueChanged<Object?> onSet;

  @override
  Widget build(BuildContext context) {
    final writable = enabled && !slot.readOnly;
    final value = slot.shown;
    final label = Text(capLabel(context, kind), style: Theme.of(context).textTheme.bodyLarge);
    if (kind == 'brightness' || kind == 'volume') {
      return _LevelSlider(label: label, status: _Status(slot), slot: slot, writable: writable, onSet: onSet);
    }
    final Widget control = switch (kind) {
      // Amber only once the device confirms: a pending wish keeps the track blue.
      'on_off' => slot.pending
          ? Hm.settingSwitch(context, Switch(value: value == true, onChanged: writable ? onSet : null))
          : Switch(value: value == true, onChanged: writable ? onSet : null),
      'mute' => Hm.settingSwitch(context, Switch(value: value == true, onChanged: writable ? onSet : null)),
      'power' => _Reading(_watts(value)),
      _ => _generic(value, writable),
    };
    return ConstrainedBox(
      constraints: const BoxConstraints(minHeight: 56),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 20),
        child: Row(children: [
          Expanded(child: label),
          _Status(slot),
          const SizedBox(width: 12),
          Flexible(child: Align(alignment: Alignment.centerRight, child: control)),
        ]),
      ),
    );
  }

  Widget _generic(Object? value, bool writable) {
    final type = slot.schema?['type'];
    bool isType(String t) => type == t || (type is List && type.contains(t));
    if (isType('boolean') && value is bool?) {
      return Builder(
        builder: (context) => Hm.settingSwitch(context, Switch(value: value ?? false, onChanged: writable ? onSet : null)),
      );
    }
    if (isType('number') && value is num) {
      final unit = slot.unit;
      return _Reading('${value.round()}${unit == null ? '' : ' $unit'}');
    }
    if (isType('string')) return _Reading(value as String? ?? '—');
    return _Reading(value == null ? '—' : jsonEncode(value));
  }

  static String _watts(Object? value) {
    final watts = value is Map ? value['watts'] : null;
    return watts is num ? formatWatts(watts) : '—';
  }
}

class _Reading extends StatelessWidget {
  const _Reading(this.text);

  final String text;

  @override
  Widget build(BuildContext context) => FlipText(
        text,
        alignment: Alignment.centerRight,
        style: Theme.of(context).textTheme.bodyLarge?.copyWith(color: Hm.textDim, fontFeatures: Hm.tabular),
      );
}

/// Spinner while a command waits for the device, error mark (with its reason) when it failed.
class _Status extends StatelessWidget {
  const _Status(this.slot);

  final Slot slot;

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    if (slot.pending) {
      return Semantics(
        label: l.pending,
        child: const SizedBox.square(dimension: 16, child: CircularProgressIndicator(strokeWidth: 2)),
      );
    }
    if (slot.failed) {
      final message = slot.reason == null ? l.failedNoReason : l.failed(slot.reason!);
      return IconButton(
        icon: const Icon(Icons.error_outline, color: Hm.danger),
        tooltip: message,
        onPressed: () => ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(message))),
      );
    }
    return const SizedBox.shrink();
  }
}

/// Follows the finger locally, sends once on release.
class _LevelSlider extends StatefulWidget {
  const _LevelSlider({required this.label, required this.status, required this.slot, required this.writable, required this.onSet});

  final Widget label;
  final Widget status;
  final Slot slot;
  final bool writable;
  final ValueChanged<Object?> onSet;

  @override
  State<_LevelSlider> createState() => _LevelSliderState();
}

class _LevelSliderState extends State<_LevelSlider> {
  double? _dragging;

  @override
  Widget build(BuildContext context) {
    final min = (widget.slot.min ?? 0).toDouble();
    final max = (widget.slot.max ?? 100).toDouble();
    final current = ((widget.slot.shown as num?) ?? min).toDouble().clamp(min, max);
    final value = _dragging ?? current;
    final unit = widget.slot.unit;
    return Padding(
      padding: const EdgeInsets.fromLTRB(20, 8, 20, 0),
      child: Column(children: [
        Row(children: [
          Expanded(child: widget.label),
          widget.status,
          const SizedBox(width: 12),
          Text(
            '${value.round()}${unit == null ? '' : ' $unit'}',
            style: Theme.of(context).textTheme.bodyLarge?.copyWith(color: Hm.textDim, fontFeatures: Hm.tabular),
          ),
        ]),
        Slider(
          value: value,
          min: min,
          max: max,
          padding: const EdgeInsets.symmetric(vertical: 14),
          onChanged: widget.writable ? (v) => setState(() => _dragging = v) : null,
          onChangeEnd: widget.writable
              ? (v) {
                  setState(() => _dragging = null);
                  widget.onSet(v.round());
                }
              : null,
        ),
      ]),
    );
  }
}
