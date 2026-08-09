import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:hermes_app/ui/features/chat/view_models/chat_providers.dart';
import 'package:hermes_app/ui/features/chat/view_models/sessions_providers.dart';
import 'package:hermes_app/ui/features/connection/view_models/connection_providers.dart';
import 'package:hermes_app/ui/features/settings/views/settings_screen.dart';
import 'package:hermes_app/ui/theme/app_dimensions.dart';
import 'package:hermes_app/ui/widgets/brand_mark.dart';
import 'package:hermes_app/ui/widgets/status_dot.dart';

/// Side drawer: brand + connection (status + editable server URL), new chat,
/// session history (switch / delete), and a settings entry.
class ChatDrawer extends ConsumerWidget {
  const ChatDrawer({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final sessions = ref.watch(sessionsProvider);
    final currentId = ref.watch(
      chatStateProvider.select((s) => s.sessionId),
    );
    final scheme = Theme.of(context).colorScheme;

    return ColoredBox(
      color: Theme.of(context).scaffoldBackgroundColor,
      child: SafeArea(
        child: Column(
          children: [
            // ---- Brand header ------------------------------------------
            Padding(
              padding: const EdgeInsets.fromLTRB(
                  HermesSpacing.lg, HermesSpacing.lg, HermesSpacing.md, HermesSpacing.md),
              child: Row(
                children: [
                  const BrandMark(size: 34),
                  const SizedBox(width: HermesSpacing.sm + 2),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Text('lebi-AI',
                            style: Theme.of(context).textTheme.titleLarge),
                        const SizedBox(height: 2),
                        _ConnectionPill(onEdit: () => _editServer(context, ref)),
                      ],
                    ),
                  ),
                ],
              ),
            ),
            const Divider(),
            // ---- New chat ----------------------------------------------
            Padding(
              padding: const EdgeInsets.symmetric(
                  horizontal: HermesSpacing.md, vertical: HermesSpacing.sm),
              child: FilledButton.icon(
                onPressed: () async {
                  await ref.read(chatStateProvider.notifier).newChat();
                  ref.invalidate(sessionsProvider);
                  if (context.mounted) Navigator.of(context).pop();
                },
                icon: const Icon(Icons.edit_square, size: 18),
                label: const Text('新建对话'),
              ),
            ),
            // ---- Session list ------------------------------------------
            Padding(
              padding: const EdgeInsets.only(
                  left: HermesSpacing.lg,
                  top: HermesSpacing.sm,
                  bottom: HermesSpacing.xs),
              child: Align(
                alignment: Alignment.centerLeft,
                child: Text(
                  '历史会话',
                  style: TextStyle(
                    fontSize: 11,
                    fontWeight: FontWeight.w700,
                    letterSpacing: 0.6,
                    color: scheme.onSurfaceVariant,
                  ),
                ),
              ),
            ),
            Expanded(
              child: sessions.when(
                loading: () => const Center(child: Padding(
                  padding: EdgeInsets.all(HermesSpacing.xl),
                  child: CircularProgressIndicator(strokeWidth: 2),
                )),
                error: (e, _) => Center(
                  child: Padding(
                    padding: const EdgeInsets.all(HermesSpacing.lg),
                    child: Text('加载失败\n$e',
                        textAlign: TextAlign.center,
                        style: TextStyle(color: scheme.onSurfaceVariant, fontSize: 13)),
                  ),
                ),
                data: (list) {
                  if (list.isEmpty) {
                    return Center(
                      child: Padding(
                        padding: const EdgeInsets.all(HermesSpacing.xl),
                        child: Text(
                          '还没有历史会话',
                          style: TextStyle(
                              color: scheme.onSurfaceVariant, fontSize: 13),
                        ),
                      ),
                    );
                  }
                  return ListView.separated(
                    padding: const EdgeInsets.symmetric(
                        horizontal: HermesSpacing.sm, vertical: 0),
                    itemCount: list.length,
                    separatorBuilder: (_, __) => const SizedBox(height: 2),
                    itemBuilder: (ctx, i) {
                      final s = list[i];
                      final active = s.id == currentId;
                      return _SessionTile(
                        title: s.title,
                        subtitle: _relativeTime(s.createdAt),
                        active: active,
                        onTap: () async {
                          await ref
                              .read(chatStateProvider.notifier)
                              .loadHistory(s.path);
                          if (context.mounted) Navigator.of(context).pop();
                        },
                        onDelete: () async {
                          await ref.read(hermesClientProvider).deleteSession(s.path);
                          ref.invalidate(sessionsProvider);
                        },
                      );
                    },
                  );
                },
              ),
            ),
            const Divider(),
            // ---- Footer ------------------------------------------------
            ListTile(
              leading: const Icon(Icons.settings_outlined),
              title: const Text('管理 / 配置'),
              trailing: const Icon(Icons.chevron_right, size: 20),
              onTap: () {
                Navigator.of(context).pop();
                Navigator.push(
                  context,
                  MaterialPageRoute(builder: (_) => const SettingsScreen()),
                );
              },
            ),
          ],
        ),
      ),
    );
  }

  Future<void> _editServer(BuildContext context, WidgetRef ref) async {
    final result = await showDialog<({String url, String token})>(
      context: context,
      builder: (ctx) => _ServerUrlDialog(
        initialUrl: ref.read(serverUrlProvider),
        initialToken: ref.read(serverTokenProvider),
      ),
    );
    if (result == null || result.url.isEmpty) return;
    ref.read(serverUrlProvider.notifier).set(result.url);
    ref.read(serverTokenProvider.notifier).set(result.token);
    ref.invalidate(healthFutureProvider);
    ref.invalidate(sessionsProvider);
    // ChatNotifier watches hermesClientProvider (url+token) → rebuilds & reconnects.
  }

  static String _relativeTime(String iso) {
    try {
      final t = DateTime.parse(iso);
      final diff = DateTime.now().difference(t);
      if (diff.inMinutes < 1) return '刚刚';
      if (diff.inMinutes < 60) return '${diff.inMinutes} 分钟前';
      if (diff.inHours < 24) return '${diff.inHours} 小时前';
      if (diff.inDays < 7) return '${diff.inDays} 天前';
      return '${t.year}-${t.month.toString().padLeft(2, '0')}-${t.day.toString().padLeft(2, '0')}';
    } on Object {
      return iso;
    }
  }
}

