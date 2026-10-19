import 'dart:async';
import 'dart:math';

import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter/services.dart';
import 'package:flutter_svg/flutter_svg.dart';

import '../clients.dart';
import '../connections.dart';
import '../l10n/app_localizations.dart';
import '../theme.dart';
import '../trackpad.dart';
import 'common.dart';
import 'launchers.dart';

/// The PC remote: one column weighted towards the thumb. The PC and its power on top, its apps,
/// the trackpad taking what is left, then playback and volume. The keyboard swaps the bottom
/// rows for a strip of keys.
class RemoteScreen extends StatefulWidget {
  const RemoteScreen({super.key, required this.agents, required this.connections, required this.onPair});

  final List<AgentClient> agents;
  final Connections connections;
  final VoidCallback onPair;

  @override
  State<RemoteScreen> createState() => _RemoteScreenState();
}

class _RemoteScreenState extends State<RemoteScreen> {
  int _selected = 0;
  bool _typing = false;

  /// Play/pause has no state to read on the PC: the icon follows the last press.
  bool _playing = false;

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    if (widget.agents.isEmpty) return EmptyState(title: l.pairPcTitle, hint: l.pairPcHint, onPair: widget.onPair);
    final agent = widget.agents[min(_selected, widget.agents.length - 1)];
    return ListenableBuilder(
      listenable: agent,
      builder: (context, _) => Padding(
        padding: const EdgeInsets.fromLTRB(16, 12, 16, 16),
        child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          _header(context, agent),
          const SizedBox(height: 16),
          _Launchers(agent: agent),
          const SizedBox(height: 16),
          Expanded(child: SketchPlane(agent: agent, connections: widget.connections)),
          const SizedBox(height: 20),
          if (_typing)
            _Keys(agent: agent, onClosed: () => setState(() => _typing = false))
          else ...[
            _playback(context, agent),
            const SizedBox(height: 20),
            _volume(context, agent),
          ],
        ]),
      ),
    );
  }

  Widget _header(BuildContext context, AgentClient agent) {
    final l = AppLocalizations.of(context);
    final theme = Theme.of(context);
    final (color, status) = !agent.connected
        ? (agent.linkError == null ? Hm.textDim : Hm.danger,
            agent.linkError == null ? l.connecting : '${l.pcUnreachable} · ${describeError(context, LinkException(agent.linkError!))}')
        : (Hm.on, agent.refused ?? l.connected);
    return Row(children: [
      Expanded(
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          if (widget.agents.length == 1)
            Text(agent.hostname, style: theme.textTheme.headlineSmall, maxLines: 1, overflow: TextOverflow.ellipsis)
          else
            DropdownButton<int>(
              value: _selected,
              isExpanded: true,
              isDense: true,
              underline: const SizedBox.shrink(),
              style: theme.textTheme.headlineSmall,
              items: [for (final (i, a) in widget.agents.indexed) DropdownMenuItem(value: i, child: Text(a.hostname))],
              onChanged: (i) => setState(() => _selected = i ?? 0),
            ),
          const SizedBox(height: 4),
          Row(children: [
            Container(width: 7, height: 7, decoration: BoxDecoration(color: color, shape: BoxShape.circle)),
            const SizedBox(width: 8),
            Flexible(
              child: Text(
                status,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: theme.textTheme.bodyMedium?.copyWith(
                  color: agent.connected && agent.refused != null ? Hm.danger : Hm.textDim,
                ),
              ),
            ),
          ]),
        ]),
      ),
      const SizedBox(width: 8),
      _Round(
        icon: _typing ? Icons.keyboard_hide_outlined : Icons.keyboard_outlined,
        label: l.keyboard,
        size: 48,
        selected: _typing,
        onPressed: agent.connected ? () => setState(() => _typing = !_typing) : null,
      ),
      const SizedBox(width: 4),
      _Round(
        icon: Icons.power_settings_new,
        label: l.power,
        size: 48,
        color: Hm.danger,
        onPressed: agent.connected ? () => _power(context, agent) : null,
      ),
    ]);
  }

  Widget _playback(BuildContext context, AgentClient agent) {
    final l = AppLocalizations.of(context);
    final on = agent.connected;
    final service = agent.activeShortcut;
    void media(String action) => agent.send({'type': 'media', 'action': action});
    void key(String key) => agent.send({'type': 'key', 'key': key});
    // With a service open, each button sends that service's own keys; without one, media keys
    // and the arrows most players seek with.
    VoidCallback? act(String action, VoidCallback fallback) {
      if (!on) return null;
      if (service == null) return fallback;
      if (!service.actions.contains(action)) return null;
      return () => agent.send({'type': 'control', 'shortcut': service.name, 'action': action});
    }

    final play = act('play_pause', () => media('play_pause'));
    return Row(mainAxisAlignment: MainAxisAlignment.spaceEvenly, children: [
      service == null
          ? _Round(icon: Icons.skip_previous, label: l.previous, onPressed: act('restart', () => media('previous')))
          : _Round(icon: Icons.replay, label: l.restart, onPressed: act('restart', () {})),
      _Round(icon: Icons.replay_5, label: l.back5, onPressed: act('back', () => key('left'))),
      _Round(
        icon: _playing ? Icons.pause : Icons.play_arrow,
        label: l.playPause,
        size: 72,
        primary: true,
        onPressed: play == null
            ? null
            : () {
                play();
                setState(() => _playing = !_playing);
              },
      ),
      _Round(icon: Icons.forward_5, label: l.forward5, onPressed: act('forward', () => key('right'))),
      _Round(
        icon: Icons.skip_next,
        label: service == null ? l.next : l.nextEpisode,
        onPressed: act('next_episode', () => media('next')),
      ),
    ]);
  }

  Widget _volume(BuildContext context, AgentClient agent) {
    final l = AppLocalizations.of(context);
    final muted = agent.muted ?? false;
    final on = agent.connected;
    final service = agent.activeShortcut;
    final skip = service != null && service.actions.contains('skip_intro');
    return Row(children: [
      _Round(
        icon: muted ? Icons.volume_off_outlined : Icons.volume_up_outlined,
        label: muted ? l.unmute : l.mute,
        size: 48,
        // Mute engaged: a selected key, like any other.
        selected: muted,
        onPressed: on && agent.muted != null ? () => agent.setMuted(!muted) : null,
      ),
      Expanded(child: _VolumeStepper(agent: agent)),
      // Only where the open service has a key for it; the space stays so the stepper never moves.
      Visibility.maintain(
        visible: skip,
        child: _Round(
          icon: Icons.fast_forward_outlined,
          label: l.skipIntro,
          size: 48,
          onPressed: on && skip ? () => agent.send({'type': 'control', 'shortcut': service.name, 'action': 'skip_intro'}) : null,
        ),
      ),
    ]);
  }

  Future<void> _power(BuildContext context, AgentClient agent) async {
    final l = AppLocalizations.of(context);
    final theme = Theme.of(context);
    final action = await showModalBottomSheet<String>(
      context: context,
      useSafeArea: true,
      builder: (context) => SafeArea(
        top: false,
        child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(24, 0, 24, 8),
            child: Text(agent.hostname, style: theme.textTheme.titleLarge),
          ),
          for (final (id, icon, label) in [
            ('lock', Icons.lock_outline, l.powerLock),
            ('sleep', Icons.bedtime_outlined, l.powerSleep),
            ('shutdown', Icons.power_settings_new, l.powerShutdown),
          ])
            ListTile(
              minTileHeight: 56,
              contentPadding: const EdgeInsets.symmetric(horizontal: 24),
              leading: Icon(icon, color: id == 'shutdown' ? Hm.danger : Hm.text),
              title: Text(label, style: id == 'shutdown' ? const TextStyle(color: Hm.danger) : null),
              onTap: () => Navigator.pop(context, id),
            ),
          const SizedBox(height: 8),
        ]),
      ),
    );
    if (action == null || !context.mounted) return;
    if (action == 'shutdown') {
      final sure = await showDialog<bool>(
        context: context,
        builder: (context) => AlertDialog(
          icon: const Icon(Icons.power_settings_new, color: Hm.danger),
          title: Text(l.shutdownConfirm(agent.hostname)),
          content: Text(l.shutdownBody),
          actions: [
            TextButton(onPressed: () => Navigator.pop(context, false), child: Text(l.cancel)),
            FilledButton(
              style: FilledButton.styleFrom(backgroundColor: Hm.danger, foregroundColor: Hm.ground),
              onPressed: () => Navigator.pop(context, true),
              child: Text(l.powerShutdown),
            ),
          ],
        ),
      );
      if (sure != true) return;
    }
    HapticFeedback.mediumImpact();
    agent.send({'type': 'power', 'action': action});
  }
}

