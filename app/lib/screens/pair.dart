import 'package:flutter/material.dart';
import 'package:mobile_scanner/mobile_scanner.dart';

import '../connections.dart';
import '../l10n/app_localizations.dart';
import '../theme.dart';

/// Scans the QR code of `/pair` and returns the [Connection] it holds.
class PairScreen extends StatefulWidget {
  const PairScreen({super.key});

  @override
  State<PairScreen> createState() => _PairScreenState();
}

class _PairScreenState extends State<PairScreen> {
  bool _done = false;
  bool _warned = false;

  void _onDetect(BarcodeCapture capture) {
    if (_done) return;
    for (final barcode in capture.barcodes) {
      final connection = Connection.fromPairUri(barcode.rawValue ?? '');
      if (connection != null) {
        _done = true;
        Navigator.pop(context, connection);
        return;
      }
    }
    if (!_warned) {
      _warned = true;
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(AppLocalizations.of(context).notHomemaidCode)));
    }
  }

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    return Scaffold(
      appBar: AppBar(title: Text(l.pairTitle), backgroundColor: Hm.ground),
      body: Stack(fit: StackFit.expand, children: [
        MobileScanner(onDetect: _onDetect),
        const CustomPaint(painter: _CropMarks()),
        Align(
          alignment: Alignment.bottomCenter,
          child: SafeArea(
            child: Container(
              margin: const EdgeInsets.all(16),
              padding: const EdgeInsets.all(14),
              decoration: BoxDecoration(
                color: Hm.panel.withValues(alpha: 0.94),
                borderRadius: BorderRadius.circular(Hm.radius),
                border: Border.all(color: Hm.line),
              ),
              child: Text(l.pairInstructions, textAlign: TextAlign.center, style: Theme.of(context).textTheme.bodyMedium),
            ),
          ),
        ),
      ]),
    );
  }
}

/// Crop marks framing the target, like the corners of a drawing sheet.
class _CropMarks extends CustomPainter {
  const _CropMarks();

  @override
  void paint(Canvas canvas, Size size) {
    final side = size.shortestSide * 0.68;
    final r = Rect.fromCenter(center: Offset(size.width / 2, size.height * 0.42), width: side, height: side);
    canvas.drawPath(
      Path.combine(PathOperation.difference, Path()..addRect(Offset.zero & size), Path()..addRect(r)),
      Paint()..color = Hm.ground.withValues(alpha: 0.55),
    );
    final mark = Paint()
      ..color = Hm.select
      ..strokeWidth = 3
      ..style = PaintingStyle.stroke;
    const len = 28.0;
    for (final (corner, dx, dy) in [(r.topLeft, 1.0, 1.0), (r.topRight, -1.0, 1.0), (r.bottomLeft, 1.0, -1.0), (r.bottomRight, -1.0, -1.0)]) {
      canvas.drawPath(
        Path()
          ..moveTo(corner.dx + dx * len, corner.dy)
          ..lineTo(corner.dx, corner.dy)
          ..lineTo(corner.dx, corner.dy + dy * len),
        mark,
      );
    }
  }

  @override
  bool shouldRepaint(_CropMarks old) => false;
}
