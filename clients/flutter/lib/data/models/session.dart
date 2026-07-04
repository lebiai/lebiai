import 'package:flutter/foundation.dart';

/// Mirrors `hermes_server::routes::sessions` DTOs (camelCase).
@immutable
class SessionSummary {
  const SessionSummary({
    required this.id,
    required this.title,
    required this.createdAt,
    required this.path,
  });
  final String id;
  final String title;
  final String createdAt;
  final String path;

  factory SessionSummary.fromJson(Map<String, dynamic> j) => SessionSummary(
        id: j['id'] as String,
        title: j['title'] as String,
        createdAt: j['createdAt'] as String,
        path: j['path'] as String,
      );
}

@immutable
class LoadedSession {
  const LoadedSession({
    required this.id,
    required this.messages,
    required this.inputTokens,
    required this.outputTokens,
  });
  final String id;
  final List<SessionMessage> messages;
  final int inputTokens;
  final int outputTokens;

  factory LoadedSession.fromJson(Map<String, dynamic> j) => LoadedSession(
        id: j['id'] as String,
        messages: (j['messages'] as List)
            .cast<Map<String, dynamic>>()
            .map(SessionMessage.fromJson)
            .toList(),
        inputTokens: (j['inputTokens'] as num).toInt(),
        outputTokens: (j['outputTokens'] as num).toInt(),
      );
}

@immutable
class SessionMessage {
  const SessionMessage({required this.role, required this.content});
  final String role;
  final List<SessionContentBlock> content;

  static SessionMessage fromJson(Map<String, dynamic> j) => SessionMessage(
        role: j['role'] as String,
        content: (j['content'] as List)
            .cast<Map<String, dynamic>>()
            .map(SessionContentBlock.fromJson)
            .toList(),
      );
}

sealed class SessionContentBlock {
  const SessionContentBlock();

  factory SessionContentBlock.fromJson(Map<String, dynamic> json) {
    return switch (json['type'] as String) {
      'text' => SessionText(text: json['text'] as String),
      'thinking' => SessionThinking(thinking: json['thinking'] as String),
      'toolUse' => SessionToolUse(
          id: json['id'] as String, name: json['name'] as String, input: json['input']),
      'toolResult' => SessionToolResult(
          toolUseId: json['toolUseId'] as String,
          content: json['content'] as String,
          isError: json['isError'] as bool),
      'image' => SessionImage(
          source: SessionImageSource.fromJson(
              json['source'] as Map<String, dynamic>)),
      _ => const SessionUnknown(),
    };
  }
}

class SessionImage extends SessionContentBlock {
  const SessionImage({required this.source});
  final SessionImageSource source;
}

@immutable
class SessionImageSource {
  const SessionImageSource({
    required this.kind,
    required this.mediaType,
    required this.data,
  });
  final String kind; // "base64"
  final String mediaType;
  final String data;

  factory SessionImageSource.fromJson(Map<String, dynamic> j) =>
      SessionImageSource(
        kind: j['type'] as String,
        mediaType: j['mediaType'] as String,
        data: j['data'] as String,
      );
}

class SessionText extends SessionContentBlock {
  const SessionText({required this.text});
  final String text;
}

class SessionThinking extends SessionContentBlock {
  const SessionThinking({required this.thinking});
  final String thinking;
}

class SessionToolUse extends SessionContentBlock {
  const SessionToolUse({required this.id, required this.name, required this.input});
  final String id;
  final String name;
  final Object? input;
}

class SessionToolResult extends SessionContentBlock {
  const SessionToolResult({
    required this.toolUseId,
    required this.content,
    required this.isError,
  });
  final String toolUseId;
  final String content;
  final bool isError;
}

class SessionUnknown extends SessionContentBlock {
  const SessionUnknown();
}
