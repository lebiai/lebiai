import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'package:hermes_app/app.dart';
import 'package:hermes_app/ui/features/connection/view_models/connection_providers.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  final prefs = await SharedPreferences.getInstance();
  const storage = FlutterSecureStorage();
  final url = prefs.getString(serverUrlKey) ?? defaultServerUrl;
  final token = await storage.read(key: serverTokenKey) ?? '';

  runApp(
    ProviderScope(
      overrides: [
        serverUrlProvider.overrideWith(() => ServerUrlNotifier(prefs, url)),
        serverTokenProvider.overrideWith(() => ServerTokenNotifier(storage, token)),
      ],
      child: const HermesApp(),
    ),
  );
}
