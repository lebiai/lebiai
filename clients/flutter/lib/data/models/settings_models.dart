import 'package:flutter/foundation.dart';

/// Mirrors `hermes_server::routes::skills::SkillItem`.
@immutable
class SkillItem {
  const SkillItem({
    required this.name,
    required this.description,
    required this.triggers,
    required this.scope,
    required this.body,
  });
  final String name;
  final String description;
  final List<String> triggers;
  final String scope;
  final String body;

  factory SkillItem.fromJson(Map<String, dynamic> j) => SkillItem(
        name: j['name'] as String,
        description: j['description'] as String,
        triggers: (j['triggers'] as List).cast<String>(),
        scope: j['scope'] as String,
        body: j['body'] as String,
      );
}

/// Pending-review inbox item from `GET /api/v1/inbox`.
@immutable
class InboxItem {
  const InboxItem({
    required this.id,
    required this.createdAt,
    required this.source,
    required this.kind,
    required this.title,
    required this.body,
    this.zone,
    required this.tags,
    this.confidence,
    this.rationale,
    this.skillName,
    this.skillDescription,
    this.skillTriggers,
  });

  final String id;
  final String createdAt;
  final String source;
  final String kind; // memory | skill
  final String title;
  final String body;
  final String? zone;
  final List<String> tags;
  final String? confidence;
  final String? rationale;
  final String? skillName;
  final String? skillDescription;
  final List<String>? skillTriggers;

  factory InboxItem.fromJson(Map<String, dynamic> j) => InboxItem(
        id: j['id'] as String,
        createdAt: j['createdAt'] as String? ?? '',
        source: j['source'] as String? ?? '',
        kind: j['kind'] as String? ?? 'memory',
        title: j['title'] as String? ?? '',
        body: j['body'] as String? ?? '',
        zone: j['zone'] as String?,
        tags: (j['tags'] as List?)?.cast<String>() ?? const [],
        confidence: j['confidence'] as String?,
        rationale: j['rationale'] as String?,
        skillName: j['skillName'] as String?,
        skillDescription: j['skillDescription'] as String?,
        skillTriggers: (j['skillTriggers'] as List?)?.cast<String>(),
      );
}

/// Mirrors `hermes_server::routes::memory::MemoryItem`.
@immutable
class MemoryItem {
  const MemoryItem({
    required this.id,
    required this.body,
    required this.scope,
    required this.pinned,
    required this.confidence,
    required this.tags,
    required this.zone,
    required this.createdAt,
    required this.source,
  });
  final String id;
  final String body;
  final String scope;
  final bool pinned;
  final String confidence;
  final List<String> tags;
  final String zone;
  final String createdAt;
  final String source;

  factory MemoryItem.fromJson(Map<String, dynamic> j) => MemoryItem(
        id: j['id'] as String,
        body: j['body'] as String,
        scope: j['scope'] as String,
        pinned: j['pinned'] as bool,
        confidence: j['confidence'] as String,
        tags: (j['tags'] as List).cast<String>(),
        zone: j['zone'] as String,
        createdAt: j['createdAt'] as String,
        source: j['source'] as String,
      );
}

/// Mirrors `hermes_server::routes::config::ConfigView`.
@immutable
class ConfigView {
  const ConfigView({
    required this.defaultProvider,
    required this.model,
    required this.maxTokens,
    required this.apiKeyMasked,
    required this.baseUrl,
    required this.reflectMinTurns,
    required this.reflectAutoAcceptMemories,
    required this.contextModelLimit,
    required this.permissionsAllow,
    required this.permissionsDeny,
    required this.workspaceRoot,
    required this.uiLanguage,
  });
  final String defaultProvider;
  final String model;
  final int maxTokens;
  final String apiKeyMasked;
  final String baseUrl;
  final int reflectMinTurns;
  final bool reflectAutoAcceptMemories;
  final int contextModelLimit;
  final List<String> permissionsAllow;
  final List<String> permissionsDeny;
  final String workspaceRoot;
  final String uiLanguage;

  factory ConfigView.fromJson(Map<String, dynamic> j) => ConfigView(
        defaultProvider: j['defaultProvider'] as String,
        model: j['model'] as String,
        maxTokens: (j['maxTokens'] as num).toInt(),
        apiKeyMasked: j['apiKeyMasked'] as String,
        baseUrl: j['baseUrl'] as String,
        reflectMinTurns: (j['reflectMinTurns'] as num).toInt(),
        reflectAutoAcceptMemories: j['reflectAutoAcceptMemories'] as bool,
        contextModelLimit: (j['contextModelLimit'] as num).toInt(),
        permissionsAllow: (j['permissionsAllow'] as List).cast<String>(),
        permissionsDeny: (j['permissionsDeny'] as List).cast<String>(),
        workspaceRoot: j['workspaceRoot'] as String,
        uiLanguage: j['uiLanguage'] as String,
      );
}
