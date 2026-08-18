import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:hermes_app/data/models/settings_models.dart';
import 'package:hermes_app/ui/features/connection/view_models/connection_providers.dart';
import 'package:hermes_app/ui/theme/app_dimensions.dart';

/// Pending-review inbox: accept / reject evolution candidates (memories & skills).
class EvolveScreen extends ConsumerStatefulWidget {
  const EvolveScreen({super.key});

  @override
  ConsumerState<EvolveScreen> createState() => _EvolveScreenState();
}

class _EvolveScreenState extends ConsumerState<EvolveScreen> {
  List<InboxItem>? _items;
  String? _error;
  bool _loading = true;
  final Set<String> _busy = {};

  @override
  void initState() {
    super.initState();
    _reload();
  }

  Future<void> _reload() async {
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final items = await ref.read(hermesClientProvider).listInbox();
      if (!mounted) return;
      setState(() {
        _items = items;
        _loading = false;
      });
    } on Object catch (e) {
      if (!mounted) return;
      setState(() {
        _error = '$e';
        _loading = false;
      });
    }
  }

  Future<void> _accept(InboxItem item) async {
    setState(() => _busy.add(item.id));
    try {
      await ref.read(hermesClientProvider).acceptInbox(item.id);
      if (!mounted) return;
      setState(() {
        _items = _items?.where((i) => i.id != item.id).toList();
        _busy.remove(item.id);
      });
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('已接受：${item.title}')),
      );
    } on Object catch (e) {
      if (!mounted) return;
      setState(() => _busy.remove(item.id));
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('接受失败：$e')),
      );
    }
  }

  Future<void> _reject(InboxItem item) async {
    setState(() => _busy.add(item.id));
    try {
      await ref.read(hermesClientProvider).rejectInbox(item.id);
      if (!mounted) return;
      setState(() {
        _items = _items?.where((i) => i.id != item.id).toList();
        _busy.remove(item.id);
      });
    } on Object catch (e) {
      if (!mounted) return;
      setState(() => _busy.remove(item.id));
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('拒绝失败：$e')),
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Scaffold(
      appBar: AppBar(
        title: const Text('进化收件箱'),
        actions: [
          IconButton(
            tooltip: '刷新',
            onPressed: _loading ? null : _reload,
            icon: const Icon(Icons.refresh),
          ),
        ],
      ),
      body: _loading
          ? const Center(child: CircularProgressIndicator(strokeWidth: 2))
          : _error != null
              ? Center(
                  child: Padding(
                    padding: const EdgeInsets.all(HermesSpacing.lg),
                    child: Text(
                      '加载失败\n$_error',
                      textAlign: TextAlign.center,
                      style: TextStyle(color: scheme.error),
                    ),
                  ),
                )
              : (_items == null || _items!.isEmpty)
                  ? Center(
                      child: Text(
                        '暂无待审候选\n对话后桌面/服务端 micro-reflection 会把记忆与技能候选放进这里',
                        textAlign: TextAlign.center,
                        style: TextStyle(color: scheme.onSurfaceVariant),
                      ),
                    )
                  : ListView.separated(
                      padding: const EdgeInsets.all(HermesSpacing.md),
                      itemCount: _items!.length,
                      separatorBuilder: (_, __) =>
                          const SizedBox(height: HermesSpacing.sm),
                      itemBuilder: (context, i) {
                        final item = _items![i];
                        final busy = _busy.contains(item.id);
                        return Card(
                          child: Padding(
                            padding: const EdgeInsets.all(HermesSpacing.md),
                            child: Column(
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                Row(
                                  children: [
                                    Chip(
                                      label: Text(
                                        item.kind == 'skill' ? '技能' : '记忆',
                                        style: const TextStyle(fontSize: 11),
                                      ),
                                      visualDensity: VisualDensity.compact,
                                    ),
                                    const SizedBox(width: 8),
                                    Expanded(
                                      child: Text(
                                        item.title,
                                        style: Theme.of(context)
                                            .textTheme
                                            .titleSmall,
                                        maxLines: 2,
                                        overflow: TextOverflow.ellipsis,
                                      ),
                                    ),
                                  ],
                                ),
                                if (item.rationale != null &&
                                    item.rationale!.isNotEmpty) ...[
                                  const SizedBox(height: 6),
                                  Text(
                                    item.rationale!,
                                    style: TextStyle(
                                      fontSize: 12,
                                      color: scheme.onSurfaceVariant,
                                    ),
                                  ),
                                ],
                                const SizedBox(height: 8),
                                Text(
                                  item.body,
                                  maxLines: 8,
                                  overflow: TextOverflow.ellipsis,
                                  style: const TextStyle(fontSize: 13),
                                ),
                                const SizedBox(height: 12),
                                Row(
                                  mainAxisAlignment: MainAxisAlignment.end,
                                  children: [
                                    TextButton(
                                      onPressed:
                                          busy ? null : () => _reject(item),
                                      child: const Text('拒绝'),
                                    ),
                                    const SizedBox(width: 8),
                                    FilledButton(
                                      onPressed:
                                          busy ? null : () => _accept(item),
                                      child: busy
                                          ? const SizedBox(
                                              width: 16,
                                              height: 16,
                                              child: CircularProgressIndicator(
                                                strokeWidth: 2,
                                              ),
                                            )
                                          : const Text('接受并落盘'),
                                    ),
                                  ],
                                ),
                              ],
                            ),
                          ),
                        );
                      },
                    ),
    );
  }
}
