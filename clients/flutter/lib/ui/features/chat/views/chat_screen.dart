import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:image_picker/image_picker.dart';
import 'package:speech_to_text/speech_to_text.dart';

import 'package:hermes_app/ui/features/chat/view_models/chat_providers.dart';
import 'package:hermes_app/ui/features/chat/views/assistant_message_view.dart';
import 'package:hermes_app/ui/features/chat/views/chat_drawer.dart';
import 'package:hermes_app/ui/features/chat/view_models/sessions_providers.dart';
import 'package:hermes_app/ui/features/connection/view_models/connection_providers.dart';
import 'package:hermes_app/ui/features/settings/views/settings_screen.dart';
import 'package:hermes_app/ui/theme/app_dimensions.dart';
import 'package:hermes_app/ui/widgets/brand_mark.dart';
import 'package:hermes_app/ui/widgets/status_dot.dart';

/// Conversation surface: streaming message thread (user bubbles + assistant
/// markdown / tool cards / thinking), an empty-state hero with prompt
/// suggestions, and a rounded composer with attachment / voice / send.
class ChatScreen extends ConsumerStatefulWidget {
  const ChatScreen({super.key});

  @override
  ConsumerState<ChatScreen> createState() => _ChatScreenState();
}

class _ChatScreenState extends ConsumerState<ChatScreen> {
  final _input = TextEditingController();
  final _scroll = ScrollController();
  final SpeechToText _speech = SpeechToText();
  final FocusNode _inputFocus = FocusNode();
  int _lastLen = 0;
  bool _speechAvailable = false;
  bool _listening = false;
  bool _composing = false; // text in the field?
  final List<Attachment> _pending = [];

  static const _suggestions = [
    '帮我解释一段报错',
    '总结这个项目',
    '写一个 Rust 函数',
    '头脑风暴几个点子',
  ];

  @override
  void initState() {
    super.initState();
    _input.addListener(() {
      final c = _input.text.trim().isNotEmpty;
      if (c != _composing) setState(() => _composing = c);
    });
    _speech.initialize().then((ok) {
      if (mounted) setState(() => _speechAvailable = ok);
    });
  }

  @override
  void dispose() {
    _input.dispose();
    _scroll.dispose();
    _inputFocus.dispose();
    super.dispose();
  }

