import 'package:flutter/material.dart';

enum ConnStatus { online, offline, checking }

/// A small colored dot for connection / turn state, with an optional pulse
/// when indeterminate.
class StatusDot extends StatelessWidget {
  const StatusDot({super.key, required this.status, this.size = 8});

  final ConnStatus status;
  final double size;

  @override
  Widget build(BuildContext context) {
    final (color, pulse) = switch (status) {
      ConnStatus.online => (const Color(0xFF22C55E), false),
      ConnStatus.offline => (const Color(0xFFEF4444), false),
      ConnStatus.checking => (const Color(0xFFF59E0B), true),
    };
    return Container(
      width: size,
      height: size,
      decoration: BoxDecoration(
        color: color,
        shape: BoxShape.circle,
        boxShadow: [BoxShadow(color: color.withValues(alpha: 0.35), blurRadius: 4)],
      ),
      child: pulse
          ? _Pulse(color: color, size: size)
          : null,
    );
  }
}

class _Pulse extends StatefulWidget {
  const _Pulse({required this.color, required this.size});
  final Color color;
  final double size;

  @override
  State<_Pulse> createState() => _PulseState();
}

class _PulseState extends State<_Pulse> with SingleTickerProviderStateMixin {
  late final AnimationController _c = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 900),
  )..repeat(reverse: true);

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return ScaleTransition(
      scale: Tween(begin: 0.7, end: 1.0).animate(
        CurvedAnimation(parent: _c, curve: Curves.easeInOut),
      ),
      child: Container(
        decoration: BoxDecoration(
          color: widget.color,
          shape: BoxShape.circle,
        ),
      ),
    );
  }
}