class _ConnectionPill extends ConsumerWidget {
  const _ConnectionPill({required this.onEdit});
  final VoidCallback onEdit;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final scheme = Theme.of(context).colorScheme;
    final health = ref.watch(healthFutureProvider);
    final status = health.when(
      data: (_) => ConnStatus.online,
      loading: () => ConnStatus.checking,
      error: (_, __) => ConnStatus.offline,
    );
    final (label, _) = switch (status) {
      ConnStatus.online => ('已连接', null),
      ConnStatus.offline => ('未连接', null),
      ConnStatus.checking => ('连接中', null),
    };
    final url = ref.watch(serverUrlProvider).replaceFirst(RegExp(r'^https?://'), '');
    return InkWell(
      borderRadius: BorderRadius.circular(HermesRadius.pill),
      onTap: onEdit,
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: 2),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            StatusDot(size: 6, status: status),
            const SizedBox(width: 5),
            Flexible(
              child: Text(
                '$label · $url',
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  fontSize: 11.5,
                  color: scheme.onSurfaceVariant,
                ),
              ),
            ),
            const SizedBox(width: 3),
            Icon(Icons.tune, size: 13, color: scheme.onSurfaceVariant),
          ],
        ),
      ),
    );
  }
}

class _SessionTile extends StatelessWidget {
  const _SessionTile({
    required this.title,
    required this.subtitle,
    required this.active,
    required this.onTap,
    required this.onDelete,
  });

  final String title;
  final String subtitle;
  final bool active;
  final VoidCallback onTap;
  final VoidCallback onDelete;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Material(
      color: active ? scheme.primaryContainer.withValues(alpha: 0.5) : Colors.transparent,
      borderRadius: BorderRadius.circular(HermesRadius.md),
      child: InkWell(
        borderRadius: BorderRadius.circular(HermesRadius.md),
        onTap: onTap,
        child: Padding(
          padding: const EdgeInsets.symmetric(
              horizontal: HermesSpacing.md, vertical: HermesSpacing.sm + 1),
          child: Row(
            children: [
              Icon(
                active ? Icons.chat_bubble : Icons.chat_bubble_outline,
                size: 16,
                color: active ? scheme.primary : scheme.onSurfaceVariant,
              ),
              const SizedBox(width: HermesSpacing.sm + 2),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Text(
                      title,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(
                        fontSize: 14,
                        fontWeight: active ? FontWeight.w600 : FontWeight.w500,
                        color: scheme.onSurface,
                      ),
                    ),
                    Text(
                      subtitle,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(
                          fontSize: 11.5, color: scheme.onSurfaceVariant),
                    ),
                  ],
                ),
              ),
              IconButton(
                icon: const Icon(Icons.delete_outline, size: 17),
                visualDensity: VisualDensity.compact,
                tooltip: '删除',
                onPressed: onDelete,
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _ServerUrlDialog extends StatefulWidget {
  const _ServerUrlDialog({required this.initialUrl, required this.initialToken});
  final String initialUrl;
  final String initialToken;

  @override
  State<_ServerUrlDialog> createState() => _ServerUrlDialogState();
}

class _ServerUrlDialogState extends State<_ServerUrlDialog> {
  late final TextEditingController _url;
  late final TextEditingController _token;
  bool _obscureToken = true;

  @override
  void initState() {
    super.initState();
    _url = TextEditingController(text: widget.initialUrl);
    _token = TextEditingController(text: widget.initialToken);
  }

  @override
  void dispose() {
    _url.dispose();
    _token.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: const Text('服务器连接'),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          TextField(
            controller: _url,
            autofocus: true,
            decoration: const InputDecoration(
              labelText: '地址',
              hintText: 'http://localhost:8765',
              prefixIcon: Icon(Icons.dns_outlined),
            ),
          ),
          const SizedBox(height: HermesSpacing.md),
          TextField(
            controller: _token,
            obscureText: _obscureToken,
            decoration: InputDecoration(
              labelText: 'Token',
              hintText: 'hermes serve 启动时打印的令牌',
              prefixIcon: const Icon(Icons.key_outlined),
              suffixIcon: IconButton(
                icon: Icon(
                  _obscureToken ? Icons.visibility_outlined : Icons.visibility_off_outlined,
                  size: 20,
                ),
                onPressed: () => setState(() => _obscureToken = !_obscureToken),
              ),
            ),
          ),
          const SizedBox(height: HermesSpacing.sm),
          Text(
            '提示：Android 模拟器需用 10.0.2.2 代替 localhost。Token 见 `hermes serve` 启动输出（本机也必填）。',
            style: Theme.of(context).textTheme.labelSmall,
          ),
        ],
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('取消'),
        ),
        FilledButton(
          onPressed: () => Navigator.pop(
            context,
            (url: _url.text.trim(), token: _token.text.trim()),
          ),
          child: const Text('连接'),
        ),
      ],
    );
  }
}
