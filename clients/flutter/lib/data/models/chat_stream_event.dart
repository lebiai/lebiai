import 'package:flutter/foundation.dart';

/// One downstream frame on the chat WebSocket.
///
/// Wire shape (server → client): `{"event": "<kind>", "data": {...}}` with
/// camelCase fields — mirrors `hermes_server::events::ChatStreamEvent`.
/// Parsed via a factory on the `event` discriminator.
@immutable
sealed class ChatStreamEvent {
  const ChatStreamEvent();

  factory ChatStreamEvent.fromJson(Map<String, dynamic> json) {
    final event = json['event'] as String;
    final data =
        (json['data'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
    return switch (event) {
      'textDelta' => TextDelta(text: data['text'] as String),
      'thinkingDelta' => ThinkingDelta(text: data['text'] as String),
      'toolUseStart' => ToolUseStart(
          id: data['id'] as String, name: data['name'] as String),
      'toolExecStart' => ToolExecStart(
          id: data['id'] as String,
          name: data['name'] as String,
          summary: data['summary'] as String,
          input: data['input']),
      'toolUseResult' => ToolUseResult(
          id: data['id'] as String,
          content: data['content'] as String,
          isError: data['isError'] as bool),
      'confirmRequired' => ConfirmRequired(
          id: data['id'] as String,
          toolName: data['toolName'] as String,
          summary: data['summary'] as String),
      'usageUpdate' => UsageUpdate(
          inputTokens: (data['inputTokens'] as num).toInt(),
          outputTokens: (data['outputTokens'] as num).toInt()),
      'error' => ErrorEvent(message: data['message'] as String),
      'skillCandidateProposed' => SkillCandidateProposed(
          name: data['name'] as String,
          description: data['description'] as String,
          body: data['body'] as String,
          triggers: (data['triggers'] as List).cast<String>()),
      'microReflection' => MicroReflection(
          summary: data['summary'] as String,
          memoryCount: (data['memoryCount'] as num).toInt(),
          skillCount: (data['skillCount'] as num).toInt(),
          autoAccepted: (data['autoAccepted'] as num).toInt()),
      'done' => const Done(),
      _ => UnknownEvent(type: event),
    };
  }
}

class TextDelta extends ChatStreamEvent {
  const TextDelta({required this.text});
  final String text;
}

class ThinkingDelta extends ChatStreamEvent {
  const ThinkingDelta({required this.text});
  final String text;
}

class ToolUseStart extends ChatStreamEvent {
  const ToolUseStart({required this.id, required this.name});
  final String id;
  final String name;
}

class ToolExecStart extends ChatStreamEvent {
  const ToolExecStart({
    required this.id,
    required this.name,
    required this.summary,
    required this.input,
  });
  final String id;
  final String name;
  final String summary;
  /// Full tool-call arguments (raw JSON value) — present here so the UI can
  /// render an expandable parameters view (the Tauri GUI drops this).
  final Object? input;
}

class ToolUseResult extends ChatStreamEvent {
  const ToolUseResult({
    required this.id,
    required this.content,
    required this.isError,
  });
  final String id;
  final String content;
  final bool isError;
}

class ConfirmRequired extends ChatStreamEvent {
  const ConfirmRequired({
    required this.id,
    required this.toolName,
    required this.summary,
  });
  final String id;
  final String toolName;
  final String summary;
}

class UsageUpdate extends ChatStreamEvent {
  const UsageUpdate({required this.inputTokens, required this.outputTokens});
  final int inputTokens;
  final int outputTokens;
}

class ErrorEvent extends ChatStreamEvent {
  const ErrorEvent({required this.message});
  final String message;
}

class SkillCandidateProposed extends ChatStreamEvent {
  const SkillCandidateProposed({
    required this.name,
    required this.description,
    required this.body,
    required this.triggers,
  });
  final String name;
  final String description;
  final String body;
  final List<String> triggers;
}

class MicroReflection extends ChatStreamEvent {
  const MicroReflection({
    required this.summary,
    required this.memoryCount,
    required this.skillCount,
    required this.autoAccepted,
  });
  final String summary;
  final int memoryCount;
  final int skillCount;
  /// Memories auto-written by this pass (no human review needed).
  final int autoAccepted;
}

class Done extends ChatStreamEvent {
  const Done();
}

class UnknownEvent extends ChatStreamEvent {
  const UnknownEvent({required this.type});
  final String type;
}
