/// An automation's nodes drawn as a flow, top to bottom: what starts it on top, a card per node,
/// its ports under it, wires to what follows. Panned and pinched with the fingers. In the editor
/// each port has a "+"; a run's trace, or a dry run, lights the path taken.
library;

import 'package:flutter/material.dart';

import '../flow.dart';
import '../theme.dart';

/// How a node shows a run: went through it (lavender), waits there (pink hatch), failed there
/// (red), or never got there (a ghost).
enum NodeMark { passed, waiting, failed, ghost }

const _cardWidth = 188.0;
const _cardHeight = 64.0;
const _columnWidth = 212.0;
const _layerHeight = 132.0;
const _margin = 48.0;
/// From a card's bottom edge to its port chips' centre.
const _portDrop = 22.0;

/// Port chips: the main way out is a round "+", another way out a "+" with its icon, an `if`'s
/// ways a "+" with their word.
const _chipHeight = 32.0;
const _sideChip = 56.0;
const _ifChip = 84.0;

/// The icon of a way out that is neither the main one nor an `if`'s.
IconData _portIcon(String port) => switch (port) {
      'error' => Icons.error_outline,
      'timeout' => Icons.timer_outlined,
      _ => Icons.repeat,
    };

class FlowCanvas extends StatefulWidget {
  const FlowCanvas({
    super.key,
    required this.graph,
    required this.root,
    required this.summary,
    required this.icon,
    required this.portLabel,
    this.marks,
    this.onRoot,
    this.onNode,
    this.onPort,
  });

  final FlowGraph graph;

  /// The top card: what starts it.
  final String root;
  /// A node in a sentence; its icon says its kind.
  final String Function(Map<String, dynamic> node) summary;
  final IconData Function(Map<String, dynamic> node) icon;
  final String Function(String port) portLabel;

  /// By node; null: no run shown.
  final Map<String, NodeMark>? marks;
  final VoidCallback? onRoot;
  final ValueChanged<String>? onNode;

  /// A port's "+" (`from` null: the start). Null: nothing to add, read only.
  final void Function(String? from, String port)? onPort;

  @override
  State<FlowCanvas> createState() => _FlowCanvasState();
}

class _FlowCanvasState extends State<FlowCanvas> {
  final _view = TransformationController();
  bool _placed = false;

