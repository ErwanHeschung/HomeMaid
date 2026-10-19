/// The "Viewport" world: a live CAD model of the home. Graphite ground, matte clay greys, and the
/// logo's two colours: its lavender, primary, for everything (HomeMaid, interaction, "physically
/// on"), its pink, secondary, for a request still pending only.
library;


import 'package:flutter/material.dart';

abstract final class Hm {
  // Grounds, from the viewport outwards.
  static const ground = Color(0xFF15171B);
  static const panel = Color(0xFF1C1F24);
  static const raised = Color(0xFF252930);
  static const line = Color(0xFF343A42);
  static const lineStrong = Color(0xFF4B525C);

  static const text = Color(0xFFEEF0F2);
  static const textDim = Color(0xFFA2A9B3);

  /// The logo's lavender, the primary colour: HomeMaid itself (the active tab, the main button),
  /// interaction, focus, selection, and "physically on".
  static const select = Color(0xFFA699DC);
  static const selectInk = Color(0xFF1E1638);

  static const brand = select;
  static const brandInk = selectInk;

  /// "Physically on" (a powered socket, a lit bulb, a running screen), from the observed state only.
  static const on = select;
  static const onInk = selectInk;

  /// The logo's pink, secondary: a request sent and not yet confirmed. Nothing else is pink.
  static const pending = Color(0xFFFD77B1);
  static const pendingInk = Color(0xFF3A0A22);

  /// A true red, well apart from the pink.
  static const danger = Color(0xFFFF5545);

  static const radius = 6.0;

  /// A switch whose change the device has not confirmed yet: pink.
  static Widget pendingSwitch(BuildContext context, Widget child) {
    final theme = Theme.of(context);
    return Theme(
      data: theme.copyWith(
        switchTheme: theme.switchTheme.copyWith(
          thumbColor: WidgetStateProperty.resolveWith((s) {
            if (s.contains(WidgetState.disabled)) return lineStrong;
            return s.contains(WidgetState.selected) ? pendingInk : textDim;
          }),
          trackColor: WidgetStateProperty.resolveWith((s) {
            if (s.contains(WidgetState.disabled)) return raised;
            return s.contains(WidgetState.selected) ? pending : raised;
          }),
          trackOutlineColor: WidgetStateProperty.resolveWith((s) => s.contains(WidgetState.selected) ? pending : lineStrong),
        ),
      ),
      child: child,
    );
  }

  static const condensed = 'Barlow Semi Condensed';
  static const tabular = [FontFeature.tabularFigures()];

