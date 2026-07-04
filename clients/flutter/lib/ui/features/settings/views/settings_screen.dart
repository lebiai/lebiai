import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:hermes_app/data/models/settings_models.dart';
import 'package:hermes_app/ui/features/connection/view_models/connection_providers.dart';
import 'package:hermes_app/ui/features/settings/view_models/settings_providers.dart';
import 'package:hermes_app/ui/theme/app_dimensions.dart';
import 'package:hermes_app/ui/widgets/app_markdown.dart';

/// Management surface: config (model switch) / skills / memories, grouped in
/// cards on top of the shared theme.
class SettingsScreen extends ConsumerWidget {
  const SettingsScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return DefaultTabController(
      length: 3,
      child: Scaffold(
        appBar: AppBar(
          title: const Text('管理'),
          bottom: const TabBar(
            tabs: [
              Tab(text: '配置'),
              Tab(text: '技能'),
              Tab(text: '记忆'),
            ],
          ),
        ),
        body: TabBarView(children: <Widget>[
          _ConfigTab(),
          _SkillsTab(),
          _MemoriesTab(),
        ]),

      ),
    );
  }
}

// ===========================================================================

class _ConfigTab extends ConsumerStatefulWidget {
  @override
  ConsumerState<_ConfigTab> createState() => _ConfigTabState();
}

class _ConfigTabState extends ConsumerState<_ConfigTab> {
  final _modelCtrl = TextEditingController();
  final _maxTokensCtrl = TextEditingController();
  bool _loaded = false;
  bool _saving = false;

  @override
  void dispose() {
    _modelCtrl.dispose();
    _maxTokensCtrl.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    setState(() => _saving = true);
    final update = <String, dynamic>{};
    if (_modelCtrl.text.trim().isNotEmpty) update['model'] = _modelCtrl.text.trim();
    if (_maxTokensCtrl.text.trim().isNotEmpty) {
      update['maxTokens'] = int.tryParse(_maxTokensCtrl.text.trim());
    }
    try {
      if (update.isNotEmpty) {
        await ref.read(hermesClientProvider).updateConfig(update);
      }
      ref.invalidate(configProvider);
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(content: Text('已保存（部分项需重启 server 生效）')),
        );
      }
    } on Object catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('保存失败: $e')),
        );
      }
    } finally {
      if (mounted) setState(() => _saving = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final cfg = ref.watch(configProvider);
    if (!_loaded) {
      cfg.whenData((c) {
        _modelCtrl.text = c.model;
        _maxTokensCtrl.text = c.maxTokens.toString();
        _loaded = true;
      });
    }
    return cfg.when(
      loading: () => const Center(child: CircularProgressIndicator(strokeWidth: 2)),
      error: (e, _) => _ErrorBody(message: '$e'),
      data: (c) => ListView(
        padding: const EdgeInsets.all(HermesSpacing.md),
        children: [
          _SectionCard(
            title: '模型',
            children: [
              TextField(
                controller: _modelCtrl,
                decoration: const InputDecoration(
                  labelText: 'model',
                  hintText: 'claude-sonnet-4-20250514',
                  prefixIcon: Icon(Icons.smart_toy_outlined),
                ),
              ),
              const SizedBox(height: HermesSpacing.md),
              TextField(
                controller: _maxTokensCtrl,
                keyboardType: TextInputType.number,
                decoration: const InputDecoration(
                  labelText: 'max_tokens',
                  prefixIcon: Icon(Icons.token_outlined),
                ),
              ),
              const SizedBox(height: HermesSpacing.md),
              SizedBox(
                width: double.infinity,
                child: FilledButton.icon(
                  icon: _saving
                      ? const SizedBox(
                          width: 16,
                          height: 16,
                          child: CircularProgressIndicator(strokeWidth: 2))
                      : const Icon(Icons.save_outlined, size: 18),
                  label: const Text('保存'),
                  onPressed: _saving ? null : _save,
                ),
              ),
            ],
          ),
          const SizedBox(height: HermesSpacing.md),
          _SectionCard(
            title: '运行环境',
            children: [
              _kv('provider', c.defaultProvider),
              _kv('base url', c.baseUrl),
              _kv('api key', c.apiKeyMasked),
              _kv('workspace', c.workspaceRoot),
              _kv('language', c.uiLanguage),
            ],
          ),
        ],
      ),
    );
  }

  Widget _kv(String k, String v) {
    final scheme = Theme.of(context).colorScheme;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: HermesSpacing.xs + 1),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(
            width: 84,
            child: Text(k,
                style: TextStyle(
                    fontSize: 12.5, color: scheme.onSurfaceVariant)),
          ),
          Expanded(child: SelectableText(v, style: const TextStyle(fontSize: 13))),
        ],
      ),
    );
  }
}

