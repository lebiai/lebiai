import 'package:flutter/material.dart';
import 'package:flutter_markdown/flutter_markdown.dart';
import 'package:markdown/markdown.dart' as md;

import 'package:hermes_app/ui/theme/app_dimensions.dart';

/// Shared markdown renderer with the app's stylesheet.
///
/// One place to tune how markdown looks across the app — assistant message
/// bodies and skill bodies use this so they render consistently (headings,
/// lists, fenced code, tables, links, blockquotes).
class AppMarkdown extends StatelessWidget {
  const AppMarkdown({
    super.key,
    required this.data,
    this.selectable = true,
    this.shrinkWrap = false,
  });

  final String data;
  final bool selectable;

  /// Let the renderer size to its content (good inside scrollable cards).
  final bool shrinkWrap;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final tt = Theme.of(context).textTheme;
    final base = MarkdownStyleSheet.fromTheme(Theme.of(context));
    final mono = const TextStyle(fontFamily: 'SF Mono');
    return MarkdownBody(
      data: data,
      selectable: selectable,
      shrinkWrap: shrinkWrap,
      extensionSet: md.ExtensionSet.gitHubWeb,
      styleSheet: base.copyWith(
        p: tt.bodyLarge,
        h1: tt.headlineMedium,
        h2: tt.titleLarge?.copyWith(fontWeight: FontWeight.w700),
        h3: tt.titleMedium,
        h4: tt.titleSmall,
        h5: tt.titleSmall,
        h6: tt.labelLarge?.copyWith(color: scheme.onSurfaceVariant),
        strong: const TextStyle(fontWeight: FontWeight.w700),
        em: const TextStyle(fontStyle: FontStyle.italic),
        del: TextStyle(color: scheme.onSurfaceVariant),
        a: TextStyle(
          color: scheme.primary,
          decoration: TextDecoration.underline,
          decorationColor: scheme.primary,
        ),
        blockSpacing: 8,
        listBullet: tt.labelLarge?.copyWith(color: scheme.onSurfaceVariant),
        listIndent: 24,
        code: mono.copyWith(
          fontSize: 13,
          height: 1.4,
          color: scheme.onSurface,
          backgroundColor: scheme.surfaceContainerHighest,
        ),
        codeblockDecoration: BoxDecoration(
          color: scheme.surfaceContainerHighest,
          borderRadius: BorderRadius.circular(HermesRadius.md),
          border: Border.all(color: scheme.outlineVariant),
        ),
        blockquote: tt.bodyMedium?.copyWith(
          color: scheme.onSurfaceVariant,
          fontStyle: FontStyle.italic,
        ),
        blockquoteDecoration: BoxDecoration(
          border: Border(left: BorderSide(color: scheme.primary, width: 3)),
          color: scheme.surfaceContainerHighest,
          borderRadius: BorderRadius.circular(HermesRadius.sm),
        ),
        tableHead: tt.labelLarge,
        tableBody: tt.bodySmall,
        tableBorder: TableBorder.all(
          color: scheme.outlineVariant,
          width: 1,
          borderRadius: BorderRadius.circular(HermesRadius.sm),
        ),
        tableColumnWidth: const FlexColumnWidth(),
        checkbox: TextStyle(color: scheme.primary),
        horizontalRuleDecoration: BoxDecoration(
          border: Border(top: BorderSide(width: 1, color: scheme.outlineVariant)),
        ),
      ),
    );
  }
}