  @override
  void dispose() {
    _view.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final graph = widget.graph;
    final at = graph.layout();
    final columns = [0.0, for (final p in at.values) p.column];
    final (left, right) = (columns.reduce((a, b) => a < b ? a : b), columns.reduce((a, b) => a > b ? a : b));
    final layers = at.values.fold(-1, (a, p) => a > p.layer ? a : p.layer);
    final size = Size((right - left) * _columnWidth + _cardWidth + 2 * _margin, (layers + 2) * _layerHeight + _margin);

    Offset card(String? id) {
      final p = id == null ? (layer: -1, column: 0.0) : at[id]!;
      return Offset(_margin + (p.column - left) * _columnWidth, _margin + (p.layer + 1) * _layerHeight);
    }

    String kind(String? id) => id == null ? 'start' : kindOf(graph.node(id)!);
    List<String> portsOfNode(String? id) => id == null ? const ['start'] : portsOf(kind(id));
    // The main way out in the middle; an `if`'s under its quarters; another flush with the right edge.
    Offset port(String? id, String name) {
      final side = portSide(kind(id), name);
      final offset = kind(id) == 'if' ? _cardWidth / 4 : (_cardWidth - _sideChip) / 2;
      return card(id) + Offset(_cardWidth / 2 + side * offset, _cardHeight + _portDrop);
    }

    final marks = widget.marks;
    bool lit(String? id) => id == null ? marks != null : marks?[id] == NodeMark.passed || marks?[id] == NodeMark.failed;
    final editing = widget.onPort != null;
    // Read only, the main "+" is hidden: its wire leaves the card's edge.
    Offset from(String? id, String name) =>
        editing || portSide(kind(id), name) != 0 ? port(id, name) : card(id) + const Offset(_cardWidth / 2, _cardHeight);
    final wires = [
      for (final l in graph.links)
        (
          from: from(l.from, l.port),
          to: card(l.to) + const Offset(_cardWidth / 2, 0),
          lit: lit(l.from) && marks?[l.to] != null && marks![l.to] != NodeMark.ghost,
        ),
    ];

    final chips = <Widget>[];
    for (final id in [null, ...graph.nodes.map((n) => n['id'] as String)]) {
      // The main "+" last, on top where its touch area meets a side chip's.
      for (final name in [...portsOfNode(id)]..sort((a, b) => (portSide(kind(id), a) == 0 ? 1 : 0) - (portSide(kind(id), b) == 0 ? 1 : 0))) {
        final used = graph.targets(id, name).isNotEmpty;
        // The main way is a bare "+", hidden read only, where the others are named.
        final main = portSide(kind(id), name) == 0;
        final label = main ? '' : widget.portLabel(name);
        if (!editing && (!used || main)) continue;
        final icon = main || kind(id) == 'if' ? null : _portIcon(name);
        final at = port(id, name);
        chips.add(Positioned(
          left: at.dx,
          top: at.dy,
          child: FractionalTranslation(
            translation: const Offset(-0.5, -0.5),
            child: Semantics(
              // The round "+" and the icon chips show no word: they say it here.
              label: widget.portLabel(name),
              button: editing,
              excludeSemantics: true,
              // A 48 dp touch area around the 32 dp chip.
              child: editing
                  ? GestureDetector(
                      behavior: HitTestBehavior.opaque,
                      onTap: () => widget.onPort!(id, name),
                      child: Padding(
                        padding: EdgeInsets.symmetric(vertical: 8, horizontal: label.isEmpty ? 8 : 2),
                        child: _PortChip(label: label, icon: icon, onTap: () => widget.onPort!(id, name)),
                      ),
                    )
                  : _PortChip(label: label, onTap: null),
            ),
          ),
        ));
      }
    }

    return LayoutBuilder(builder: (context, box) {
      if (!_placed && box.hasBoundedWidth) {
        _placed = true;
        // All of its width at first, shrunk to fit if need be (readable down to 60 %), centred.
        final scale = (box.maxWidth / size.width).clamp(0.6, 1.0);
        _view.value = Matrix4.identity()
          ..translateByDouble((box.maxWidth - size.width * scale) / 2, 0, 0, 1)
          ..scaleByDouble(scale, scale, 1, 1);
      }
      return InteractiveViewer(
        transformationController: _view,
        constrained: false,
        boundaryMargin: EdgeInsets.all(box.maxWidth),
        minScale: 0.3,
        maxScale: 2,
        child: SizedBox.fromSize(
          size: size,
          child: Stack(children: [
            Positioned.fill(child: CustomPaint(painter: _Wires(wires))),
            Positioned(
              left: card(null).dx,
              top: card(null).dy,
              child: _Card(
                icon: Icons.bolt,
                summary: widget.root,
                mark: marks == null ? null : NodeMark.passed,
                onTap: widget.onRoot,
              ),
            ),
            for (final n in graph.nodes)
              Positioned(
                left: card(n['id'] as String).dx,
                top: card(n['id'] as String).dy,
                child: _Card(
                  icon: widget.icon(n),
                  summary: widget.summary(n),
                  mark: marks == null ? null : marks[n['id']] ?? NodeMark.ghost,
                  onTap: widget.onNode == null ? null : () => widget.onNode!(n['id'] as String),
                ),
              ),
            ...chips,
          ]),
        ),
      );
    });
  }
}

class _Card extends StatelessWidget {
  const _Card({required this.icon, required this.summary, required this.mark, required this.onTap});

