import 'package:flutter/material.dart';

import 'app_dimensions.dart';

/// lebi-AI design system.
///
/// One confident accent (teal) on cool-neutral surfaces, instead of the
/// default Material indigo seed. Light and dark are hand-tuned so bubbles,
/// dividers and input rings read correctly in both modes.
class HermesColors {
  const HermesColors._();

  // Brand accent -----------------------------------------------------------
  static const Color lightPrimary = Color(0xFF0F8378);
  static const Color lightPrimaryContainer = Color(0xFFCDEEE9);
  static const Color lightOnPrimaryContainer = Color(0xFF0A2E2B);

  static const Color darkPrimary = Color(0xFF3DD9C4);
  static const Color darkPrimaryContainer = Color(0xFF1C504A);
  static const Color darkOnPrimaryContainer = Color(0xFFB7F1E8);

  // Surfaces ---------------------------------------------------------------
  static const Color lightBg = Color(0xFFF7F8FA);
  static const Color lightSurface = Color(0xFFFFFFFF);
  static const Color lightBubbleAssistant = Color(0xFFFFFFFF);
  static const Color lightHairline = Color(0xFFE5E8EC);
  static const Color lightMuted = Color(0xFF6B7280);

  static const Color darkBg = Color(0xFF0F1113);
  static const Color darkSurface = Color(0xFF171A1D);
  static const Color darkBubbleAssistant = Color(0xFF1C2024);
  static const Color darkHairline = Color(0xFF2A2F35);
  static const Color darkMuted = Color(0xFF98A0A8);
}

ThemeData appLightTheme() {
  final scheme = const ColorScheme(
    brightness: Brightness.light,
    primary: HermesColors.lightPrimary,
    onPrimary: Colors.white,
    primaryContainer: HermesColors.lightPrimaryContainer,
    onPrimaryContainer: HermesColors.lightOnPrimaryContainer,
    secondary: Color(0xFF455A64),
    onSecondary: Colors.white,
    secondaryContainer: Color(0xFFE7EBEF),
    onSecondaryContainer: Color(0xFF1B2210),
    tertiary: Color(0xFF6D5AE6),
    onTertiary: Colors.white,
    error: Color(0xFFC62828),
    onError: Colors.white,
    surface: HermesColors.lightSurface,
    onSurface: Color(0xFF1A1C1E),
    surfaceContainerHighest: Color(0xFFEFF1F4),
    onSurfaceVariant: Color(0xFF555B62),
    outline: Color(0xFFCBD0D6),
    outlineVariant: HermesColors.lightHairline,
  );
  return _base(scheme, HermesColors.lightBg);
}

ThemeData appDarkTheme() {
  final scheme = const ColorScheme(
    brightness: Brightness.dark,
    primary: HermesColors.darkPrimary,
    onPrimary: Color(0xFF00322E),
    primaryContainer: HermesColors.darkPrimaryContainer,
    onPrimaryContainer: HermesColors.darkOnPrimaryContainer,
    secondary: Color(0xFF9EC6D8),
    onSecondary: Color(0xFF0E2A33),
    secondaryContainer: Color(0xFF222A30),
    onSecondaryContainer: Color(0xFFBFE6F4),
    tertiary: Color(0xFFB3A4FF),
    onTertiary: Color(0xFF1E1248),
    error: Color(0xFFFF8082),
    onError: Color(0xFF4A0002),
    surface: HermesColors.darkSurface,
    onSurface: Color(0xFFE5E7EA),
    surfaceContainerHighest: HermesColors.darkBubbleAssistant,
    onSurfaceVariant: Color(0xFF9AA1A9),
    outline: Color(0xFF3A4148),
    outlineVariant: HermesColors.darkHairline,
  );
  return _base(scheme, HermesColors.darkBg);
}