/// Logos bundled in assets/logos (Simple Icons slugs), and other names people give those apps.
const _logos = {
  'appletv', 'applemusic', 'crunchyroll', 'deezer', 'discord', 'firefoxbrowser', 'googlechrome', 'kodi', 'max', //
  'netflix', 'obsstudio', 'plex', 'soundcloud', 'spotify', 'steam', 'twitch', 'vlcmediaplayer', 'youtube',
};
const _aliases = {'firefox': 'firefoxbrowser', 'chrome': 'googlechrome', 'vlc': 'vlcmediaplayer', 'obs': 'obsstudio'};

/// The bundled logo for a launcher: the one its config names, else one matching its name.
String? logoFor(String? icon, String name) {
  if (icon != null && _logos.contains(icon)) return icon;
  final key = name.toLowerCase().replaceAll(RegExp('[^a-z0-9]'), '');
  return _logos.contains(key) ? key : _aliases[key];
}

/// The PC's services (`[[agent.shortcut]]`), then its apps (`[agent.apps]`): round keys with their
/// logo, in ink like every other key, or their initials. The service the playback drives is ringed
/// in lavender.
class _Launchers extends StatelessWidget {
  const _Launchers({required this.agent});

  final AgentClient agent;

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final theme = Theme.of(context);
    final items = [
      for (final s in agent.shortcuts) (s.name, logoFor(s.icon, s.name), s.name == agent.active, () => agent.open(s), true),
      for (final app in agent.apps) (app, logoFor(null, app), false, () => agent.send({'type': 'launch', 'app': app}), false),
    ];
    return SizedBox(
      height: 82,
      child: ListView.separated(
        scrollDirection: Axis.horizontal,
        itemCount: items.length + 1,
        separatorBuilder: (_, _) => const SizedBox(width: 12),
        itemBuilder: (context, i) {
          // Last, the way to add sites and the PC's apps.
          if (i == items.length) {
            return SizedBox(
              width: 64,
              child: Column(children: [
                _Round(icon: Icons.add, label: l.addLauncher, size: 56, onPressed: agent.connected ? () => showLaunchers(context, agent) : null),
              ]),
            );
          }
          final (name, logo, active, launch, site) = items[i];
          final enabled = agent.connected;
          final ink = !enabled ? Hm.textDim.withValues(alpha: 0.5) : (active ? Hm.select : Hm.text);
          return GestureDetector(
            // Long press: off the remote (out of homemaid.toml).
            onLongPress: enabled ? () => removeLauncher(context, agent, name, site: site) : null,
            child: SizedBox(
            width: 64,
            child: Column(children: [
              _Round(
                label: l.launchApp(name),
                size: 56,
                selected: active,
                onPressed: enabled ? launch : null,
                child: logo != null
                    ? SvgPicture.asset('assets/logos/$logo.svg', width: 24, height: 24, colorFilter: ColorFilter.mode(ink, BlendMode.srcIn))
                    : Text(_monogram(name), style: theme.textTheme.titleMedium?.copyWith(fontFamily: Hm.condensed, letterSpacing: 0.5)),
              ),
              const SizedBox(height: 6),
              ExcludeSemantics(
                child: Text(
                  name,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: theme.textTheme.labelSmall?.copyWith(color: active ? Hm.select : Hm.textDim),
                ),
              ),
            ]),
            ),
          );
        },
      ),
    );
  }

  /// The initials of a two-word name ("VS Code" -> VC), else its first two letters.
  static String _monogram(String name) {
    final words = name.split(RegExp(r'[\s_-]+')).where((w) => w.isNotEmpty).toList();
    return words.length > 1
        ? '${words[0][0]}${words[1][0]}'.toUpperCase()
        : name[0].toUpperCase() + name.substring(1, min(2, name.length)).toLowerCase();
  }
}

