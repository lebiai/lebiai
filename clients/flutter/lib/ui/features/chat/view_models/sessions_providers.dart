import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:hermes_app/data/models/session.dart';
import 'package:hermes_app/ui/features/connection/view_models/connection_providers.dart';

/// Async list of past sessions. `autoDispose` + invalidate to refresh
/// (after new/delete).
final sessionsProvider =
    FutureProvider.autoDispose<List<SessionSummary>>((ref) async {
  final client = ref.watch(hermesClientProvider);
  return client.listSessions();
});
