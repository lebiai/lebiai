import 'dart:async';
import 'dart:convert';

import 'package:dio/dio.dart';
import 'package:flutter/foundation.dart';
import 'package:web_socket_channel/web_socket_channel.dart';

import 'package:hermes_app/data/models/chat_stream_event.dart';
import 'package:hermes_app/data/models/health.dart';
import 'package:hermes_app/data/models/session.dart';
import 'package:hermes_app/data/models/settings_models.dart';

/// HTTP/WebSocket client for the `hermes-server` backend.
class HermesClient {
  HermesClient({required String baseUrl, String? token})
      : _dio = Dio(
          BaseOptions(
            baseUrl: baseUrl,
            connectTimeout: const Duration(seconds: 5),
            receiveTimeout: const Duration(seconds: 10),
            responseType: ResponseType.json,
            headers: (token == null || token.isEmpty)
                ? null
                : {'Authorization': 'Bearer $token'},
          ),
        ),
        _token = token;

  final Dio _dio;
  final String? _token;

  /// Liveness probe → `GET /api/v1/health`. Throws on failure (never null).
  Future<HealthResponse> checkHealth() async {
    try {
      final response = await _dio.get<dynamic>('/api/v1/health');
      if (response.statusCode != 200) {
        throw Exception('health check failed: HTTP ${response.statusCode}');
      }
      final data = response.data as Map<String, dynamic>;
      return HealthResponse.fromJson(data);
    } on DioException catch (e) {
      throw Exception('无法连接服务器: ${e.message}');
    }
  }

  /// Open the chat WebSocket. Upgrades the base `http(s)://` → `ws(s)://`.
  ///
  /// Connection errors surface on [HermesChatConnection.events] (the channel
  /// connects asynchronously); this returns once the channel is created.
  HermesChatConnection connectChat() {
    final wsBase = _dio.options.baseUrl.replaceFirst(RegExp(r'^http'), 'ws');
    final token = _token;
    final auth = (token == null || token.isEmpty)
        ? ''
        : '?token=${Uri.encodeComponent(token)}';
    final uri = Uri.parse('$wsBase/api/v1/chat$auth');
    final channel = WebSocketChannel.connect(uri);
    final conn = HermesChatConnection._(channel);
    // Decode each frame in isolation: one malformed/non-JSON frame (e.g. a
    // stale server still sending a text placeholder) must not kill the whole
    // stream — skip it and keep listening for the next valid event.
    channel.stream.listen(
      (raw) {
        if (raw is! String) return;
        Map<String, dynamic> json;
        try {
          json = jsonDecode(raw) as Map<String, dynamic>;
        } on Object catch (e) {
          if (kDebugMode) debugPrint('hermes: ignoring non-JSON WS frame: $e');
          return;
        }
        try {
          conn._controller.add(ChatStreamEvent.fromJson(json));
        } on Object catch (e) {
          if (kDebugMode) debugPrint('hermes: bad event shape: $e');
        }
      },
      onError: (Object e) => conn._controller.addError(e),
      onDone: () => conn._controller.close(),
    );
    return conn;
  }

  // ----- session REST -------------------------------------------------------

  Future<List<SessionSummary>> listSessions() async {
    final r = await _dio.get<dynamic>('/api/v1/sessions');
    final list = (r.data as List).cast<Map<String, dynamic>>();
    return list.map(SessionSummary.fromJson).toList();
  }

  Future<SessionSummary> newSession() async {
    final r = await _dio.post<dynamic>('/api/v1/sessions');
    return SessionSummary.fromJson(r.data as Map<String, dynamic>);
  }

  Future<LoadedSession> loadSession(String path) async {
    final r = await _dio.get<dynamic>(
      '/api/v1/sessions/load',
      queryParameters: {'path': path},
    );
    return LoadedSession.fromJson(r.data as Map<String, dynamic>);
  }