/// Volume in single steps, the PC's real level between the buttons. A press moves it by one;
/// holding repeats, and the step grows so the whole range stays quick to cross.
class _VolumeStepper extends StatefulWidget {
  const _VolumeStepper({required this.agent});

  final AgentClient agent;

  @override
  State<_VolumeStepper> createState() => _VolumeStepperState();
}

class _VolumeStepperState extends State<_VolumeStepper> {
  static const _hold = Duration(milliseconds: 400), _tick = Duration(milliseconds: 140);
  static const _maxStep = 5, _ramp = 4;

  Timer? _timer;
  int _ticks = 0;

  void _nudge(int delta) {
    final level = widget.agent.volume;
    // Stepping from a guess could jump the PC: wait for its real level.
    if (level == null) return;
    final next = (level + delta).clamp(0, 100);
    if (next != level) widget.agent.setVolume(next);
  }

  void _start(int direction) {
    HapticFeedback.selectionClick();
    _ticks = 0;
    _nudge(direction);
    _timer = Timer(_hold, () {
      _timer = Timer.periodic(_tick, (_) {
        _ticks++;
        _nudge(direction * min(1 + _ticks ~/ _ramp, _maxStep));
      });
    });
  }

  void _stop() {
    if (_timer == null) return;
    _timer?.cancel();
    _timer = null;
    // The PC may round or refuse: read back what it really is.
    widget.agent.readState();
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    final level = widget.agent.volume;
    final on = widget.agent.connected && level != null;
    Widget step(IconData icon, String label, int direction) => Listener(
          onPointerDown: on ? (_) => _start(direction) : null,
          onPointerUp: (_) => _stop(),
          onPointerCancel: (_) => _stop(),
          // Taps come from the Listener; the button only draws and serves accessibility.
          child: _Round(icon: icon, label: label, size: 48, onPressed: on ? () {} : null, onActivate: () => _nudge(direction)),
        );
    return Row(mainAxisAlignment: MainAxisAlignment.center, children: [
      step(Icons.remove, l.volumeDown, -1),
      SizedBox(
        width: 72,
        child: Semantics(
          label: l.capVolume,
          value: level == null ? null : '$level %',
          liveRegion: true,
          child: ExcludeSemantics(
            child: Text(
              level?.toString() ?? '--',
              textAlign: TextAlign.center,
              style: Theme.of(context).textTheme.headlineMedium?.copyWith(
                    fontFeatures: Hm.tabular,
                    color: widget.agent.muted ?? false ? Hm.textDim : Hm.text,
                  ),
            ),
          ),
        ),
      ),
      step(Icons.add, l.volumeUp, 1),
    ]);
  }
}

