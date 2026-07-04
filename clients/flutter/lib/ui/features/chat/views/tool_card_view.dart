import 'dart:convert';

import 'package:flutter/material.dart';

import 'package:hermes_app/ui/features/chat/view_models/chat_providers.dart';
import 'package:hermes_app/ui/theme/app_dimensions.dart';

/// One tool-call card. Status state machine:
/// `calling` (spinner) → `result` (✓ green / ✗ red). Tap to expand the full
/// input JSON and result text.
class ToolCardView extends StatefulWidget {
  const ToolCardView({super.key, required this.call});

  final ToolCall call;

  @override
  State<ToolCardView> createState() => _ToolCardViewState();
}

class _ToolCardViewState extends State<ToolCardView> {
  bool _expanded = false;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final call = widget.call;
    final isResult = call.status == ToolCallStatus.result;
    final isErr = call.isError == true;
    final accent = isErr
        ? scheme.error
        : isResult
            ? const Color(0xFF22C55E)
            : scheme.primary;

    final subtitle = call.summary.isEmpty
        ? (isResult ? '完成' : '调用中…')
        : call.summary;

    return Container(
      decoration: BoxDecoration(
        color: scheme.surfaceContainerHighest.withValues(alpha: 0.6),
        borderRadius: BorderRadius.circular(HermesRadius.md),
        border: Border.all(color: scheme.outlineVariant),
      ),
      child: Material(
        color: Colors.transparent,
        child: InkWell(
          borderRadius: BorderRadius.circular(HermesRadius.md),
          onTap: () => setState(() => _expanded = !_expanded),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Padding(
                padding: const EdgeInsets.symmetric(
                    horizontal: HermesSpacing.md, vertical: HermesSpacing.sm + 2),
                child: Row(
                  children: [
                    _StatusGlyph(status: call.status, isError: isErr, color: accent),
                    const SizedBox(width: HermesSpacing.sm + 2),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          Row(
                            children: [
                              Flexible(
                                child: Text(
                                  call.name,
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                  style: const TextStyle(
                                    fontWeight: FontWeight.w600,
                                    fontSize: 13.5,
                                  ),
                                ),
                              ),
                              const SizedBox(width: 6),
                              Icon(
                                Icons.terminal,
                                size: 12,
                                color: scheme.onSurfaceVariant.withValues(alpha: 0.6),
                              ),
                            ],
                          ),
                          const SizedBox(height: 1),
                          Text(
                            subtitle,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: TextStyle(
                              fontSize: 12,
                              color: scheme.onSurfaceVariant,
                            ),
                          ),
                        ],
                      ),
                    ),
                    AnimatedRotation(
                      turns: _expanded ? 0.5 : 0,
                      duration: const Duration(milliseconds: 150),
                      child: Icon(Icons.keyboard_arrow_down,
                          size: 18, color: scheme.onSurfaceVariant),
                    ),
                  ],
                ),
              ),
              AnimatedSize(
                duration: const Duration(milliseconds: 160),
                curve: Curves.easeInOut,
                alignment: Alignment.topCenter,
                child: _expanded
                    ? _Detail(call: call, isError: isErr)
                    : const SizedBox(width: double.infinity, height: 0),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _StatusGlyph extends StatelessWidget {
  const _StatusGlyph({
    required this.status,
    required this.isError,
    required this.color,
  });

  final ToolCallStatus status;
  final bool isError;
  final Color color;

  @override
  Widget build(BuildContext context) {
    final isResult = status == ToolCallStatus.result;
    return SizedBox(
      width: 18,
      height: 18,
      child: isResult
          ? Icon(
              isError ? Icons.close : Icons.check,
              size: 18,
              color: color,
            )
          : CircularProgressIndicator(
              strokeWidth: 2,
              valueColor: AlwaysStoppedAnimation(color),
            ),
    );
  }
}

class _Detail extends StatelessWidget {
  const _Detail({required this.call, required this.isError});
  final ToolCall call;
  final bool isError;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.fromLTRB(
          HermesSpacing.md, 0, HermesSpacing.md, HermesSpacing.md),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          if (call.input != null) ...[
            _SectionLabel(label: '参数'),
            _CodeBox(text: _prettyJson(call.input)),
            if (call.result != null) const SizedBox(height: HermesSpacing.sm),
          ],
          if (call.result != null) ...[
            _SectionLabel(label: isError ? '错误' : '结果', isError: isError),
            _CodeBox(text: call.result!, isError: isError),
          ],
        ],
      ),
    );
  }

  String _prettyJson(Object? v) {
    try {
      return const JsonEncoder.withIndent('  ').convert(v);
    } on Object {
      return v.toString();
    }
  }
}

class _SectionLabel extends StatelessWidget {
  const _SectionLabel({required this.label, this.isError = false});
  final String label;
  final bool isError;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Padding(
      padding: const EdgeInsets.only(bottom: 4),
      child: Text(
        label.toUpperCase(),
        style: TextStyle(
          fontSize: 10.5,
          fontWeight: FontWeight.w700,
          letterSpacing: 0.6,
          color: isError ? scheme.error : scheme.onSurfaceVariant,
        ),
      ),
    );
  }
}

class _CodeBox extends StatelessWidget {
  const _CodeBox({required this.text, this.isError = false});
  final String text;
  final bool isError;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Container(
      constraints: const BoxConstraints(maxHeight: 220),
      width: double.infinity,
      padding: const EdgeInsets.all(HermesSpacing.sm + 2),
      decoration: BoxDecoration(
        color: isError
            ? scheme.errorContainer.withValues(alpha: 0.4)
            : scheme.surfaceContainerHighest,
        borderRadius: BorderRadius.circular(HermesRadius.sm + 2),
        border: Border.all(color: scheme.outlineVariant),
      ),
      child: SingleChildScrollView(
        child: SelectableText(
          text,
          style: TextStyle(
            fontFamily: 'SF Mono',
            fontSize: 12,
            height: 1.45,
            color: isError ? scheme.error : scheme.onSurface,
          ),
        ),
      ),
    );
  }
}
