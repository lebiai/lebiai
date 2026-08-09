import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'package:hermes_app/ui/features/chat/views/chat_screen.dart';
import 'package:hermes_app/ui/theme/app_theme.dart';

/// Root widget. Hosts the chat screen as home (connection lives in the
/// drawer + settings, so the user lands straight in the conversation).
class HermesApp extends StatelessWidget {
  const HermesApp({super.key});

  @override
  Widget build(BuildContext context) {
    // Edge-to-edge, but with a sensible default that lets content scroll
    // even under translucent bars, and a softer scroll glow.
    return AnnotatedRegion<SystemUiOverlayStyle>(
      value: SystemUiOverlayStyle.dark,
      child: MaterialApp(
        title: 'lebi-AI',
        debugShowCheckedModeBanner: false,
        theme: appLightTheme(),
        darkTheme: appDarkTheme(),
        themeMode: ThemeMode.system,
        scrollBehavior: const _HermesScrollBehavior(),
        home: const ChatScreen(),
      ),
    );
  }
}

class _HermesScrollBehavior extends MaterialScrollBehavior {
  const _HermesScrollBehavior();

  // Drag works with any pointer (trackpad / mouse too), not just touch.
  @override
  Set<PointerDeviceKind> get dragDevices => const {
        PointerDeviceKind.touch,
        PointerDeviceKind.mouse,
        PointerDeviceKind.trackpad,
      };
}