/// Typing on the PC: the phone's keyboard sends each character as it is typed, deletions as
/// Backspace, a new line as Enter; above it, the keys a phone keyboard lacks.
class _Keys extends StatefulWidget {
  const _Keys({required this.agent, required this.onClosed});

  final AgentClient agent;

  /// The phone's keyboard was put away (back, or its own button): typing mode ends with it, so
  /// the keyboard key opens it again in one tap.
  final VoidCallback onClosed;

  @override
  State<_Keys> createState() => _KeysState();
}

class _KeysState extends State<_Keys> with WidgetsBindingObserver {
  /// A zero-width character always sits in the field, so a Backspace on "empty" still arrives.
  static const _sentinel = '\u200B';
  final _field = TextEditingController(text: _sentinel);
  final _focus = FocusNode();

  bool _shown = false;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    WidgetsBinding.instance.addPostFrameCallback((_) => _focus.requestFocus());
  }

  @override
  void didChangeMetrics() {
    final view = View.of(context);
    final open = view.viewInsets.bottom > 0;
    if (_shown && !open) widget.onClosed();
    _shown = open;
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _field.dispose();
    _focus.dispose();
    super.dispose();
  }

  void _changed(String value) {
    if (value.length < _sentinel.length || !value.startsWith(_sentinel)) {
      _key('backspace');
    } else {
      final typed = value.substring(_sentinel.length);
      final lines = typed.split('\n');
      for (final (i, line) in lines.indexed) {
        if (i > 0) _key('enter');
        if (line.isNotEmpty) widget.agent.send({'type': 'text', 'text': line});
      }
    }
    _field.value = const TextEditingValue(text: _sentinel, selection: TextSelection.collapsed(offset: _sentinel.length));
  }

  void _key(String key) => widget.agent.send({'type': 'key', 'key': key});

  @override
  Widget build(BuildContext context) {
    Widget key(String id, {IconData? icon, String? text, required String label}) => _Round(
          icon: icon,
          label: label,
          size: 44,
          onPressed: () => _key(id),
          child: text == null ? null : Text(text, style: Theme.of(context).textTheme.labelMedium),
        );
    return Column(mainAxisSize: MainAxisSize.min, children: [
      // Invisible: it only holds the keyboard open and receives what is typed.
      SizedBox(
        height: 1,
        child: Opacity(
          opacity: 0,
          child: TextField(
            controller: _field,
            focusNode: _focus,
            onChanged: _changed,
            maxLines: null,
            keyboardType: TextInputType.visiblePassword,
            enableSuggestions: false,
            autocorrect: false,
            onTapOutside: (_) {},
            // The keyboard's action key is Enter on the PC; the keyboard stays open.
            onEditingComplete: () => _key('enter'),
          ),
        ),
      ),
      Row(mainAxisAlignment: MainAxisAlignment.spaceBetween, children: [
        key('escape', text: 'Esc', label: 'Escape'),
        key('tab', text: 'Tab', label: 'Tab'),
        key('left', icon: Icons.keyboard_arrow_left, label: '←'),
        key('up', icon: Icons.keyboard_arrow_up, label: '↑'),
        key('down', icon: Icons.keyboard_arrow_down, label: '↓'),
        key('right', icon: Icons.keyboard_arrow_right, label: '→'),
      ]),
    ]);
  }
}

