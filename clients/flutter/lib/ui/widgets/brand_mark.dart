import 'package:flutter/material.dart';

/// lebi-AI mark — rounded tile with the brand gradient. Used in the app bar,
/// empty state, and drawer.
class BrandMark extends StatelessWidget {
  const BrandMark({super.key, this.size = 36, this.radius});

  /// Tile edge length in dp.
  final double size;
  final double? radius;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final isLight = scheme.brightness == Brightness.light;
    return Container(
      width: size,
      height: size,
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(radius ?? size * 0.28),
        gradient: LinearGradient(
          begin: Alignment.topLeft,
          end: Alignment.bottomRight,
          colors: isLight
              ? [const Color(0xFF13A89A), const Color(0xFF0B6F66)]
              : [const Color(0xFF4DE3CF), const Color(0xFF1E9C8D)],
        ),
        boxShadow: [
          BoxShadow(
            color: scheme.primary.withValues(alpha: isLight ? 0.25 : 0.35),
            blurRadius: size * 0.35,
            offset: Offset(0, size * 0.12),
          ),
        ],
      ),
      child: Icon(
        Icons.north_east, // "send / messenger" arrow
        color: Colors.white,
        size: size * 0.5,
      ),
    );
  }
}