  void _scrollToBottom() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (_scroll.hasClients) {
        _scroll.animateTo(
          _scroll.position.maxScrollExtent,
          duration: const Duration(milliseconds: 140),
          curve: Curves.easeOut,
        );
      }
    });
  }

  void _send([String? override]) {
    final text = (override ?? _input.text).trim();
    if (text.isEmpty && _pending.isEmpty) return;
    ref.read(chatStateProvider.notifier).send(text, attachments: _pending);
    _input.clear();
    setState(() {
      _pending.clear();
      _composing = false;
    });
  }

  void removePending(Attachment a) => setState(() => _pending.remove(a));

  Future<void> _pickImage() async {
    final picker = ImagePicker();
    final x = await picker.pickImage(source: ImageSource.gallery, imageQuality: 85);
    if (x == null) return;
    final bytes = await x.readAsBytes();
    if (!mounted) return;
    setState(() {
      _pending.add(Attachment(
        mediaType: x.mimeType ?? 'image/png',
        data: base64Encode(bytes),
      ));
    });
    _inputFocus.requestFocus();
  }

  Future<void> _toggleMic() async {
    if (!_speechAvailable) return;
    if (_listening) {
      await _speech.stop();
      if (mounted) setState(() => _listening = false);
      return;
    }
    setState(() => _listening = true);
    await _speech.listen(
      onResult: (r) {
        if (r.finalResult && r.recognizedWords.isNotEmpty) {
          setState(() {
            _input.text = '${_input.text}${r.recognizedWords}';
            _input.selection = TextSelection.fromPosition(
              TextPosition(offset: _input.text.length),
            );
          });
        }
      },
    );
    if (mounted) setState(() => _listening = false);
  }

  Future<void> _showConfirm(PendingConfirm pc) async {
    final action = await showDialog<String>(
      context: context,
      barrierDismissible: false,
      builder: (ctx) => _ConfirmDialog(toolName: pc.toolName, summary: pc.summary),
    );
    if (action == null || !mounted) return;
    String? reason;
    if (action == 'deny') {
      reason = await _askReason();
      if (!mounted) return;
    }
    ref.read(chatStateProvider.notifier).respondConfirm(
          action,
          toolName: pc.toolName,
          reason: reason,
        );
  }

  Future<String?> _askReason() {
    String reason = '';
    return showDialog<String>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('拒绝原因（可选）'),
        content: TextField(
          autofocus: true,
          onChanged: (v) => reason = v,
          decoration: const InputDecoration(hintText: '为什么拒绝？会反馈给模型'),
        ),
        actions: [
          FilledButton(
            onPressed: () => Navigator.pop(ctx, reason),
            child: const Text('确认拒绝'),
          ),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final state = ref.watch(chatStateProvider);
    final health = ref.watch(healthFutureProvider);
    final scheme = Theme.of(context).colorScheme;

    ref.listen(chatStateProvider, (prev, next) {
      final prevId = prev?.pendingConfirm?.id;
      if (next.pendingConfirm != null && next.pendingConfirm!.id != prevId) {
        _showConfirm(next.pendingConfirm!);
      }
      if (next.messages.length != _lastLen) {
        _lastLen = next.messages.length;
        _scrollToBottom();
      }
    });

    final canSend = _composing || _pending.isNotEmpty;

    return Scaffold(
      drawer: const Drawer(width: 320, child: ChatDrawer()),
      appBar: AppBar(
        title: Row(
          children: [
            const BrandMark(size: 30),
            const SizedBox(width: 10),
            Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisSize: MainAxisSize.min,
              children: [
                const Text('lebi-AI'),
                Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    StatusDot(
                      size: 6,
                      status: health.when(
                        data: (_) => ConnStatus.online,
                        loading: () => ConnStatus.checking,
                        error: (_, __) => ConnStatus.offline,
                      ),
                    ),
                    const SizedBox(width: 5),
                    Text(
                      health.when(
                        data: (_) => '在线',
                        loading: () => '连接中',
                        error: (_, __) => '离线',
                      ),
                      style: Theme.of(context).textTheme.labelSmall?.copyWith(
                            color: scheme.onSurfaceVariant,
                          ),
                    ),
                  ],
                ),
              ],
            ),
          ],
        ),
        actions: [
          IconButton(
            icon: const Icon(Icons.edit_square),
            tooltip: '新对话',
            onPressed: () async {
              await ref.read(chatStateProvider.notifier).newChat();
              ref.invalidate(sessionsProvider);
            },
          ),
          IconButton(
            icon: const Icon(Icons.settings_outlined),
            tooltip: '管理',
            onPressed: () => Navigator.push(
              context,
              MaterialPageRoute(builder: (_) => const SettingsScreen()),
            ),
          ),
        ],
        bottom: state.isRunning
            ? const PreferredSize(
                preferredSize: Size.fromHeight(2),
                child: LinearProgressIndicator(minHeight: 2),
              )
            : null,
      ),
      body: Column(
        children: [
          Expanded(
            child: state.messages.isEmpty
                ? _EmptyState(onSuggestion: _send)
                : ListView.builder(
                    controller: _scroll,
                    padding: const EdgeInsets.fromLTRB(
                        HermesSpacing.lg, HermesSpacing.lg, HermesSpacing.lg, HermesSpacing.sm),
                    itemCount: state.messages.length,
                    itemBuilder: (ctx, i) {
                      final msg = state.messages[i];
                      return switch (msg) {
                        UserMessage(:final text, :final images) =>
                          _UserBubble(text: text, images: images),
                        AssistantMessage() => AssistantMessageView(message: msg),
                      };
                    },
                  ),
          ),
          if (state.notice != null)
            _NoticeBanner(
              message: state.notice!,
              onDismiss: () =>
                  ref.read(chatStateProvider.notifier).clearNotice(),
            ),
          if (state.error != null)
            _ErrorBanner(
              message: state.error!,
              onDismiss: () =>
                  ref.read(chatStateProvider.notifier).clearError(),
            ),
          if (_pending.isNotEmpty) _PendingAttachments(pending: _pending),
          _Composer(
            controller: _input,
            focus: _inputFocus,
            composing: canSend,
            listening: _listening,
            speechAvailable: _speechAvailable,
            running: state.isRunning,
            usage: state.usage,
            onPickImage: _pickImage,
            onToggleMic: _toggleMic,
            onSend: () => _send(),
          ),
        ],
      ),
    );
  }
}

