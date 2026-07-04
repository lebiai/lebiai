import 'package:flutter/material.dart';

import 'package:hermes_app/ui/features/chat/views/tool_card_view.dart';
import 'package:hermes_app/ui/features/chat/view_models/chat_providers.dart';
import 'package:hermes_app/ui/theme/app_dimensions.dart';
import 'package:hermes_app/ui/widgets/app_markdown.dart';
import 'package:hermes_app/ui/widgets/brand_mark.dart';

/// Renders one assistant turn: an avatar, a surface bubble holding optional
/// collapsed thinking, the tool-call cards produced, and the streaming
/// markdown text (with a blinking cursor while streaming).
class AssistantMessageView extends StatelessWidget {
  const AssistantMessageView({super.key, required this.message});

  final AssistantMessage message;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final waiting = message.streaming &&
        message.text.isEmpty &&
        message.thinking.isEmpty &&
        message.toolCalls.isEmpty;

    return Padding(
      padding: const EdgeInsets.symmetric(vertical: HermesSpacing.xs + 1),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Padding(
            padding: EdgeInsets.only(top: 2),
            child: BrandMark(size: 26),
          ),
          const SizedBox(width: HermesSpacing.sm + 2),
          Flexible(
            child: Container(
              padding: const EdgeInsets.symmetric(
                  horizontal: HermesSpacing.md, vertical: HermesSpacing.sm + 2),
              decoration: BoxDecoration(
                color: scheme.surface,
                border: Border.all(color: scheme.outlineVariant),
                borderRadius: const BorderRadius.only(
                  topLeft: Radius.circular(HermesRadius.bubble),
                  topRight: Radius.circular(HermesRadius.bubble),
                  bottomRight: Radius.circular(HermesRadius.bubble),
                  bottomLeft: Radius.circular(HermesRadius.sm), // tail
                ),
              ),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                mainAxisSize: MainAxisSize.min,
                children: [
                  if (message.thinking.isNotEmpty)
                    ThinkingBlock(text: message.thinking),
                  for (final c in message.toolCalls) ...[
                    ToolCardView(call: c),
                    const SizedBox(height: HermesSpacing.xs),
                  ],
                  if (message.text.isNotEmpty)
                    AppMarkdown(data: message.text)
                  else if (waiting)
                    const _ThinkingDots(),
                  if (message.streaming && message.text.isNotEmpty)
                    const _BlinkingCursor(),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class ThinkingBlock extends StatefulWidget {
  const ThinkingBlock({super.key, required this.text});
  final String text;

  @override
  State<ThinkingBlock> createState() => _ThinkingBlockState();
}

class _ThinkingBlockState extends State<ThinkingBlock> {
  bool _expanded = false;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Padding(
      padding: const EdgeInsets.only(bottom: HermesSpacing.sm),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          InkWell(
            borderRadius: BorderRadius.circular(HermesRadius.pill),
            onTap: () => setState(() => _expanded = !_expanded),
            child: Padding(
              padding: const EdgeInsets.symmetric(
                  horizontal: HermesSpacing.sm, vertical: HermesSpacing.xs),
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Icon(
                    _expanded ? Icons.expand_less : Icons.psychology_outlined,
                    size: 15,
                    color: scheme.primary,
                  ),
                  const SizedBox(width: 5),
                  Text(
                    _expanded ? '收起思考' : '查看思考过程',
                    style: Theme.of(context).textTheme.labelMedium?.copyWith(
                          color: scheme.primary,
                        ),
                  ),
                ],
              ),
            ),
          ),
          AnimatedCrossFade(
            duration: const Duration(milliseconds: 160),
            crossFadeState: _expanded
                ? CrossFadeState.showSecond
                : CrossFadeState.showFirst,
            firstChild: const SizedBox(width: double.infinity),
            secondChild: Container(
              width: double.infinity,
              margin: const EdgeInsets.only(top: HermesSpacing.xs),
              padding: const EdgeInsets.all(HermesSpacing.md),
              decoration: BoxDecoration(
                color: scheme.surfaceContainerHighest,
                borderRadius: BorderRadius.circular(HermesRadius.md),
              ),
              child: SelectableText(
                widget.text,
                style: TextStyle(
                  fontSize: 12.5,
                  height: 1.5,
                  color: scheme.onSurfaceVariant,
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _BlinkingCursor extends StatefulWidget {
  const _BlinkingCursor();

  @override
  State<_BlinkingCursor> createState() => _BlinkingCursorState();
}

class _BlinkingCursorState extends State<_BlinkingCursor>
    with SingleTickerProviderStateMixin {
  late final AnimationController _c = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 540),
  )..repeat();

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return FadeTransition(
      opacity: _c,
      child: Text('▌', style: TextStyle(color: scheme.primary, fontSize: 14)),
    );
  }
}

/// Three pulsing dots while the model is "thinking" before any text streams.
class _ThinkingDots extends StatefulWidget {
  const _ThinkingDots();

  @override
  State<_ThinkingDots> createState() => _ThinkingDotsState();
}

class _ThinkingDotsState extends State<_ThinkingDots>
    with SingleTickerProviderStateMixin {
  late final AnimationController _c = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 1200),
  )..repeat();

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: List.generate(3, (i) {
          return Padding(
            padding: const EdgeInsets.symmetric(horizontal: 2),
            child: ScaleTransition(
              scale: _DelayTween(begin: 0.5, end: 1.0, delay: i * 0.2)
                  .animate(CurvedAnimation(parent: _c, curve: Curves.easeInOut)),
              child: Container(
                width: 7,
                height: 7,
                decoration: BoxDecoration(
                  color: scheme.primary.withValues(alpha: 0.6),
                  shape: BoxShape.circle,
                ),
              ),
            ),
          );
        }),
      ),
    );
  }
}

class _DelayTween extends Tween<double> {
  _DelayTween({required super.begin, required super.end, required this.delay});
  final double delay;

  @override
  double lerp(double t) {
    return super.lerp(((t + delay) % 1.0));
  }
}
