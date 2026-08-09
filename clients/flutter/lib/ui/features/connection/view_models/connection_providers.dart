import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'package:hermes_app/data/models/health.dart';
import 'package:hermes_app/data/services/hermes_client.dart';

const serverUrlKey = 'hermes_server_url';
const serverTokenKey = 'hermes_server_token';
const defaultServerUrl = 'http://localhost:8765';

/// User-editable server base URL. Default points at localhost for local dev
/// (desktop / iOS simulator). Android emulators need `10.0.2.2` instead.
final serverUrlProvider =
    NotifierProvider<ServerUrlNotifier, String>(ServerUrlNotifier.new);

class ServerUrlNotifier extends Notifier<String> {
  /// Optional bootstrap: persisted value + prefs handle (injected from
  /// `main()` via `overrideWith`).
  ServerUrlNotifier([this._prefs, this._initial]);

  final SharedPreferences? _prefs;
  final String? _initial;

  @override
  String build() => _initial ?? defaultServerUrl;

  void set(String url) {
    state = url;
    _prefs?.setString(serverUrlKey, url);
  }
}

/// Bearer token the server requires (printed once on `hermes serve` startup).
/// Stored in OS secure storage (iOS/macOS Keychain, Android Keystore) — never
/// plaintext prefs. Empty string ⇒ unauthenticated (server will 401 every request).
final serverTokenProvider =
    NotifierProvider<ServerTokenNotifier, String>(ServerTokenNotifier.new);

class ServerTokenNotifier extends Notifier<String> {
  ServerTokenNotifier([this._storage, this._initial]);

  final FlutterSecureStorage? _storage;
  final String? _initial;

  @override
  String build() => _initial ?? '';

  Future<void> set(String token) async {
    state = token;
    if (token.isEmpty) {
      await _storage?.delete(key: serverTokenKey);
    } else {
      await _storage?.write(key: serverTokenKey, value: token);
    }
  }
}

/// The [HermesClient], rebuilt whenever the URL **or** token changes. The
/// token is sent as `Authorization: Bearer …` on every REST call and as
/// `?token=` on the WS upgrade.
final hermesClientProvider = Provider<HermesClient>((ref) {
  final url = ref.watch(serverUrlProvider);
  final token = ref.watch(serverTokenProvider);
  return HermesClient(baseUrl: url, token: token);
});

/// Async health probe; `autoDispose` so it re-runs cleanly on reconnect.
final healthFutureProvider =
    FutureProvider.autoDispose<HealthResponse>((ref) async {
  final client = ref.watch(hermesClientProvider);
  return client.checkHealth();
});
