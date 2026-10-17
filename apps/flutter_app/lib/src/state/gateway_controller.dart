/// Application state: connection lifecycle, session list, transcripts and the
/// streaming assistant buffer.
library;

import 'dart:async';

import 'package:flutter/foundation.dart';

import '../models/models.dart';
import '../protocol/endpoint.dart';
import '../protocol/frames.dart';
import '../protocol/gateway_client.dart';

class GatewayController extends ChangeNotifier {
  GatewayController({GatewayClient? client})
      : _client = client ?? GatewayClient();

  final GatewayClient _client;
  StreamSubscription<GatewayEvent>? _eventSubscription;
  StreamSubscription<GatewayLinkState>? _linkSubscription;
  Timer? _refreshTimer;

  GatewayLinkState _link = GatewayLinkState.disconnected;
  HelloOk? _hello;
  Uri? _endpoint;
  String? _lastError;
  String? _activeSessionKey;
  String? _activeRunId;
  DateTime? _lastTick;
  bool _sending = false;
  bool _disposed = false;

  List<SessionRow> _sessions = const <SessionRow>[];
  final Map<String, List<ChatMessage>> _messages =
      <String, List<ChatMessage>>{};
  final Map<String, String> _streaming = <String, String>{};
  final Set<String> _messageIds = <String>{};

  GatewayLinkState get link => _link;
  bool get isReady => _link == GatewayLinkState.ready;
  bool get isConnecting => _link == GatewayLinkState.connecting;
  HelloOk? get hello => _hello;
  Uri? get endpoint => _endpoint;
  String? get lastError => _lastError;
  String? get activeSessionKey => _activeSessionKey;
  bool get sending => _sending;
  DateTime? get lastTick => _lastTick;
  List<SessionRow> get sessions => _sessions;

  List<ChatMessage> messagesFor(String sessionKey) =>
      _messages[sessionKey] ?? const <ChatMessage>[];

  String streamingFor(String sessionKey) => _streaming[sessionKey] ?? '';

  bool isStreaming(String sessionKey) => _streaming.containsKey(sessionKey);

  Future<bool> connect(String rawEndpoint, String token) async {
    _lastError = null;
    _link = GatewayLinkState.connecting;
    _notify();

    try {
      final endpoint = parseGatewayEndpoint(rawEndpoint);
      final hello = await _client.connect(endpoint: endpoint, token: token);

      _eventSubscription ??= _client.events.listen(_onEvent);
      _linkSubscription ??= _client.linkStates.listen(_onLinkState);

      _endpoint = endpoint;
      _hello = hello;
      _link = GatewayLinkState.ready;
      _notify();

      await refreshSessions();
      return true;
    } catch (error) {
      _lastError = describeError(error);
      _link = GatewayLinkState.disconnected;
      _notify();
      return false;
    }
  }

  Future<void> refreshSessions() async {
    if (!isReady) return;
    try {
      final payload = await _client.request('sessions.list', <String, dynamic>{
        'limit': 100,
      });
      final raw = payload['sessions'];
      if (raw is List) {
        final rows = raw
            .whereType<Map>()
            .map((item) => SessionRow.fromJson(item.cast<String, dynamic>()))
            .where((row) => !row.archived)
            .toList()
          ..sort((a, b) => (b.updatedAt ?? 0).compareTo(a.updatedAt ?? 0));
        _sessions = rows;
        _notify();
      }
    } catch (error) {
      _lastError = describeError(error);
      _notify();
    }
  }

  Future<SessionRow?> createSession({String label = '新会话'}) async {
    if (!isReady) return null;
    try {
      final payload = await _client.request(
        'sessions.create',
        <String, dynamic>{'label': label},
      );
      final row = SessionRow.fromJson(payload);
      await refreshSessions();
      return row;
    } catch (error) {
      _lastError = describeError(error);
      _notify();
      return null;
    }
  }

  Future<void> openSession(String sessionKey) async {
    _activeSessionKey = sessionKey;
    _notify();
    if (messagesFor(sessionKey).isEmpty) {
      await loadHistory(sessionKey);
    }
  }

  Future<void> loadHistory(String sessionKey) async {
    if (!isReady) return;
    try {
      final payload = await _client.request('sessions.history', <String, dynamic>{
        'key': sessionKey,
        'limit': 200,
      });
      final raw = payload['messages'];
      if (raw is List) {
        final messages = raw
            .whereType<Map>()
            .map((item) => ChatMessage.fromJson(item.cast<String, dynamic>()))
            .toList()
          ..sort((a, b) => a.seq.compareTo(b.seq));
        _messages[sessionKey] = messages;
        for (final message in messages) {
          _messageIds.add(message.id);
        }
        _notify();
      }
    } catch (error) {
      _lastError = describeError(error);
      _notify();
    }
  }