class _SkillsTab extends ConsumerWidget {
  const _SkillsTab();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final skills = ref.watch(skillsListProvider);
    return skills.when(
      loading: () => const Center(child: CircularProgressIndicator(strokeWidth: 2)),
      error: (e, _) => _ErrorBody(message: '$e'),
      data: (list) {
        if (list.isEmpty) {
          return _EmptyBody(icon: Icons.auto_awesome_mosaic_outlined, text: '暂无技能');
        }
        return ListView.builder(
          padding: const EdgeInsets.all(HermesSpacing.md),
          itemCount: list.length,
          itemBuilder: (ctx, i) {
            final s = list[i];
            return Padding(
              padding: const EdgeInsets.only(bottom: HermesSpacing.sm),
              child: _SkillCard(
                skill: s,
                canDelete: s.scope == 'User',
                onDelete: () async {
                  await ref.read(hermesClientProvider).deleteSkill(s.name);
                  ref.invalidate(skillsListProvider);
                },
              ),
            );
          },
        );
      },
    );
  }
}

/// Collapsible skill card. Header shows name + scope + description; tapping
/// expands the full body rendered as markdown (headings, code, lists, …).
class _SkillCard extends StatefulWidget {
  const _SkillCard({required this.skill, required this.canDelete, required this.onDelete});

  final SkillItem skill;
  final bool canDelete;
  final VoidCallback onDelete;

  @override
  State<_SkillCard> createState() => _SkillCardState();
}

class _SkillCardState extends State<_SkillCard> {
  bool _expanded = false;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final s = widget.skill;
    return Container(
      clipBehavior: Clip.hardEdge,
      decoration: BoxDecoration(
        color: scheme.surface,
        borderRadius: BorderRadius.circular(HermesRadius.lg),
        border: Border.all(color: scheme.outlineVariant),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          InkWell(
            onTap: () => setState(() => _expanded = !_expanded),
            child: Padding(
              padding: const EdgeInsets.symmetric(
                  horizontal: HermesSpacing.md, vertical: HermesSpacing.sm + 2),
              child: Row(
                children: [
                  Container(
                    width: 32,
                    height: 32,
                    decoration: BoxDecoration(
                      color: scheme.primaryContainer,
                      borderRadius: BorderRadius.circular(HermesRadius.sm + 2),
                    ),
                    child: Icon(Icons.auto_awesome_outlined,
                        size: 18, color: scheme.onPrimaryContainer),
                  ),
                  const SizedBox(width: HermesSpacing.sm + 2),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Text(s.name,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: const TextStyle(
                                fontSize: 15, fontWeight: FontWeight.w600)),
                        const SizedBox(height: 1),
                        Text(
                          '${s.scope} · ${s.description}',
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(
                              fontSize: 12, color: scheme.onSurfaceVariant),
                        ),
                      ],
                    ),
                  ),
                  AnimatedRotation(
                    turns: _expanded ? 0.5 : 0,
                    duration: const Duration(milliseconds: 150),
                    child: Icon(Icons.keyboard_arrow_down,
                        size: 20, color: scheme.onSurfaceVariant),
                  ),
                ],
              ),
            ),
          ),
          AnimatedSize(
            duration: const Duration(milliseconds: 160),
            curve: Curves.easeInOut,
            alignment: Alignment.topCenter,
            child: _expanded
                ? Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Divider(height: 1, color: scheme.outlineVariant),
                      if (s.triggers.isNotEmpty)
                        Padding(
                          padding: const EdgeInsets.fromLTRB(HermesSpacing.md,
                              HermesSpacing.sm, HermesSpacing.md, 0),
                          child: Wrap(
                            spacing: 6,
                            runSpacing: 4,
                            children: [
                              for (final t in s.triggers)
                                Chip(
                                  label: Text(t),
                                  padding: EdgeInsets.zero,
                                  labelStyle: TextStyle(
                                      fontSize: 11, color: scheme.onSurfaceVariant),
                                  visualDensity: const VisualDensity(
                                      horizontal: -3, vertical: -3),
                                  materialTapTargetSize:
                                      MaterialTapTargetSize.shrinkWrap,
                                  backgroundColor: scheme.surfaceContainerHighest,
                                ),
                            ],
                          ),
                        ),
                      Padding(
                        padding: const EdgeInsets.all(HermesSpacing.md),
                        child: AppMarkdown(data: s.body),
                      ),
                      if (widget.canDelete)
                        Padding(
                          padding: const EdgeInsets.only(
                              left: HermesSpacing.md,
                              right: HermesSpacing.md,
                              bottom: HermesSpacing.sm),
                          child: Align(
                            alignment: Alignment.centerLeft,
                            child: TextButton.icon(
                              icon: const Icon(Icons.delete_outline, size: 18),
                              label: const Text('删除'),
                              style: TextButton.styleFrom(
                                  foregroundColor: scheme.error),
                              onPressed: widget.onDelete,
                            ),
                          ),
                        ),
                    ],
                  )
                : const SizedBox(width: double.infinity, height: 0),
          ),
        ],
      ),
    );
  }
}

