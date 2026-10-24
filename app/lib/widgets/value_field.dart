/// A capability's value as a field to set, for the editors (automations, scenes): on/off, a
/// slider between its bounds, or typed.
library;

import 'package:flutter/material.dart';

import '../l10n/app_localizations.dart';
import '../screens/automations.dart' show valueText;
import '../theme.dart';

/// On/off (a switch, a boolean setting).
bool isBoolCap(String cap, Map<String, dynamic> meta) =>
    cap == 'on_off' || cap == 'mute' || (meta['schema'] as Map?)?['type'] == 'boolean';

/// A level or a number.
bool isNumberCap(String cap, Map<String, dynamic> meta) =>
    const {'brightness', 'volume', 'power'}.contains(cap) || meta['max'] is num || (meta['schema'] as Map?)?['type'] == 'number';

/// What a new value starts at: on, the middle of its range, or empty.
Object defaultCapValue(String cap, Map<String, dynamic> meta) =>
    isBoolCap(cap, meta) ? true : isNumberCap(cap, meta) ? ((meta['max'] as num? ?? 100) / 2).round() : '';

/// A capability's value: on/off, a slider between its bounds, or typed.
class ValueField extends StatelessWidget {
  const ValueField({super.key, required this.cap, required this.meta, required this.value, required this.onChanged, this.number = false});

  final String cap;
  final Map<String, dynamic> meta;
  final Object? value;
  final ValueChanged<Object> onChanged;

  /// A threshold: always a number.
  final bool number;

  @override
  Widget build(BuildContext context) {
    final l = AppLocalizations.of(context);
    if (!number && isBoolCap(cap, meta)) {
      return SegmentedButton<bool>(
        showSelectedIcon: false,
        segments: [
          ButtonSegment(value: true, label: Text(valueText(context, cap, true))),
          ButtonSegment(value: false, label: Text(valueText(context, cap, false))),
        ],
        selected: {value == true},
        onSelectionChanged: (s) => onChanged(s.first),
      );
    }
    final (min, max) = (meta['min'] as num? ?? 0, meta['max'] as num? ?? (const {'brightness', 'volume'}.contains(cap) ? 100 : null));
    if ((number || isNumberCap(cap, meta)) && max != null) {
      final v = ((value as num?) ?? min).clamp(min, max).toDouble();
      return Row(children: [
        Expanded(
          child: Slider(
            value: v,
            min: min.toDouble(),
            max: max.toDouble(),
            divisions: (max - min).round().clamp(1, 1000),
            semanticFormatterCallback: (x) => valueText(context, cap, x.round()),
            onChanged: (x) => onChanged(x.round()),
          ),
        ),
        SizedBox(
          width: 64,
          child: Text(valueText(context, cap, v.round()), textAlign: TextAlign.end, style: const TextStyle(fontFeatures: Hm.tabular)),
        ),
      ]);
    }
    final numeric = number || isNumberCap(cap, meta);
    return TextValueField(
      label: l.valueLabel,
      value: value == null ? '' : '$value',
      number: numeric,
      onChanged: (text) => onChanged(numeric ? (num.tryParse(text.replaceAll(',', '.')) ?? 0) : text),
    );
  }
}

class TextValueField extends StatefulWidget {
  const TextValueField({super.key, required this.label, required this.value, required this.onChanged, this.number = false, this.error});

  final String label;
  final String value;
  final ValueChanged<String> onChanged;
  final bool number;
  final String? error;

  @override
  State<TextValueField> createState() => TextValueFieldFieldState();
}

class TextValueFieldFieldState extends State<TextValueField> {
  late final _text = TextEditingController(text: widget.value);

  @override
  void didUpdateWidget(TextValueField oldWidget) {
    super.didUpdateWidget(oldWidget);
    // Changed from elsewhere (a chip): shown; what is being typed is left alone.
    if (widget.value != oldWidget.value && widget.value != _text.text) _text.text = widget.value;
  }

  @override
  void dispose() {
    _text.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => TextField(
        controller: _text,
        decoration: InputDecoration(labelText: widget.label, errorText: widget.error),
        keyboardType: widget.number ? const TextInputType.numberWithOptions(decimal: true, signed: true) : TextInputType.text,
        onChanged: widget.onChanged,
      );
}