  Future<void> send(String sessionKey, String text) async {
    final trimmed = text.trim();
    if (trimmed.isEmpty || !isReady) return;

    _sending = true;
    _streaming[sessionKey] = '';
    _activeSessionKey = sessionKey;
    _notify();

    try {
      final payload = await _client.request('chat.send', <String, dynamic>{
        'sessionKey': sessionKey,
        'message': trimmed,
      });
      _activeRunId = payload['runId']?.toString();
      _notify();
    } catch (error) {
      _sending = false;
      _streaming.remove(sessionKey);
      _lastError = describeError(error);
      _notify();
    }
  }

  Future<void> abort() async {
    final runId = _activeRunId;
    if (runId == null || !isReady) return;
    try {
      await _client.request('chat.abort', <String, dynamic>{'runId': runId});
    } catch (error) {
      _lastError = describeError(error);
      _notify();
    }
  }

  Future<void> disconnect() async {
    await _eventSubscription?.cancel();
    await _linkSubscription?.cancel();
    _eventSubscription = null;
    _linkSubscription = null;
    await _client.close();
    _link = GatewayLinkState.disconnected;
    _hello = null;
    _sessions = const <SessionRow>[];
    _messages.clear();
    _streaming.clear();
    _messageIds.clear();
    _sending = false;
    _activeRunId = null;
    _activeSessionKey = null;
    _notify();
  }

  void clearError() {
    if (_lastError == null) return;
    _lastError = null;
    _notify();
  }

  @override
  void dispose() {
    _disposed = true;
    _refreshTimer?.cancel();
    _eventSubscription?.cancel();
    _linkSubscription?.cancel();
    _client.dispose();
    super.dispose();
  }

  void _onEvent(GatewayEvent event) {
    switch (event.event) {
      case 'chat':
        _onChatEvent(event.payload);
        break;
      case 'session.message':
        _onSessionMessage(event.payload);
        break;
      case 'sessions.changed':
        _scheduleRefresh();
        break;
      case 'tick':
        _lastTick = DateTime.now();
        _notify();
        break;
      default:
        break;
    }
  }

  void _onChatEvent(Map<String, dynamic> payload) {
    final sessionKey =
        payload['sessionKey']?.toString() ?? _activeSessionKey;
    if (sessionKey == null) return;
    final kind = payload['kind']?.toString();

    switch (kind) {
      case 'start':
        _activeRunId = payload['runId']?.toString();
        _streaming[sessionKey] = '';
        _sending = true;
        _notify();
        break;
      case 'delta':
        _streaming[sessionKey] =
            (_streaming[sessionKey] ?? '') + (payload['delta']?.toString() ?? '');
        _notify();
        break;
      case 'final':
        final text = payload['text']?.toString() ?? _streaming[sessionKey] ?? '';
        final messageId = payload['messageId']?.toString();
        _streaming.remove(sessionKey);
        _sending = false;
        _activeRunId = null;
        if (messageId != null && messageId.isNotEmpty) {
          _addMessage(ChatMessage(
            id: messageId,
            sessionKey: sessionKey,
            role: 'assistant',
            content: text,
            createdAt: DateTime.now().millisecondsSinceEpoch.toDouble(),
            seq: 0,
          ));
        }
        _notify();
        break;
      case 'aborted':
        _streaming.remove(sessionKey);
        _sending = false;
        _activeRunId = null;
        _notify();
        break;
      case 'error':
        _streaming.remove(sessionKey);
        _sending = false;
        _activeRunId = null;
        _lastError = payload['error']?.toString() ?? 'run failed';
        _notify();
        break;
      default:
        break;
    }
  }

  void _onSessionMessage(Map<String, dynamic> payload) {
    final raw = payload['message'];
    if (raw is! Map) return;
    final message = ChatMessage.fromJson(raw.cast<String, dynamic>());
    if (message.sessionKey.isEmpty || message.id.isEmpty) return;
    _addMessage(message);
    _notify();
  }

  void _addMessage(ChatMessage message) {
    if (_messageIds.contains(message.id)) return;
    _messageIds.add(message.id);
    final list = _messages.putIfAbsent(message.sessionKey, () => <ChatMessage>[]);
    list.add(message);
    list.sort((a, b) => a.seq.compareTo(b.seq));
  }

  void _onLinkState(GatewayLinkState state) {
    if (state == GatewayLinkState.ready) return;
    if (_link == GatewayLinkState.disconnected) return;
    _link = state;
    if (state == GatewayLinkState.disconnected) {
      _lastError ??= '与网关的连接已断开';
    }
    _notify();
  }

  void _scheduleRefresh() {
    _refreshTimer?.cancel();
    _refreshTimer = Timer(const Duration(milliseconds: 300), () {
      refreshSessions();
    });
  }

  void _notify() {
    if (_disposed) return;
    notifyListeners();
  }
}

String describeError(Object error) {
  if (error is GatewayException) return error.message;
  if (error is TimeoutException) return '连接超时，请检查地址与网络';
  return '$error';
}
