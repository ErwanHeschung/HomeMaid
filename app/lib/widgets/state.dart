/// How an entity's state reads everywhere: the same fill grammar in the viewport, the rows and
/// the sheet (solid = confirmed, blue hatch = pending, outline = offline, amber = physically on).
library;

import 'package:flutter/material.dart';

import '../l10n/app_localizations.dart';
import '../model.dart';
import '../theme.dart';

/// 0..1: how much of the entity is physically on (brightness scales a lamp). Amber is the device's
/// reported state only: a pending wish never lights anything.
double onLevel(Entity e) {
  if (!e.available) return 0;
  final power = inner(e.caps['on_off']?.observed);
  final brightness = inner(e.caps['brightness']?.observed);
  if (power != null) {
    if (power != true) return 0;
    return brightness is num ? (0.35 + 0.65 * brightness / 100).clamp(0.0, 1.0) : 1;
  }
  if (brightness is num) return brightness > 0 ? (0.35 + 0.65 * brightness / 100).clamp(0.0, 1.0) : 0;
  // No power switch: a reachable host with a volume is running (a PC behind its agent); a sensor
  // is never "on".
  return e.caps.containsKey('volume') ? 1 : 0;
}

bool isPending(Entity e) => e.caps.values.any((s) => s.pending);

/// The color a summary is set in: amber when physically on, blue while a wish is pending.
Color stateColor(Entity e) {
  if (!e.available) return Hm.textDim;
  if (isFailed(e) || alarming(e)) return Hm.danger;
  if (isPending(e)) return Hm.select;
  return onLevel(e) > 0 ? Hm.on : Hm.textDim;
}

/// A sensor reporting danger (smoke): shown in red like a failure.
bool alarming(Entity e) => e.caps['custom:smoke']?.shown == true;

bool isFailed(Entity e) => e.caps.values.any((s) => s.failed);

String? failureReason(Entity e) => e.caps.values.where((s) => s.failed).map((s) => s.reason).nonNulls.firstOrNull;

String capLabel(BuildContext context, String kind) {
  final l = AppLocalizations.of(context);
  final known = switch (kind) {
    'on_off' => l.capOnOff,
    'brightness' => l.capBrightness,
    'volume' => l.capVolume,
    'mute' => l.capMute,
    'power' => l.capPower,
    'custom:cpu' => l.capCpu,
    'custom:memory' => l.capMemory,
    'custom:active_window' => l.capActiveWindow,
    _ => null,
  };
  if (known != null) return known;
  // A plugin's own capability: its name, humanized.
  final words = kind.replaceFirst('custom:', '').replaceAll('_', ' ');
  return words.isEmpty ? kind : words[0].toUpperCase() + words.substring(1);
}

String percent(num v) => '${v.round()} %';

/// One line for rows and callouts: the state that matters most, two facts at most.
String summary(BuildContext context, Entity e) {
  final l = AppLocalizations.of(context);
  if (!e.available) return l.offline;
  if (isFailed(e)) return l.failedNoReason;
  final parts = <String>[];
  final power = e.caps['on_off']?.shown;
  if (power != null) parts.add(power == true ? l.stateOn : l.stateOff);
  final brightness = e.caps['brightness']?.shown;
  if (brightness is num && power != false) parts.add(percent(brightness));
  final volume = e.caps['volume']?.shown;
  if (volume is num) parts.add(e.caps['mute']?.shown == true ? l.muted : '${l.capVolume} ${percent(volume)}');
  // A PC or a TV has no power switch: reachable with a volume means running, as the render shows.
  if (power == null && onLevel(e) > 0) parts.insert(0, l.stateOn);
  final watts = e.caps['power']?.shown;
  if (watts is Map && watts['watts'] is num) parts.add(formatWatts(watts['watts'] as num));
  if (parts.isEmpty) {
    // A sensor: its yes/no facts first (locked, open...), then its readings.
    final caps = e.orderedCaps.where((c) => c.key.startsWith('custom:'));
    for (final MapEntry(key: kind, value: slot) in [
      ...caps.where((c) => c.value.shown is bool),
      ...caps.where((c) => c.value.shown is num),
      ...caps.where((c) => c.value.shown is String),
    ]) {
      final value = slot.shown;
      parts.add(switch (value) {
        bool b => _yesNo(l, kind, b) ?? '${capLabel(context, kind)} · ${b ? l.yes : l.no}',
        num n => '${capLabel(context, kind)} ${n.round()} ${slot.unit ?? ''}'.trim(),
        _ => '$value',
      });
    }
  }
  return parts.take(2).join(' · ');
}