class _MemoriesTab extends ConsumerWidget {
  const _MemoriesTab();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final scheme = Theme.of(context).colorScheme;
    final memories = ref.watch(memoriesListProvider);
    return memories.when(
      loading: () => const Center(child: CircularProgressIndicator(strokeWidth: 2)),
      error: (e, _) => _ErrorBody(message: '$e'),
      data: (list) {
        if (list.isEmpty) {
          return const _EmptyBody(icon: Icons.psychology_outlined, text: '暂无记忆');
        }
        return ListView.builder(
          padding: const EdgeInsets.all(HermesSpacing.md),
          itemCount: list.length,
          itemBuilder: (ctx, i) {
            final m = list[i];
            return Padding(
              padding: const EdgeInsets.only(bottom: HermesSpacing.sm),
              child: _SectionCard(
                title: m.body,
                subtitle:
                    '${m.scope} · ${m.confidence} · ${m.zone}${m.tags.isNotEmpty ? ' · ${m.tags.join(", ")}' : ''}',
                leading: Icon(
                  m.pinned ? Icons.push_pin : Icons.push_pin_outlined,
                  size: 18,
                  color: m.pinned ? const Color(0xFFF59E0B) : scheme.onSurfaceVariant,
                ),
                children: [
                  Row(
                    children: [
                      TextButton.icon(
                        icon: Icon(m.pinned ? Icons.push_pin : Icons.push_pin_outlined,
                            size: 16),
                        label: Text(m.pinned ? '取消置顶' : '置顶'),
                        onPressed: () async {
                          await ref.read(hermesClientProvider).togglePinMemory(m.id);
                          ref.invalidate(memoriesListProvider);
                        },
                      ),
                      TextButton.icon(
                        icon: const Icon(Icons.delete_outline, size: 16),
                        label: const Text('删除'),
                        style: TextButton.styleFrom(foregroundColor: scheme.error),
                        onPressed: () async {
                          await ref
                              .read(hermesClientProvider)
                              .deleteMemory(m.id, scope: m.scope);
                          ref.invalidate(memoriesListProvider);
                        },
                      ),
                    ],
                  ),
                ],
              ),
            );
          },
        );
      },
    );
  }
}

// ----- shared bits ---------------------------------------------------------

class _SectionCard extends StatelessWidget {
  const _SectionCard({
    required this.title,
    required this.children,
    this.subtitle,
    this.leading,
  });

  final String title;
  final String? subtitle;
  final Widget? leading;
  final List<Widget> children;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.all(HermesSpacing.md),
      decoration: BoxDecoration(
        color: scheme.surface,
        borderRadius: BorderRadius.circular(HermesRadius.lg),
        border: Border.all(color: scheme.outlineVariant),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              if (leading != null) ...[
                leading!,
                const SizedBox(width: HermesSpacing.sm),
              ],
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Text(title,
                        maxLines: 2,
                        overflow: TextOverflow.ellipsis,
                        style: const TextStyle(
                            fontSize: 15, fontWeight: FontWeight.w600)),
                    if (subtitle != null) ...[
                      const SizedBox(height: 2),
                      Text(subtitle!,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(
                              fontSize: 12, color: scheme.onSurfaceVariant)),
                    ],
                  ],
                ),
              ),
            ],
          ),
          if (children.isNotEmpty) ...[
            const SizedBox(height: HermesSpacing.md),
            ...children,
          ],
        ],
      ),
    );
  }
}

class _ErrorBody extends StatelessWidget {
  const _ErrorBody({required this.message});
  final String message;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(HermesSpacing.xl),
        child: Text('加载失败\n$message',
            textAlign: TextAlign.center,
            style: TextStyle(color: scheme.error, fontSize: 13)),
      ),
    );
  }
}

class _EmptyBody extends StatelessWidget {
  const _EmptyBody({required this.icon, required this.text});
  final IconData icon;
  final String text;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Center(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(icon, size: 40, color: scheme.onSurfaceVariant.withValues(alpha: 0.4)),
          const SizedBox(height: HermesSpacing.sm),
          Text(text, style: TextStyle(color: scheme.onSurfaceVariant)),
        ],
      ),
    );
  }
}
