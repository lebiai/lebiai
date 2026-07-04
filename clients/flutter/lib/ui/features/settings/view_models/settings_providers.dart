import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:hermes_app/data/models/settings_models.dart';
import 'package:hermes_app/ui/features/connection/view_models/connection_providers.dart';

final configProvider = FutureProvider.autoDispose<ConfigView>((ref) async {
  return ref.read(hermesClientProvider).getConfig();
});

final skillsListProvider = FutureProvider.autoDispose<List<SkillItem>>((ref) async {
  return ref.read(hermesClientProvider).listSkills();
});

final memoriesListProvider = FutureProvider.autoDispose<List<MemoryItem>>((ref) async {
  return ref.read(hermesClientProvider).listMemories();
});
