import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:hermes_app/data/models/chat_stream_event.dart';
import 'package:hermes_app/data/models/session.dart';
import 'package:hermes_app/data/services/hermes_client.dart';
import 'package:hermes_app/ui/features/connection/view_models/connection_providers.dart';

// ===== State models =========================================================

@immutable
class UsageStats {
  const UsageStats(this.inputTokens, this.outputTokens);
  final int inputTokens;
  final int outputTokens;
}

@immutable
class PendingConfirm {
  const PendingConfirm({
    required this.id,
    required this.toolName,
    required this.summary,
  });
  final String id;
  final String toolName;
  final String summary;
}

enum ToolCallStatus { calling, result }

/// One image attachment being sent with a message (base64, no `data:` prefix).
@immutable
class Attachment {
  const Attachment({required this.mediaType, required this.data});
  final String mediaType;
  final String data;
}

@immutable
class ToolCall {
  const ToolCall({
    required this.id,
    required this.name,
    required this.status,
    this.summary = '',
    this.input,
    this.result,
    this.isError,
  });
  final String id;
  final String name;
  final ToolCallStatus status;
  final String summary;
  final Object? input;
  final String? result;
  final bool? isError;

  ToolCall copyWith({
    String? name,
    ToolCallStatus? status,
    String? summary,
    Object? input,
    String? result,
    bool? isError,
  }) =>
      ToolCall(
        id: id,
        name: name ?? this.name,
        status: status ?? this.status,
        summary: summary ?? this.summary,
        input: input ?? this.input,
        result: result ?? this.result,
        isError: isError ?? this.isError,
      );
}

sealed class ChatMessage {
  const ChatMessage();
}

class UserMessage extends ChatMessage {
  const UserMessage({required this.text, this.images = const []});
  final String text;
  final List<String> images; // base64 (no prefix) for preview
}

class AssistantMessage extends ChatMessage {
  const AssistantMessage({
    required this.text,
    required this.thinking,
    required this.toolCalls,
    required this.streaming,
  });
  final String text;
  final String thinking;
  final List<ToolCall> toolCalls;
  final bool streaming;

  AssistantMessage copyWith({
    String? text,
    String? thinking,
    List<ToolCall>? toolCalls,
    bool? streaming,
  }) =>
      AssistantMessage(
        text: text ?? this.text,
        thinking: thinking ?? this.thinking,
        toolCalls: toolCalls ?? this.toolCalls,
        streaming: streaming ?? this.streaming,
      );
}

class _Sentinel {
  const _Sentinel();
}

const _unset = _Sentinel();

@immutable
class ChatState {
  const ChatState({
    required this.messages,
    required this.isRunning,
    required this.usage,
    this.pendingConfirm,
    this.error,
    this.sessionId,
    this.sessionPath,
  });

  final List<ChatMessage> messages;
  final bool isRunning;
  final UsageStats usage;
  final PendingConfirm? pendingConfirm;
  final String? error;
  final String? sessionId;
  final String? sessionPath;

  factory ChatState.initial() =>
      const ChatState(messages: [], isRunning: false, usage: UsageStats(0, 0));

  /// Nullable fields use a sentinel so passing `null` clears them.
  ChatState copyWith({
    List<ChatMessage>? messages,
    bool? isRunning,
    UsageStats? usage,
    Object? pendingConfirm = _unset,
    Object? error = _unset,
    Object? sessionId = _unset,
    Object? sessionPath = _unset,
  }) =>
      ChatState(
        messages: messages ?? this.messages,
        isRunning: isRunning ?? this.isRunning,
        usage: usage ?? this.usage,
        pendingConfirm: identical(pendingConfirm, _unset)
            ? this.pendingConfirm
            : pendingConfirm as PendingConfirm?,
        error: identical(error, _unset) ? this.error : error as String?,
        sessionId:
            identical(sessionId, _unset) ? this.sessionId : sessionId as String?,
        sessionPath: identical(sessionPath, _unset)
            ? this.sessionPath
            : sessionPath as String?,
      );
}

// ===== Notifier =============================================================

/// Drives the chat over the WS connection and manages the active session.
class ChatNotifier extends Notifier<ChatState> {
  HermesChatConnection? _connection;

  @override
  ChatState build() {
    // watch the client so changing the server URL (in the drawer) rebuilds
    // this notifier → drops the old WS, opens a fresh one.
    try {
      _connection = ref.watch(hermesClientProvider).connectChat();
      _connection!.events.listen(
        _onEvent,
        onError: (Object e) => state = state.copyWith(
          isRunning: false,
          error: '连接错误: $e',
        ),
      );
    } on Object catch (e) {
      return ChatState.initial().copyWith(error: '无法打开连接: $e');
    }
    ref.onDispose(() => _connection?.close());
    // Open a fresh session as soon as we connect.
    Future(() => newChat());
    return ChatState.initial();
  }

  /// Create a new session and reset the conversation view.
  Future<void> newChat() async {
    state = ChatState.initial();
    try {
      final s = await ref.read(hermesClientProvider).newSession();
      state = state.copyWith(sessionId: s.id, sessionPath: s.path);
    } on Object catch (e) {
      state = state.copyWith(error: '新建会话失败: $e');
    }
  }

  /// Load a past session: re-attach its writer server-side, replay messages.
  Future<void> loadHistory(String path) async {
    try {
      final loaded = await ref.read(hermesClientProvider).loadSession(path);
      state = ChatState(
        messages: _convertHistory(loaded),
        isRunning: false,
        usage: UsageStats(loaded.inputTokens, loaded.outputTokens),
        sessionId: loaded.id,
        sessionPath: path,
      );
    } on Object catch (e) {
      state = state.copyWith(error: '加载会话失败: $e');
    }
  }

