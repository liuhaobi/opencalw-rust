import 'package:flutter/material.dart';

import '../models/models.dart';
import '../state/gateway_controller.dart';
import '../theme.dart';
import 'chat_page.dart';

class HomePage extends StatefulWidget {
  const HomePage({super.key, required this.controller});

  final GatewayController controller;

  @override
  State<HomePage> createState() => _HomePageState();
}

class _HomePageState extends State<HomePage> {
  bool _creating = false;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      widget.controller.refreshSessions();
    });
  }

  Future<void> _createSession() async {
    setState(() => _creating = true);
    final row = await widget.controller.createSession();
    if (!mounted) return;
    setState(() => _creating = false);
    if (row == null) {
      _showError(widget.controller.lastError ?? '创建会话失败');
      return;
    }
    _open(row);
  }

  void _open(SessionRow row) {
    Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => ChatPage(
          controller: widget.controller,
          sessionKey: row.key,
          title: row.displayName,
        ),
      ),
    );
  }

  void _showError(String message) {
    ScaffoldMessenger.of(context)
        .showSnackBar(SnackBar(content: Text(message)));
  }

  void _showGatewayInfo() {
    final controller = widget.controller;
    final hello = controller.hello;
    showDialog<void>(
      context: context,
      builder: (context) {
        final theme = Theme.of(context);
        return AlertDialog(
          title: const Text('网关信息'),
          content: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              _infoRow('地址', controller.endpoint?.toString() ?? '-'),
              _infoRow('协议', 'v${hello?.protocol ?? '-'}'),
              _infoRow('服务端', hello?.serverVersion ?? '-'),
              _infoRow('鉴权', hello?.authMethod ?? '-'),
              _infoRow('方法数', '${hello?.methods.length ?? 0}'),
              _infoRow('事件数', '${hello?.events.length ?? 0}'),
              _infoRow('心跳', '${hello?.tickIntervalMs ?? 0} ms'),
              if (controller.lastTick != null)
                _infoRow(
                  '最近心跳',
                  TimeOfDay.fromDateTime(controller.lastTick!).format(context),
                ),
              if (hello != null && hello.scopes.isNotEmpty) ...[
                const SizedBox(height: 12),
                Text('权限', style: theme.textTheme.labelMedium),
                const SizedBox(height: 4),
                Wrap(
                  spacing: 6,
                  runSpacing: 6,
                  children: hello.scopes
                      .map((scope) => Chip(
                            label: Text(scope,
                                style: const TextStyle(fontSize: 11)),
                            visualDensity: VisualDensity.compact,
                          ))
                      .toList(),
                ),
              ],
            ],
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.of(context).pop(),
              child: const Text('关闭'),
            ),
          ],
        );
      },
    );
  }

  Widget _infoRow(String label, String value) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 3),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(
            width: 68,
            child: Text(
              label,
              style: TextStyle(
                color: Theme.of(context).colorScheme.onSurfaceVariant,
                fontSize: 13,
              ),
            ),
          ),
          Expanded(
            child: Text(value, style: const TextStyle(fontSize: 13)),
          ),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final controller = widget.controller;
    final theme = Theme.of(context);
    final sessions = controller.sessions;

    return Scaffold(
      appBar: AppBar(
        titleSpacing: 20,
        title: Row(
          children: [
            const Text('🦞', style: TextStyle(fontSize: 20)),
            const SizedBox(width: 8),
            const Text(
              'OpenClaw',
              style: TextStyle(fontWeight: FontWeight.w700, letterSpacing: -0.3),
            ),
            const SizedBox(width: 10),
            Container(
              width: 8,
              height: 8,
              decoration: const BoxDecoration(
                color: Color(0xFF4ADE80),
                shape: BoxShape.circle,
              ),
            ),
          ],
        ),
        actions: [
          IconButton(
            tooltip: '刷新',
            onPressed: controller.refreshSessions,
            icon: const Icon(Icons.refresh),
          ),
          PopupMenuButton<String>(
            onSelected: (value) async {
              switch (value) {
                case 'info':
                  _showGatewayInfo();
                  break;
                case 'disconnect':
                  await controller.disconnect();
                  break;
              }
            },
            itemBuilder: (context) => const [
              PopupMenuItem(value: 'info', child: Text('网关信息')),
              PopupMenuItem(value: 'disconnect', child: Text('断开连接')),
            ],
          ),
        ],
      ),
      floatingActionButton: FloatingActionButton.extended(
        onPressed: _creating ? null : _createSession,
        backgroundColor: kBrandCoral,
        foregroundColor: Colors.black,
        icon: _creating
            ? const SizedBox(
                width: 16,
                height: 16,
                child: CircularProgressIndicator(strokeWidth: 2),
              )
            : const Icon(Icons.add_comment_outlined),
        label: const Text('新会话'),
      ),
      body: RefreshIndicator(
        onRefresh: controller.refreshSessions,
        child: sessions.isEmpty
            ? ListView(
                padding: const EdgeInsets.all(32),
                children: [
                  const SizedBox(height: 80),
                  Icon(
                    Icons.chat_bubble_outline,
                    size: 48,
                    color: theme.colorScheme.onSurfaceVariant,
                  ),
                  const SizedBox(height: 16),
                  Text(
                    '还没有会话',
                    textAlign: TextAlign.center,
                    style: theme.textTheme.titleMedium,
                  ),
                  const SizedBox(height: 8),
                  Text(
                    '点击右下角新建一个会话开始对话。',
                    textAlign: TextAlign.center,
                    style: theme.textTheme.bodySmall?.copyWith(
                      color: theme.colorScheme.onSurfaceVariant,
                    ),
                  ),
                ],
              )
            : ListView.separated(
                padding: const EdgeInsets.fromLTRB(16, 8, 16, 96),
                itemCount: sessions.length,
                separatorBuilder: (_, __) => const SizedBox(height: 8),
                itemBuilder: (context, index) {
                  final row = sessions[index];
                  return _SessionTile(
                    row: row,
                    onTap: () => _open(row),
                  );
                },
              ),
      ),
    );
  }
}