/// The words of the common yes/no sensors; others read "name · Yes".
String? _yesNo(AppLocalizations l, String kind, bool on) => switch (kind) {
      'custom:locked' => on ? l.stateLocked : l.stateUnlocked,
      'custom:open' || 'custom:contact' => on ? l.stateOpen : l.stateClosed,
      'custom:motion' => on ? l.stateMotion : l.stateNoMotion,
      'custom:smoke' => on ? l.stateSmoke : l.stateNoSmoke,
      _ => null,
    };

/// One decimal only where it means something (under 10 W).
String formatWatts(num watts) => watts < 10 ? '${watts.toStringAsFixed(1)} W' : '${watts.round()} W';

/// A small square filled the way the model is: the legend of the fill grammar.
class StateSwatch extends StatelessWidget {
  const StateSwatch(this.entity, {super.key, this.size = 14});

  final Entity entity;
  final double size;

  @override
  Widget build(BuildContext context) => CustomPaint(
        size: Size.square(size),
        painter: _SwatchPainter(
          offline: !entity.available,
          failed: isFailed(entity),
          pending: isPending(entity),
          on: onLevel(entity),
        ),
      );
}

class _SwatchPainter extends CustomPainter {
  _SwatchPainter({required this.offline, required this.failed, required this.pending, required this.on});

  final bool offline, failed, pending;
  final double on;

  @override
  void paint(Canvas canvas, Size size) {
    final rect = (Offset.zero & size).deflate(0.75);
    final stroke = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1.5;
    if (offline) {
      canvas.drawRect(rect, stroke..color = Hm.textDim);
      return;
    }
    canvas.drawRect(rect, Paint()..color = Color.lerp(const Color(0xFF8F96A0), Hm.on, on)!);
    if (pending) {
      canvas.save();
      canvas.clipRect(rect);
      final hatch = Paint()
        ..color = Hm.select
        ..strokeWidth = 1.4;
      for (var x = -size.height; x < size.width; x += 4) {
        canvas.drawLine(Offset(x, size.height), Offset(x + size.height, 0), hatch);
      }
      canvas.restore();
    }
    if (failed) canvas.drawRect(rect, stroke..color = Hm.danger);
  }

  @override
  bool shouldRepaint(_SwatchPainter old) =>
      old.offline != offline || old.failed != failed || old.pending != pending || old.on != on;
}

/// A value that changes in place: the new one slides up into the same box, nothing else moves.
class FlipText extends StatelessWidget {
  const FlipText(this.text, {super.key, this.style, this.alignment = Alignment.centerLeft});

  final String text;
  final TextStyle? style;
  final Alignment alignment;

  @override
  Widget build(BuildContext context) => AnimatedSwitcher(
        duration: MediaQuery.of(context).disableAnimations ? Duration.zero : const Duration(milliseconds: 220),
        switchInCurve: Curves.easeOutCubic,
        switchOutCurve: Curves.easeInCubic,
        layoutBuilder: (current, previous) => Stack(alignment: alignment, children: [...previous, ?current]),
        transitionBuilder: (child, animation) => ClipRect(
          child: SlideTransition(
            position: Tween(begin: const Offset(0, 0.7), end: Offset.zero).animate(animation),
            child: FadeTransition(opacity: animation, child: child),
          ),
        ),
        child: Text(text, key: ValueKey(text), maxLines: 1, overflow: TextOverflow.ellipsis, style: style),
      );
}

/// The power switch of a device. Amber only once the device reports it on: while a command is
/// pending the track stays selection blue; offline, the switch dims (Material's 38 %).
class PowerSwitch extends StatelessWidget {
  const PowerSwitch({super.key, required this.entity, required this.onChanged});

  final Entity entity;
  final ValueChanged<bool> onChanged;

  @override
  Widget build(BuildContext context) {
    final slot = entity.caps['on_off'];
    final toggle = Switch(value: slot?.shown == true, onChanged: entity.available ? onChanged : null);
    if (!entity.available) return Opacity(opacity: 0.38, child: toggle);
    return slot?.pending ?? false ? Hm.settingSwitch(context, toggle) : toggle;
  }
}