  void send(String content, {List<Attachment> attachments = const []}) {
    final conn = _connection;
    final sid = state.sessionId;
    if (conn == null ||
        sid == null ||
        (content.trim().isEmpty && attachments.isEmpty) ||
        state.isRunning) {
      return;
    }
    final user = UserMessage(
      text: content,
      images: attachments.map((a) => a.data).toList(),
    );
    final assistant = AssistantMessage(
      text: '',
      thinking: '',
      toolCalls: const [],
      streaming: true,
    );
    state = state.copyWith(
      messages: [...state.messages, user, assistant],
      isRunning: true,
      error: null,
    );
    conn.send(
      sid,
      content,
      attachments: attachments
          .map((a) => {'mediaType': a.mediaType, 'data': a.data})
          .toList(),
    );
  }

  void cancel() => _connection?.cancel(state.sessionId ?? '');

  /// Clear the transient error banner.
  void clearError() {
    if (state.error != null) state = state.copyWith(error: null);
  }

  void respondConfirm(String action, {String? toolName, String? reason}) {
    final pc = state.pendingConfirm;
    if (pc == null) return;
    _connection?.respondConfirm(
      pc.id,
      action,
      toolName: toolName,
      reason: reason,
    );
    state = state.copyWith(pendingConfirm: null);
  }

  // ----- history conversion -------------------------------------------------

  List<ChatMessage> _convertHistory(LoadedSession loaded) {
    final result = <ChatMessage>[];
    for (final sm in loaded.messages) {
      if (sm.role == 'user') {
        final text = sm.content
            .whereType<SessionText>()
            .map((c) => c.text)
            .join();
        final images = sm.content
            .whereType<SessionImage>()
            .map((c) => c.source.data)
            .toList();
        result.add(UserMessage(text: text, images: images));
      } else {
        final text = sm.content
            .whereType<SessionText>()
            .map((c) => c.text)
            .join('\n');
        final thinking = sm.content
            .whereType<SessionThinking>()
            .map((c) => c.thinking)
            .join('\n');
        final uses = sm.content.whereType<SessionToolUse>().toList();
        final results = sm.content.whereType<SessionToolResult>().toList();
        final calls = uses.map((u) {
          SessionToolResult? r;
          for (final x in results) {
            if (x.toolUseId == u.id) {
              r = x;
              break;
            }
          }
          return ToolCall(
            id: u.id,
            name: u.name,
            summary: '',
            input: u.input,
            result: r?.content,
            isError: r?.isError,
            status: r == null
                ? ToolCallStatus.calling
                : ToolCallStatus.result,
          );
        }).toList();
        result.add(AssistantMessage(
          text: text,
          thinking: thinking,
          toolCalls: calls,
          streaming: false,
        ));
      }
    }
    return result;
  }

  // ----- event dispatch ----------------------------------------------------

  void _onEvent(ChatStreamEvent event) {
    switch (event) {
      case TextDelta(:final text):
        _updateAssistant((a) => a.copyWith(text: a.text + text));
      case ThinkingDelta(:final text):
        _updateAssistant((a) => a.copyWith(thinking: a.thinking + text));
      case ToolUseStart(:final id, :final name):
        _addToolCall(id, name);
      case ToolExecStart(:final id, :final name, :final summary, :final input):
        _updateToolCall(id, name: name, summary: summary, input: input);
      case ToolUseResult(:final id, :final content, :final isError):
        _finishToolCall(id, content, isError);
      case ConfirmRequired(:final id, :final toolName, :final summary):
        state = state.copyWith(
          pendingConfirm: PendingConfirm(id: id, toolName: toolName, summary: summary),
        );
      case UsageUpdate(:final inputTokens, :final outputTokens):
        state = state.copyWith(usage: UsageStats(inputTokens, outputTokens));
      case ErrorEvent(:final message):
        state = state.copyWith(isRunning: false, error: message);
      case Done():
        _finishTurn();
      case SkillCandidateProposed():
      case UnknownEvent():
        break;
    }
  }

  void _finishTurn() {
    _updateAssistant((a) => a.copyWith(streaming: false));
    state = state.copyWith(isRunning: false);
  }

  void _updateAssistant(AssistantMessage Function(AssistantMessage) fn) {
    final msgs = List<ChatMessage>.from(state.messages);
    for (var i = msgs.length - 1; i >= 0; i--) {
      final m = msgs[i];
      if (m is AssistantMessage) {
        msgs[i] = fn(m);
        state = state.copyWith(messages: msgs);
        return;
      }
    }
  }

  void _addToolCall(String id, String name) {
    _updateAssistant((a) {
      if (a.toolCalls.any((c) => c.id == id)) return a;
      return a.copyWith(toolCalls: [
        ...a.toolCalls,
        ToolCall(id: id, name: name, status: ToolCallStatus.calling),
      ]);
    });
  }

  void _updateToolCall(
    String id, {
    String? name,
    String? summary,
    Object? input,
  }) {
    _updateAssistant((a) => a.copyWith(
          toolCalls: a.toolCalls
              .map((c) => c.id == id
                  ? c.copyWith(
                      name: name ?? c.name,
                      summary: summary ?? c.summary,
                      input: input ?? c.input,
                    )
                  : c)
              .toList(),
        ));
  }

  void _finishToolCall(String id, String result, bool isError) {
    _updateAssistant((a) => a.copyWith(
          toolCalls: a.toolCalls
              .map((c) => c.id == id
                  ? c.copyWith(
                      result: result,
                      isError: isError,
                      status: ToolCallStatus.result,
                    )
                  : c)
              .toList(),
        ));
  }
}

final chatStateProvider =
    NotifierProvider<ChatNotifier, ChatState>(ChatNotifier.new);
