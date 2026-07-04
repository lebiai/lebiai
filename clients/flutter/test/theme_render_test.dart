import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:hermes_app/ui/theme/app_theme.dart';

/// Regression: the custom theme's TextTheme must carry explicit colors. An
/// earlier build left every TextStyle color null, which bypassed Material 3's
/// auto-tinting and rendered TextField content / hero headlines as a
/// washed-out grey (reported as "text unreadable").
void main() {
  group('theme text colors are explicit and readable', () {
    for (final bright in [Brightness.light, Brightness.dark]) {
      final theme = bright == Brightness.light
          ? appLightTheme()
          : appDarkTheme();
      final name = bright == Brightness.light ? 'light' : 'dark';

      test('$name theme: every TextTheme style has a non-null color', () {
        final tt = theme.textTheme;
        final styles = {
          'displayLarge': tt.displayLarge,
          'displayMedium': tt.displayMedium,
          'headlineMedium': tt.headlineMedium,
          'titleLarge': tt.titleLarge,
          'titleMedium': tt.titleMedium,
          'bodyLarge': tt.bodyLarge,
          'bodyMedium': tt.bodyMedium,
          'labelLarge': tt.labelLarge,
        };
        for (final entry in styles.entries) {
          expect(entry.value, isNotNull, reason: '${entry.key} was null');
          expect(entry.value!.color, isNotNull,
              reason: '${entry.key}.color was null — text would render grey');
        }
      });

      testWidgets(
          '$name theme: TextField content resolves to onSurface (readable)',
          (tester) async {
        await tester.pumpWidget(
          MaterialApp(
            theme: theme,
            home: Scaffold(
              body: TextField(
                controller: TextEditingController(text: 'deepseek-v4-pro'),
              ),
            ),
          ),
        );
        await tester.pump();

        final editable = tester.widget<EditableText>(
          find.byType(EditableText),
        );
        // The TextField passes its content style through to EditableText.
        // It must resolve to the theme's onSurface, never a null/grey default.
        expect(editable.style.color, theme.colorScheme.onSurface);
      });
    }
  });
}