// ----- Empty state ---------------------------------------------------------

class _EmptyState extends StatelessWidget {
  const _EmptyState({required this.onSuggestion});
  final ValueChanged<String> onSuggestion;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Center(
      child: SingleChildScrollView(
        padding: const EdgeInsets.all(HermesSpacing.xl),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const BrandMark(size: 64),
            const SizedBox(height: HermesSpacing.lg),
            Text(
              '你好，我是乐彼AI',
              style: Theme.of(context).textTheme.headlineMedium,
            ),
            const SizedBox(height: HermesSpacing.sm),
            Text(
              '你的本地 AI 工作伙伴。选个起点，或直接输入。',
              textAlign: TextAlign.center,
              style: Theme.of(context).textTheme.bodyMedium?.copyWith(
                    color: scheme.onSurfaceVariant,
                  ),
            ),
            const SizedBox(height: HermesSpacing.xl),
            Wrap(
              alignment: WrapAlignment.center,
              spacing: HermesSpacing.sm,
              runSpacing: HermesSpacing.sm,
              children: [
                for (final s in _ChatScreenState._suggestions)
                  ActionChip(
                    label: Text(s),
                    onPressed: () => onSuggestion(s),
                    shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.circular(HermesRadius.pill),
                    ),
                  ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

// ----- User bubble ---------------------------------------------------------

class _UserBubble extends StatelessWidget {
  const _UserBubble({required this.text, required this.images});
  final String text;
  final List<String> images;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final hasText = text.isNotEmpty;
    return Align(
      alignment: Alignment.centerRight,
      child: ConstrainedBox(
        constraints: BoxConstraints(
          maxWidth: MediaQuery.of(context).size.width * 0.8,
        ),
        child: Container(
          margin: const EdgeInsets.symmetric(vertical: HermesSpacing.xs + 1),
          padding: const EdgeInsets.symmetric(
              horizontal: HermesSpacing.md, vertical: HermesSpacing.sm + 2),
          decoration: BoxDecoration(
            color: scheme.primaryContainer,
            borderRadius: const BorderRadius.only(
              topLeft: Radius.circular(HermesRadius.bubble),
              topRight: Radius.circular(HermesRadius.bubble),
              bottomLeft: Radius.circular(HermesRadius.bubble),
              bottomRight: Radius.circular(HermesRadius.sm), // tail
            ),
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.end,
            mainAxisSize: MainAxisSize.min,
            children: [
              if (hasText)
                Text(text, style: TextStyle(color: scheme.onPrimaryContainer)),
              for (final img in images) ...[
                if (hasText) const SizedBox(height: HermesSpacing.xs),
                ClipRRect(
                  borderRadius: BorderRadius.circular(HermesRadius.md),
                  child: Image.memory(base64Decode(img), width: 220),
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }
}

// ----- Pending attachments strip ------------------------------------------

class _PendingAttachments extends StatelessWidget {
  const _PendingAttachments({required this.pending});
  final List<Attachment> pending;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(
          HermesSpacing.md, HermesSpacing.sm, HermesSpacing.md, 0),
      child: SizedBox(
        height: 76,
        child: ListView.separated(
          scrollDirection: Axis.horizontal,
          itemCount: pending.length,
          separatorBuilder: (_, __) => const SizedBox(width: HermesSpacing.sm),
          itemBuilder: (ctx, i) {
            final a = pending[i];
            return _PendingTile(
              data: a.data,
              onRemove: () => ctx
                  .findAncestorStateOfType<_ChatScreenState>()
                  ?.removePending(a),
            );
          },
        ),
      ),
    );
  }
}

class _PendingTile extends StatelessWidget {
  const _PendingTile({required this.data, required this.onRemove});
  final String data;
  final VoidCallback onRemove;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Stack(
      children: [
        Container(
          decoration: BoxDecoration(
            borderRadius: BorderRadius.circular(HermesRadius.md),
            border: Border.all(color: scheme.outlineVariant),
          ),
          child: ClipRRect(
            borderRadius: BorderRadius.circular(HermesRadius.md),
            child: Image.memory(base64Decode(data),
                height: 72, width: 72, fit: BoxFit.cover),
          ),
        ),
        Positioned(
          right: -4,
          top: -4,
          child: IconButton.filled(
            icon: const Icon(Icons.close, size: 14),
            visualDensity: VisualDensity.compact,
            iconSize: 14,
            padding: EdgeInsets.zero,
            constraints: const BoxConstraints(minHeight: 22, minWidth: 22),
            style: IconButton.styleFrom(
              backgroundColor: scheme.surface,
              foregroundColor: scheme.onSurface,
            ),
            onPressed: onRemove,
          ),
        ),
      ],
    );
  }
}

// ----- Notice banner -------------------------------------------------------

class _NoticeBanner extends StatelessWidget {
  const _NoticeBanner({required this.message, required this.onDismiss});
  final String message;
  final VoidCallback onDismiss;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Container(
      margin: const EdgeInsets.fromLTRB(
          HermesSpacing.md, HermesSpacing.sm, HermesSpacing.md, 0),
      padding: const EdgeInsets.symmetric(
          horizontal: HermesSpacing.md, vertical: HermesSpacing.sm + 2),
      decoration: BoxDecoration(
        color: scheme.secondaryContainer,
        borderRadius: BorderRadius.circular(HermesRadius.md),
      ),
      child: Row(
        children: [
          Icon(Icons.auto_awesome, size: 18, color: scheme.onSecondaryContainer),
          const SizedBox(width: HermesSpacing.sm),
          Expanded(
            child: Text(
              message,
              style: TextStyle(
                color: scheme.onSecondaryContainer,
                fontSize: 13,
              ),
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
            ),
          ),
          IconButton(
            icon: const Icon(Icons.close, size: 16),
            onPressed: onDismiss,
          ),
        ],
      ),
    );
  }
}

// ----- Error banner --------------------------------------------------------

class _ErrorBanner extends StatelessWidget {
  const _ErrorBanner({required this.message, required this.onDismiss});
  final String message;
  final VoidCallback onDismiss;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Container(
      margin: const EdgeInsets.fromLTRB(
          HermesSpacing.md, HermesSpacing.sm, HermesSpacing.md, 0),
      padding: const EdgeInsets.symmetric(
          horizontal: HermesSpacing.md, vertical: HermesSpacing.sm + 2),
      decoration: BoxDecoration(
        color: scheme.errorContainer,
        borderRadius: BorderRadius.circular(HermesRadius.md),
      ),
      child: Row(
        children: [
          Icon(Icons.error_outline, size: 18, color: scheme.onErrorContainer),
          const SizedBox(width: HermesSpacing.sm),
          Expanded(
            child: Text(
              message,
              style: TextStyle(
                color: scheme.onErrorContainer,
                fontSize: 13,
              ),
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
            ),
          ),
          IconButton(
            icon: const Icon(Icons.close, size: 16),
            visualDensity: VisualDensity.compact,
            onPressed: onDismiss,
            color: scheme.onErrorContainer,
          ),
        ],
      ),
    );
  }
}

// ----- Composer ------------------------------------------------------------

class _Composer extends StatelessWidget {
  const _Composer({
    required this.controller,
    required this.focus,
    required this.composing,
    required this.listening,
    required this.speechAvailable,
    required this.running,
    required this.usage,
    required this.onPickImage,
    required this.onToggleMic,
    required this.onSend,
  });

  final TextEditingController controller;
  final FocusNode focus;
  final bool composing;
  final bool listening;
  final bool speechAvailable;
  final bool running;
  final UsageStats usage;
  final VoidCallback onPickImage;
  final VoidCallback onToggleMic;
  final VoidCallback onSend;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return SafeArea(
      top: false,
      child: Padding(
        padding: const EdgeInsets.fromLTRB(
            HermesSpacing.md, HermesSpacing.sm, HermesSpacing.md, HermesSpacing.sm),
        child: Column(
          children: [
            Row(
              crossAxisAlignment: CrossAxisAlignment.end,
              children: [
                IconButton(
                  icon: const Icon(Icons.add_photo_alternate_outlined),
                  tooltip: '添加图片',
                  onPressed: onPickImage,
                ),
                Expanded(
                  child: Container(
                    padding:
                        const EdgeInsets.symmetric(horizontal: HermesSpacing.md),
                    decoration: BoxDecoration(
                      color: scheme.surfaceContainerHighest,
                      borderRadius: BorderRadius.circular(HermesRadius.xl),
                    ),
                    child: TextField(
                      controller: controller,
                      focusNode: focus,
                      minLines: 1,
                      maxLines: 6,
                      textInputAction: TextInputAction.newline,
                      style: Theme.of(context).textTheme.bodyLarge,
                      decoration: InputDecoration(
                        border: InputBorder.none,
                        enabledBorder: InputBorder.none,
                        focusedBorder: InputBorder.none,
                        hintText: '发消息给 Hermes…',
                        isDense: true,
                        contentPadding: const EdgeInsets.symmetric(vertical: 12),
                      ),
                      onSubmitted: (_) {
                        if (composing) onSend();
                      },
                    ),
                  ),
                ),
                IconButton(
                  icon: Icon(listening ? Icons.mic : Icons.mic_none),
                  color: listening ? scheme.error : null,
                  tooltip: '语音输入',
                  onPressed: speechAvailable ? onToggleMic : null,
                ),
                const SizedBox(width: HermesSpacing.xs),
                _SendButton(enabled: composing && !running, onPressed: onSend),
              ],
            ),
            if (usage.inputTokens > 0 || usage.outputTokens > 0)
              Padding(
                padding: const EdgeInsets.only(top: 2),
                child: Text(
                  '↑ ${usage.inputTokens}  ↓ ${usage.outputTokens} tokens',
                  style: Theme.of(context).textTheme.labelSmall?.copyWith(
                        color: scheme.onSurfaceVariant.withValues(alpha: 0.7),
                      ),
                ),
              ),
          ],
        ),
      ),
    );
  }
}

class _SendButton extends StatelessWidget {
  const _SendButton({required this.enabled, required this.onPressed});
  final bool enabled;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final color = enabled ? scheme.primary : scheme.surfaceContainerHighest;
    final fg = enabled ? scheme.onPrimary : scheme.onSurfaceVariant;
    return AnimatedContainer(
      duration: const Duration(milliseconds: 150),
      width: 40,
      height: 40,
      decoration: BoxDecoration(
        color: color,
        shape: BoxShape.circle,
        boxShadow: enabled
            ? [
                BoxShadow(
                  color: scheme.primary.withValues(alpha: 0.3),
                  blurRadius: 8,
                  offset: const Offset(0, 2),
                ),
              ]
            : null,
      ),
      child: IconButton(
        icon: const Icon(Icons.arrow_upward, size: 20),
        color: fg,
        onPressed: enabled ? onPressed : null,
        padding: EdgeInsets.zero,
        style: const ButtonStyle(
          shape: WidgetStatePropertyAll(CircleBorder()),
        ),
      ),
    );
  }
}

// ----- Confirm dialog ------------------------------------------------------

class _ConfirmDialog extends StatelessWidget {
  const _ConfirmDialog({required this.toolName, required this.summary});
  final String toolName;
  final String summary;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return AlertDialog(
      icon: CircleAvatar(
        backgroundColor: scheme.errorContainer,
        radius: 22,
        child: Icon(Icons.shield_outlined, color: scheme.onErrorContainer),
      ),
      title: Text('确认执行「$toolName」'),
      content: Container(
        constraints: const BoxConstraints(maxWidth: 320),
        padding: const EdgeInsets.all(HermesSpacing.md),
        decoration: BoxDecoration(
          color: scheme.surfaceContainerHighest,
          borderRadius: BorderRadius.circular(HermesRadius.md),
        ),
        child: Text(summary, style: Theme.of(context).textTheme.bodyMedium),
      ),
      actionsAlignment: MainAxisAlignment.spaceBetween,
      actionsPadding: const EdgeInsets.fromLTRB(
          HermesSpacing.md, 0, HermesSpacing.md, HermesSpacing.md),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context, 'deny'),
          child: const Text('拒绝'),
        ),
        Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextButton(
              onPressed: () => Navigator.pop(context, 'always_allow'),
              child: const Text('总是允许'),
            ),
            FilledButton(
              onPressed: () => Navigator.pop(context, 'allow'),
              child: const Text('允许'),
            ),
          ],
        ),
      ],
    );
  }
}
