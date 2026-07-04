import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:hermes_app/ui/theme/app_theme.dart';
import 'package:hermes_app/ui/widgets/app_markdown.dart';

/// Smoke test: AppMarkdown must render a rich skill body (headings, lists,
/// fenced code, tables, blockquotes) without throwing, under both light and
/// dark themes. Previously skills were rendered as raw `SelectableText`, so
/// `#`, `**` and ``` fences showed as literal characters.
const _body = '''# Code Reviewer

Reviews pull requests for bugs, **security**, and style.

## When to use
- On a fresh PR
- Before merging

```rust
fn add(a: i32, b: i32) -> i32 {
    a + b
}
```

| Severity | Meaning |
|----------|---------|
| high     | must fix |
''';

void main() {
  for (final bright in [Brightness.light, Brightness.dark]) {
    final theme = bright == Brightness.light
        ? appLightTheme()
        : appDarkTheme();
    testWidgets('AppMarkdown renders rich body (${bright.name})',
        (tester) async {
      await tester.pumpWidget(MaterialApp(
        theme: theme,
        home: Scaffold(body: AppMarkdown(data: _body)),
      ));
      await tester.pumpAndSettle();
      // Heading text is rendered as a real Text node, not literal "# Code".
      expect(find.text('Code Reviewer'), findsOneWidget);
      expect(find.text('When to use'), findsOneWidget);
      // Fenced code content survived parsing (not swallowed as raw fence).
      expect(find.textContaining('fn add'), findsWidgets);
    });
  }
}