  static ThemeData theme() {
    const scheme = ColorScheme(
      brightness: Brightness.dark,
      primary: select,
      onPrimary: selectInk,
      primaryContainer: Color(0xFF2E2950),
      onPrimaryContainer: text,
      secondary: pending,
      onSecondary: pendingInk,
      tertiary: pending,
      onTertiary: pendingInk,
      error: danger,
      onError: Color(0xFF3A0905),
      surface: ground,
      onSurface: text,
      onSurfaceVariant: textDim,
      surfaceContainerLowest: Color(0xFF111316),
      surfaceContainerLow: Color(0xFF181B1F),
      surfaceContainer: panel,
      surfaceContainerHigh: raised,
      surfaceContainerHighest: Color(0xFF2D3239),
      outline: lineStrong,
      outlineVariant: line,
      inverseSurface: text,
      onInverseSurface: ground,
      inversePrimary: Color(0xFF6A5BB8),
    );

    const family = 'Barlow';
    final base = ThemeData(useMaterial3: true, colorScheme: scheme, fontFamily: family);
    final type = base.textTheme.copyWith(
      displaySmall: const TextStyle(fontSize: 34, fontWeight: FontWeight.w600, height: 1.1, letterSpacing: -0.4),
      headlineSmall: const TextStyle(fontSize: 24, fontWeight: FontWeight.w600, height: 1.2),
      titleLarge: const TextStyle(fontSize: 20, fontWeight: FontWeight.w600),
      titleMedium: const TextStyle(fontSize: 16, fontWeight: FontWeight.w600, letterSpacing: 0.1),
      titleSmall: const TextStyle(fontSize: 14, fontWeight: FontWeight.w600, letterSpacing: 0.1),
      bodyLarge: const TextStyle(fontSize: 16, fontWeight: FontWeight.w400, height: 1.35),
      bodyMedium: const TextStyle(fontSize: 14, fontWeight: FontWeight.w400, height: 1.35),
      bodySmall: const TextStyle(fontSize: 12, fontWeight: FontWeight.w400, height: 1.3),
      labelLarge: const TextStyle(fontSize: 15, fontWeight: FontWeight.w600, letterSpacing: 0.2),
      labelMedium: const TextStyle(fontFamily: condensed, fontSize: 13, fontWeight: FontWeight.w500, letterSpacing: 0.2),
      labelSmall: const TextStyle(fontFamily: condensed, fontSize: 12, fontWeight: FontWeight.w500, letterSpacing: 0.3),
    )
        // The styles above are new TextStyles: without this they lose Barlow for the platform
        // font. The condensed labels keep their own family.
        .apply(fontFamily: family, bodyColor: text, displayColor: text)
        .copyWith(
          labelMedium: const TextStyle(fontFamily: condensed, fontSize: 13, fontWeight: FontWeight.w500, letterSpacing: 0.2, color: text),
          labelSmall: const TextStyle(fontFamily: condensed, fontSize: 12, fontWeight: FontWeight.w500, letterSpacing: 0.3, color: text),
        );

    final shape = RoundedRectangleBorder(borderRadius: BorderRadius.circular(radius));
    final hairline = RoundedRectangleBorder(
      borderRadius: BorderRadius.circular(radius),
      side: const BorderSide(color: lineStrong),
    );

    return base.copyWith(
      scaffoldBackgroundColor: ground,
      textTheme: type,
      splashFactory: InkSparkle.splashFactory,
      dividerTheme: const DividerThemeData(color: line, thickness: 1, space: 1),
      navigationBarTheme: NavigationBarThemeData(
        backgroundColor: panel,
        surfaceTintColor: Colors.transparent,
        indicatorColor: brand.withValues(alpha: 0.16),
        indicatorShape: shape,
        height: 68,
        iconTheme: WidgetStateProperty.resolveWith(
          (s) => IconThemeData(color: s.contains(WidgetState.selected) ? brand : textDim),
        ),
        labelTextStyle: WidgetStateProperty.resolveWith(
          (s) => type.labelMedium!.copyWith(color: s.contains(WidgetState.selected) ? text : textDim),
        ),
      ),
      switchTheme: SwitchThemeData(
        thumbColor: WidgetStateProperty.resolveWith((s) {
          if (s.contains(WidgetState.disabled)) return lineStrong;
          return s.contains(WidgetState.selected) ? onInk : textDim;
        }),
        trackColor: WidgetStateProperty.resolveWith((s) {
          if (s.contains(WidgetState.disabled)) return raised;
          return s.contains(WidgetState.selected) ? on : raised;
        }),
        trackOutlineColor: WidgetStateProperty.resolveWith(
          (s) => s.contains(WidgetState.selected) ? on : lineStrong,
        ),
      ),
      sliderTheme: const SliderThemeData(
        activeTrackColor: select,
        inactiveTrackColor: raised,
        thumbColor: text,
        overlayColor: Color(0x29A699DC),
        trackHeight: 4,
      ),
      filledButtonTheme: FilledButtonThemeData(
        style: FilledButton.styleFrom(
          minimumSize: const Size(64, 48),
          shape: shape,
          textStyle: type.labelLarge,
          backgroundColor: brand,
          foregroundColor: brandInk,
        ),
      ),
      outlinedButtonTheme: OutlinedButtonThemeData(
        style: OutlinedButton.styleFrom(
          minimumSize: const Size(48, 48),
          shape: hairline,
          side: const BorderSide(color: lineStrong),
          foregroundColor: text,
          textStyle: type.labelLarge,
        ),
      ),
      textButtonTheme: TextButtonThemeData(
        style: TextButton.styleFrom(minimumSize: const Size(48, 48), shape: shape, foregroundColor: select),
      ),
      inputDecorationTheme: InputDecorationTheme(
        filled: true,
        fillColor: panel,
        isDense: true,
        contentPadding: const EdgeInsets.symmetric(horizontal: 12, vertical: 14),
        border: OutlineInputBorder(borderRadius: BorderRadius.circular(radius), borderSide: const BorderSide(color: line)),
        enabledBorder:
            OutlineInputBorder(borderRadius: BorderRadius.circular(radius), borderSide: const BorderSide(color: line)),
        focusedBorder: OutlineInputBorder(
          borderRadius: BorderRadius.circular(radius),
          borderSide: const BorderSide(color: select, width: 1.5),
        ),
        hintStyle: type.bodyMedium!.copyWith(color: textDim),
        labelStyle: type.bodyMedium!.copyWith(color: textDim),
      ),
      // Choices are interaction: lavender when picked, never the secondary (pink means pending).
      segmentedButtonTheme: SegmentedButtonThemeData(
        style: ButtonStyle(
          shape: WidgetStatePropertyAll(shape),
          side: const WidgetStatePropertyAll(BorderSide(color: lineStrong)),
          minimumSize: const WidgetStatePropertyAll(Size(48, 44)),
          padding: const WidgetStatePropertyAll(EdgeInsets.symmetric(horizontal: 6)),
          textStyle: WidgetStatePropertyAll(type.titleSmall),
          backgroundColor: WidgetStateProperty.resolveWith(
            (s) => s.contains(WidgetState.selected) ? select.withValues(alpha: 0.16) : Colors.transparent,
          ),
          foregroundColor: WidgetStateProperty.resolveWith((s) => s.contains(WidgetState.selected) ? select : textDim),
        ),
      ),
      chipTheme: ChipThemeData(
        backgroundColor: Colors.transparent,
        selectedColor: select.withValues(alpha: 0.16),
        showCheckmark: false,
        shape: shape,
        side: WidgetStateBorderSide.resolveWith((s) => BorderSide(color: s.contains(WidgetState.selected) ? select : lineStrong)),
        labelStyle: type.titleSmall!.copyWith(color: text),
      ),
      appBarTheme: AppBarTheme(
        backgroundColor: ground,
        surfaceTintColor: Colors.transparent,
        scrolledUnderElevation: 0,
        titleTextStyle: type.titleLarge,
      ),
      bottomSheetTheme: const BottomSheetThemeData(
        backgroundColor: panel,
        surfaceTintColor: Colors.transparent,
        showDragHandle: true,
        dragHandleColor: lineStrong,
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.vertical(top: Radius.circular(12))),
      ),
      dialogTheme: DialogThemeData(
        backgroundColor: panel,
        surfaceTintColor: Colors.transparent,
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(10), side: const BorderSide(color: line)),
      ),
      snackBarTheme: SnackBarThemeData(
        backgroundColor: raised,
        contentTextStyle: type.bodyMedium,
        behavior: SnackBarBehavior.floating,
        shape: hairline,
      ),
      listTileTheme: const ListTileThemeData(iconColor: textDim, minVerticalPadding: 12),
      progressIndicatorTheme: const ProgressIndicatorThemeData(color: select),
      iconTheme: const IconThemeData(color: textDim),
    );
  }
}