  Future<void> deleteSession(String path) async {
    await _dio.delete<dynamic>(
      '/api/v1/sessions',
      queryParameters: {'path': path},
    );
  }

  // ----- skills REST --------------------------------------------------------

  Future<List<SkillItem>> listSkills() async {
    final r = await _dio.get<dynamic>('/api/v1/skills');
    return (r.data as List)
        .cast<Map<String, dynamic>>()
        .map(SkillItem.fromJson)
        .toList();
  }

  Future<void> saveSkill({
    required String name,
    required String description,
    required List<String> triggers,
    required String body,
    String scope = 'User',
  }) async {
    await _dio.post<dynamic>(
      '/api/v1/skills',
      data: {
        'name': name,
        'description': description,
        'triggers': triggers,
        'body': body,
        'scope': scope,
      },
    );
  }

  Future<void> deleteSkill(String name, {String scope = 'User'}) async {
    await _dio.delete<dynamic>(
      '/api/v1/skills/$name',
      queryParameters: {'scope': scope},
    );
  }

  // ----- memory REST --------------------------------------------------------

  Future<List<MemoryItem>> listMemories() async {
    final r = await _dio.get<dynamic>('/api/v1/memories');
    return (r.data as List)
        .cast<Map<String, dynamic>>()
        .map(MemoryItem.fromJson)
        .toList();
  }

  Future<MemoryItem> createMemory({
    required String body,
    List<String> tags = const [],
    String scope = 'User',
    String? zone,
    bool pinned = false,
  }) async {
    final r = await _dio.post<dynamic>(
      '/api/v1/memories',
      data: {
        'body': body,
        'tags': tags,
        'scope': scope,
        if (zone != null) 'zone': zone,
        'pinned': pinned,
      },
    );
    return MemoryItem.fromJson(r.data as Map<String, dynamic>);
  }

  Future<void> deleteMemory(String id, {String scope = 'User'}) async {
    await _dio.delete<dynamic>(
      '/api/v1/memories/$id',
      queryParameters: {'scope': scope},
    );
  }

  Future<void> togglePinMemory(String id) async {
    await _dio.post<dynamic>('/api/v1/memories/$id/toggle-pin');
  }

  // ----- config REST --------------------------------------------------------

  Future<ConfigView> getConfig() async {
    final r = await _dio.get<dynamic>('/api/v1/config');
    return ConfigView.fromJson(r.data as Map<String, dynamic>);
  }

  /// Partial update; only non-null fields in [update] are written server-side.
  Future<void> updateConfig(Map<String, dynamic> update) async {
    await _dio.put<dynamic>('/api/v1/config', data: update);
  }
}

/// Live chat WebSocket session. Push upstream frames via [send]/[cancel]/
/// [respondConfirm]; consume downstream [ChatStreamEvent]s via [events].
class HermesChatConnection {
  HermesChatConnection._(this._channel);

  final WebSocketChannel _channel;
  final StreamController<ChatStreamEvent> _controller =
      StreamController<ChatStreamEvent>.broadcast();

  Stream<ChatStreamEvent> get events => _controller.stream;

  void send(
    String sessionId,
    String content, {
    List<Map<String, String>> attachments = const [],
  }) {
    _channel.sink.add(jsonEncode({
      'type': 'send',
      'sessionId': sessionId,
      'content': content,
      if (attachments.isNotEmpty) 'attachments': attachments,
    }));
  }

  void cancel(String sessionId) {
    _channel.sink.add(jsonEncode({'type': 'cancel', 'sessionId': sessionId}));
  }

  void respondConfirm(
    String id,
    String action, {
    String? toolName,
    String? reason,
  }) {
    _channel.sink.add(jsonEncode({
      'type': 'confirm',
      'id': id,
      'action': action,
      if (toolName != null) 'toolName': toolName,
      if (reason != null) 'reason': reason,
    }));
  }

  Future<void> close() async {
    await _channel.sink.close();
    await _controller.close();
  }
}