ThemeData _base(ColorScheme scheme, Color scaffoldBg) {
  final isLight = scheme.brightness == Brightness.light;
  final base = ThemeData(
    useMaterial3: true,
    colorScheme: scheme,
    scaffoldBackgroundColor: scaffoldBg,
    splashFactory: InkSparkle.splashFactory,
    visualDensity: VisualDensity.standard,
  );

  return base.copyWith(
    textTheme: _textTheme(scheme),
    appBarTheme: AppBarTheme(
      backgroundColor: scaffoldBg,
      foregroundColor: scheme.onSurface,
      elevation: 0,
      scrolledUnderElevation: 0,
      centerTitle: false,
      titleTextStyle: base.textTheme.titleLarge!.copyWith(
        fontWeight: FontWeight.w700,
        letterSpacing: -0.2,
      ),
    ),
    cardTheme: CardThemeData(
      color: scheme.surface,
      elevation: 0,
      margin: EdgeInsets.zero,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(HermesRadius.lg),
        side: BorderSide(color: scheme.outlineVariant),
      ),
    ),
    dividerTheme: DividerThemeData(
      color: scheme.outlineVariant,
      space: 1,
      thickness: 1,
    ),
    inputDecorationTheme: _inputTheme(scheme),
    filledButtonTheme: FilledButtonThemeData(
      style: FilledButton.styleFrom(
        backgroundColor: scheme.primary,
        foregroundColor: scheme.onPrimary,
        padding: const EdgeInsets.symmetric(horizontal: 20, vertical: 14),
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(HermesRadius.md),
        ),
        textStyle: const TextStyle(fontWeight: FontWeight.w600, fontSize: 15),
      ),
    ),
    textButtonTheme: TextButtonThemeData(
      style: TextButton.styleFrom(foregroundColor: scheme.primary),
    ),
    iconButtonTheme: IconButtonThemeData(
      style: IconButton.styleFrom(
        foregroundColor: isLight ? HermesColors.lightMuted : HermesColors.darkMuted,
        shape: const CircleBorder(),
      ),
    ),
    listTileTheme: ListTileThemeData(
      iconColor: scheme.onSurfaceVariant,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(HermesRadius.md),
      ),
    ),
    tabBarTheme: TabBarThemeData(
      labelColor: scheme.primary,
      unselectedLabelColor: scheme.onSurfaceVariant,
      indicatorColor: scheme.primary,
      labelStyle: const TextStyle(fontWeight: FontWeight.w600, fontSize: 14),
      unselectedLabelStyle: const TextStyle(fontSize: 14),
      dividerColor: Colors.transparent,
      indicatorSize: TabBarIndicatorSize.label,
    ),
    snackBarTheme: SnackBarThemeData(
      behavior: SnackBarBehavior.floating,
      backgroundColor: isLight ? const Color(0xFF2A2D31) : const Color(0xFFE8EAED),
      contentTextStyle: TextStyle(
        color: isLight ? Colors.white : const Color(0xFF1A1C1E),
      ),
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(HermesRadius.md),
      ),
    ),
    dialogTheme: DialogThemeData(
      backgroundColor: scheme.surface,
      elevation: 0,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(HermesRadius.xl),
        side: BorderSide(color: scheme.outlineVariant),
      ),
    ),
    progressIndicatorTheme: ProgressIndicatorThemeData(
      color: scheme.primary,
      linearTrackColor: scheme.surfaceContainerHighest,
    ),
  );
}

TextTheme _textTheme(ColorScheme scheme) {
  // IMPORTANT: every style gets an explicit color. A custom TextTheme handed
  // to ThemeData is NOT auto-tinted by colorScheme — leaving color null makes
  // TextField content / hero headlines etc. render as a washed-out default
  // grey. `onSurface` is the primary text color, `onSurfaceVariant` the
  // muted/caption color.
  final ink = scheme.onSurface;
  final muted = scheme.onSurfaceVariant;
  TextStyle ts(Color c) =>
      TextStyle(color: c, fontFamilyFallback: const ['SF Pro', 'Inter', 'Roboto']);
  return TextTheme(
    displayLarge: ts(ink).copyWith(fontSize: 32, fontWeight: FontWeight.w700, height: 1.2, letterSpacing: -0.5),
    displayMedium: ts(ink).copyWith(fontSize: 26, fontWeight: FontWeight.w700, height: 1.25, letterSpacing: -0.4),
    headlineMedium: ts(ink).copyWith(fontSize: 22, fontWeight: FontWeight.w700, height: 1.3, letterSpacing: -0.3),
    headlineSmall: ts(ink).copyWith(fontSize: 20, fontWeight: FontWeight.w600, height: 1.3),
    titleLarge: ts(ink).copyWith(fontSize: 19, fontWeight: FontWeight.w700, height: 1.3, letterSpacing: -0.2),
    titleMedium: ts(ink).copyWith(fontSize: 16, fontWeight: FontWeight.w600, height: 1.35),
    titleSmall: ts(ink).copyWith(fontSize: 14, fontWeight: FontWeight.w600, height: 1.35),
    bodyLarge: ts(ink).copyWith(fontSize: 15.5, fontWeight: FontWeight.w400, height: 1.55),
    bodyMedium: ts(ink).copyWith(fontSize: 14.5, fontWeight: FontWeight.w400, height: 1.55),
    bodySmall: ts(muted).copyWith(fontSize: 12.5, fontWeight: FontWeight.w400, height: 1.45),
    labelLarge: ts(ink).copyWith(fontSize: 14, fontWeight: FontWeight.w600, height: 1.3),
    labelMedium: ts(muted).copyWith(fontSize: 12.5, fontWeight: FontWeight.w600, height: 1.3, letterSpacing: 0.1),
    labelSmall: ts(muted).copyWith(fontSize: 11, fontWeight: FontWeight.w500, height: 1.3, letterSpacing: 0.2),
  );
}

InputDecorationTheme _inputTheme(ColorScheme scheme) {
  final fill = scheme.brightness == Brightness.light
      ? const Color(0xFFEFF1F4)
      : const Color(0xFF1C2024);
  return InputDecorationTheme(
    filled: true,
    fillColor: fill,
    hintStyle: TextStyle(color: scheme.onSurfaceVariant.withValues(alpha: 0.7)),
    contentPadding: const EdgeInsets.symmetric(horizontal: 14, vertical: 12),
    border: OutlineInputBorder(
      borderRadius: BorderRadius.circular(HermesRadius.md),
      borderSide: BorderSide.none,
    ),
    enabledBorder: OutlineInputBorder(
      borderRadius: BorderRadius.circular(HermesRadius.md),
      borderSide: BorderSide.none,
    ),
    focusedBorder: OutlineInputBorder(
      borderRadius: BorderRadius.circular(HermesRadius.md),
      borderSide: BorderSide(color: scheme.primary, width: 1.5),
    ),
    errorBorder: OutlineInputBorder(
      borderRadius: BorderRadius.circular(HermesRadius.md),
      borderSide: BorderSide(color: scheme.error, width: 1.5),
    ),
  );
}