/// A round key: hairline ring, the icon or text in ink. The primary one carries the selection
/// lavender; [selected] fills it lightly. Disabled keys fade to the dim line.
class _Round extends StatelessWidget {
  const _Round({
    this.icon,
    this.child,
    required this.label,
    required this.onPressed,
    this.onActivate,
    this.size = 56,
    this.primary = false,
    this.selected = false,
    this.color,
  });

  final IconData? icon;
  final Widget? child;
  final String label;
  final VoidCallback? onPressed;

  /// For accessibility activation when taps are handled elsewhere (the volume stepper).
  final VoidCallback? onActivate;
  final double size;
  final bool primary, selected;
  final Color? color;

  @override
  Widget build(BuildContext context) {
    final enabled = onPressed != null;
    final accent = color ?? (primary || selected ? Hm.select : Hm.text);
    final ring = !enabled ? Hm.line : (color ?? (primary || selected ? Hm.select : Hm.lineStrong));
    final ink = enabled ? accent : Hm.textDim.withValues(alpha: 0.5);
    return Semantics(
      button: true,
      enabled: enabled,
      label: label,
      onTap: onActivate,
      child: Tooltip(
        message: label,
        excludeFromSemantics: true,
        child: SizedBox.square(
          dimension: max(size, 48),
          child: Center(
            child: Material(
              shape: CircleBorder(side: BorderSide(color: ring, width: primary ? 2 : 1.5)),
              color: selected || primary ? Hm.select.withValues(alpha: enabled ? 0.12 : 0.04) : Colors.transparent,
              clipBehavior: Clip.antiAlias,
              child: InkWell(
                onTap: onPressed == null
                    ? null
                    : () {
                        if (onActivate == null) HapticFeedback.selectionClick();
                        onPressed!();
                      },
                child: SizedBox.square(
                  dimension: size,
                  child: Center(
                    child: ExcludeSemantics(
                      child: IconTheme(
                        data: IconThemeData(color: ink, size: size * 0.42),
                        child: DefaultTextStyle.merge(style: TextStyle(color: ink), child: child ?? Icon(icon)),
                      ),
                    ),
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

/// The trackpad as a sketch plane: a dot grid, a crosshair under each finger with its live
/// coordinates, and a square mark where a click lands. Moves are batched once per frame.
class SketchPlane extends StatefulWidget {
  const SketchPlane({super.key, required this.agent, required this.connections});

  final AgentClient agent;
  final Connections connections;

  @override
  State<SketchPlane> createState() => _SketchPlaneState();
}

class _SketchPlaneState extends State<SketchPlane> with SingleTickerProviderStateMixin {
  final _pad = Trackpad();
  final _fingers = <int, Offset>{};
  Offset? _click;
  double _clickFade = 0;

  late final Ticker _ticker = createTicker((_) {
    for (final request in _pad.flush()) {
      widget.agent.send(request);
    }
    if (_clickFade > 0) setState(() => _clickFade = max(0, _clickFade - 0.06));
    if (!_pad.touching && _clickFade == 0) _ticker.stop();
  });

  int get _now => DateTime.now().millisecondsSinceEpoch;

  void _run() {
    if (!_ticker.isActive) _ticker.start();
  }

  @override
  void dispose() {
    _ticker.dispose();
    super.dispose();
  }

  void _up(PointerEvent e) {
    final click = _pad.up(e.pointer, _now);
    final at = _fingers.remove(e.pointer);
    if (click != null) {
      HapticFeedback.lightImpact();
      widget.agent.send(click);
      _click = at;
      _clickFade = MediaQuery.of(context).disableAnimations ? 0 : 1;
      _run();
    }
    setState(() {});
  }

  @override
  Widget build(BuildContext context) {
    _pad
      ..speed = widget.connections.speed
      ..scrollSign = widget.connections.scrollSign;
    final l = AppLocalizations.of(context);
    return Semantics(
      label: l.trackpadHint,
      child: Listener(
        onPointerDown: (e) {
          _pad.down(e.pointer, Point(e.localPosition.dx, e.localPosition.dy), _now);
          setState(() => _fingers[e.pointer] = e.localPosition);
          _run();
        },
        onPointerMove: (e) {
          _pad.move(e.pointer, Point(e.localPosition.dx, e.localPosition.dy));
          setState(() => _fingers[e.pointer] = e.localPosition);
        },
        onPointerUp: _up,
        onPointerCancel: (e) {
          _pad.up(e.pointer, 0);
          setState(() => _fingers.remove(e.pointer));
        },
        child: ClipRRect(
          borderRadius: BorderRadius.circular(Hm.radius),
          child: CustomPaint(
            painter: _SketchPainter(fingers: _fingers.values.toList(), click: _click, clickFade: _clickFade),
            child: Align(
              alignment: Alignment.bottomCenter,
              child: Padding(
                padding: const EdgeInsets.all(12),
                child: AnimatedOpacity(
                  opacity: _fingers.isEmpty ? 1 : 0,
                  duration: const Duration(milliseconds: 180),
                  child: Text(
                    l.trackpadHint,
                    textAlign: TextAlign.center,
                    style: Theme.of(context).textTheme.bodySmall?.copyWith(color: Hm.textDim),
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _SketchPainter extends CustomPainter {
  _SketchPainter({required this.fingers, required this.click, required this.clickFade});

  final List<Offset> fingers;
  final Offset? click;
  final double clickFade;

  static const _step = 22.0;

  @override
  void paint(Canvas canvas, Size size) {
    final rect = Offset.zero & size;
    canvas.drawRect(rect, Paint()..color = Hm.panel);
    final dot = Paint()..color = Hm.lineStrong;
    for (var y = _step / 2; y < size.height; y += _step) {
      for (var x = _step / 2; x < size.width; x += _step) {
        canvas.drawRect(Rect.fromCenter(center: Offset(x, y), width: 1.6, height: 1.6), dot);
      }
    }
    canvas.drawRRect(
      RRect.fromRectAndRadius(rect.deflate(0.5), const Radius.circular(Hm.radius)),
      Paint()
        ..style = PaintingStyle.stroke
        ..color = Hm.line,
    );

    final cross = Paint()
      ..color = Hm.select.withValues(alpha: 0.7)
      ..strokeWidth = 1;
    for (final f in fingers) {
      canvas.drawLine(Offset(0, f.dy), Offset(size.width, f.dy), cross);
      canvas.drawLine(Offset(f.dx, 0), Offset(f.dx, size.height), cross);
      canvas.drawRect(
        Rect.fromCenter(center: f, width: 14, height: 14),
        Paint()
          ..style = PaintingStyle.stroke
          ..color = Hm.select
          ..strokeWidth = 1.5,
      );
      final readout = TextPainter(
        text: TextSpan(
          text: 'x ${f.dx.round().toString().padLeft(3)}   y ${f.dy.round().toString().padLeft(3)}',
          style: const TextStyle(fontFamily: Hm.condensed, fontSize: 12, color: Hm.select, fontFeatures: Hm.tabular),
        ),
        textDirection: TextDirection.ltr,
      )..layout();
      final at = Offset(
        (f.dx + 12).clamp(4, size.width - readout.width - 4),
        (f.dy - 24).clamp(4, size.height - readout.height - 4),
      );
      readout.paint(canvas, at);
    }

    if (click != null && clickFade > 0) {
      final grow = 1 - clickFade;
      canvas.drawRect(
        Rect.fromCenter(center: click!, width: 14 + 26 * grow, height: 14 + 26 * grow),
        Paint()
          ..style = PaintingStyle.stroke
          ..color = Hm.select.withValues(alpha: clickFade)
          ..strokeWidth = 1.5,
      );
    }
  }

  @override
  bool shouldRepaint(_SketchPainter old) => true;
}