class _SessionTile extends StatelessWidget {
  const _SessionTile({required this.row, required this.onTap});

  final SessionRow row;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final updated = row.updatedDateTime;

    return Card(
      child: InkWell(
        borderRadius: BorderRadius.circular(16),
        onTap: onTap,
        child: Padding(
          padding: const EdgeInsets.all(14),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              CircleAvatar(
                radius: 20,
                backgroundColor: kBrandCoral.withValues(alpha: 0.18),
                child: Text(
                  row.initial,
                  style: const TextStyle(
                    color: kBrandCoral,
                    fontWeight: FontWeight.w700,
                  ),
                ),
              ),
              const SizedBox(width: 12),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Row(
                      children: [
                        Expanded(
                          child: Text(
                            row.displayName,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: theme.textTheme.titleSmall?.copyWith(
                              fontWeight: FontWeight.w600,
                            ),
                          ),
                        ),
                        if (updated != null)
                          Text(
                            _formatTime(updated),
                            style: theme.textTheme.labelSmall?.copyWith(
                              color: theme.colorScheme.onSurfaceVariant,
                            ),
                          ),
                      ],
                    ),
                    const SizedBox(height: 4),
                    Text(
                      (row.preview?.trim().isNotEmpty ?? false)
                          ? row.preview!
                          : '暂无消息',
                      maxLines: 2,
                      overflow: TextOverflow.ellipsis,
                      style: theme.textTheme.bodySmall?.copyWith(
                        color: theme.colorScheme.onSurfaceVariant,
                        height: 1.35,
                      ),
                    ),
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  String _formatTime(DateTime time) {
    final now = DateTime.now();
    final sameDay = now.year == time.year &&
        now.month == time.month &&
        now.day == time.day;
    final hh = time.hour.toString().padLeft(2, '0');
    final mm = time.minute.toString().padLeft(2, '0');
    if (sameDay) return '$hh:$mm';
    return '${time.month}/${time.day}';
  }
}
