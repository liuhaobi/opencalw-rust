import 'package:flutter/material.dart';

/// Lobster orange, the upstream brand accent.
const Color kBrandCoral = Color(0xFFFF6B4A);
const Color kCanvas = Color(0xFF0D0C0B);
const Color kSurface = Color(0xFF171514);

ThemeData buildOpenClawTheme() {
  final scheme = ColorScheme.fromSeed(
    seedColor: kBrandCoral,
    brightness: Brightness.dark,
  ).copyWith(
    surface: kCanvas,
  );

  final base = ThemeData(
    useMaterial3: true,
    colorScheme: scheme,
    scaffoldBackgroundColor: kCanvas,
  );

  return base.copyWith(
    appBarTheme: const AppBarTheme(
      backgroundColor: kCanvas,
      surfaceTintColor: Colors.transparent,
      centerTitle: false,
      elevation: 0,
    ),
    inputDecorationTheme: InputDecorationTheme(
      filled: true,
      fillColor: kSurface,
      border: OutlineInputBorder(
        borderRadius: BorderRadius.circular(14),
        borderSide: BorderSide.none,
      ),
      focusedBorder: OutlineInputBorder(
        borderRadius: BorderRadius.circular(14),
        borderSide: const BorderSide(color: kBrandCoral, width: 1.4),
      ),
    ),
    filledButtonTheme: FilledButtonThemeData(
      style: FilledButton.styleFrom(
        minimumSize: const Size.fromHeight(52),
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(14),
        ),
        textStyle: const TextStyle(fontSize: 16, fontWeight: FontWeight.w600),
      ),
    ),
    cardTheme: CardTheme(
      color: kSurface,
      elevation: 0,
      margin: EdgeInsets.zero,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(16),
      ),
    ),
    snackBarTheme: const SnackBarThemeData(
      behavior: SnackBarBehavior.floating,
    ),
  );
}