  final IconData icon;
  final String summary;
  final NodeMark? mark;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final border = switch (mark) {
      NodeMark.passed => Hm.select,
      NodeMark.waiting => Hm.pending,
      NodeMark.failed => Hm.danger,
      _ => Hm.lineStrong,
    };
    final accent = switch (mark) {
      NodeMark.waiting => Hm.pending,
      NodeMark.failed => Hm.danger,
      NodeMark.passed => Hm.select,
      _ => Hm.textDim,
    };
    return Opacity(
      opacity: mark == NodeMark.ghost ? 0.4 : 1,
      child: Semantics(
        button: onTap != null,
        label: summary,
        excludeSemantics: true,
        child: Material(
          color: Hm.panel,
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(Hm.radius),
            side: BorderSide(color: border, width: mark == null || mark == NodeMark.ghost ? 1 : 1.5),
          ),
          clipBehavior: Clip.antiAlias,
          child: InkWell(
            onTap: onTap,
            child: CustomPaint(
              painter: mark == NodeMark.waiting ? const _Hatch() : null,
              child: SizedBox(
                width: _cardWidth,
                height: _cardHeight,
                child: Padding(
                  padding: const EdgeInsets.fromLTRB(10, 6, 10, 6),
                  child: Row(children: [
                    Icon(icon, size: 20, color: accent),
                    const SizedBox(width: 10),
                    Expanded(
                      // Cut to the card, whatever the text size.
                      child: Text(summary, style: theme.textTheme.bodySmall, maxLines: 3, overflow: TextOverflow.ellipsis),
                    ),
                  ]),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _PortChip extends StatelessWidget {
  const _PortChip({required this.label, required this.onTap, this.icon});

  /// Empty: the main way out, a round "+".
  final String label;

  /// Instead of the label while editing: a side way out.
  final IconData? icon;

  /// Null: read only, the label as a caption.
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final editing = onTap != null;
    final round = editing && label.isEmpty;
    final iconOnly = editing && icon != null;
    // Even leading: the condensed face sits centred on the "+", not on its baseline.
    final style = Theme.of(context).textTheme.labelSmall?.copyWith(
          color: editing ? Hm.select : Hm.textDim,
          height: 1,
          leadingDistribution: TextLeadingDistribution.even,
        );
    return Material(
      color: Hm.ground,
      shape: round
          ? const CircleBorder(side: BorderSide(color: Hm.select))
          : StadiumBorder(side: BorderSide(color: editing ? Hm.select : Hm.line)),
      child: InkWell(
        customBorder: round ? const CircleBorder() : const StadiumBorder(),
        onTap: onTap,
        child: SizedBox(
          width: round ? _chipHeight : (iconOnly ? _sideChip : (editing ? _ifChip : null)),
          // A caption only as tall as its text.
          height: editing ? _chipHeight : 22,
          child: Padding(
            padding: EdgeInsets.symmetric(horizontal: round ? 0 : (iconOnly ? 8 : 10)),
            child: Row(
              mainAxisSize: editing ? MainAxisSize.max : MainAxisSize.min,
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                if (editing) const Icon(Icons.add, size: 16, color: Hm.select),
                if (iconOnly) ...[const SizedBox(width: 2), Icon(icon, size: 16, color: Hm.select)],
                if (!round && !iconOnly) ...[
                  if (editing) const SizedBox(width: 4),
                  Flexible(child: Text(label, style: style, maxLines: 1, overflow: TextOverflow.ellipsis)),
                ],
              ],
            ),
          ),
        ),
      ),
    );
  }
}

typedef _Wire = ({Offset from, Offset to, bool lit});

class _Wires extends CustomPainter {
  _Wires(this.wires);

  final List<_Wire> wires;

  @override
  void paint(Canvas canvas, Size size) {
    for (final w in wires) {
      final paint = Paint()
        ..style = PaintingStyle.stroke
        ..strokeWidth = w.lit ? 2 : 1.4
        ..color = w.lit ? Hm.select : Hm.lineStrong;
      final bend = (w.to.dy - w.from.dy).abs() / 2;
      canvas.drawPath(
        Path()
          ..moveTo(w.from.dx, w.from.dy)
          ..cubicTo(w.from.dx, w.from.dy + bend, w.to.dx, w.to.dy - bend, w.to.dx, w.to.dy),
        paint,
      );
      // Which way it goes.
      canvas.drawPath(
        Path()
          ..moveTo(w.to.dx - 4, w.to.dy - 6)
          ..lineTo(w.to.dx + 4, w.to.dy - 6)
          ..lineTo(w.to.dx, w.to.dy)
          ..close(),
        Paint()..color = paint.color,
      );
    }
  }

  @override
  bool shouldRepaint(_Wires old) => true;
}

/// The pink hatch of something still pending: here, a run waiting on the node.
class _Hatch extends CustomPainter {
  const _Hatch();

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..color = Hm.pending.withValues(alpha: 0.22)
      ..strokeWidth = 1.4;
    for (var x = -size.height; x < size.width; x += 7) {
      canvas.drawLine(Offset(x, size.height), Offset(x + size.height, 0), paint);
    }
  }

  @override
  bool shouldRepaint(_Hatch old) => false;
}
