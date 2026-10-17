/// WebSocket client for the OpenClaw gateway protocol.
///
/// Uses `dart:io` so the same implementation works on OpenHarmony, Android and
/// iOS without a third-party transport package. One socket carries requests,
/// responses and pushed events; responses are matched to callers by frame id.
library;

import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'frames.dart';

class GatewayClient {
  GatewayClient({
    this.clientId = 'openclaw-flutter',
    this.clientVersion = '0.1.0',
    this.requestTimeout = const Duration(seconds: 30),
  });

  final String clientId;
  final String clientVersion;
  final Duration requestTimeout;

  final Map<String, Completer<Map<String, dynamic>>> _pending =
      <String, Completer<Map<String, dynamic>>>{};
  final StreamController<GatewayEvent> _events =
      StreamController<GatewayEvent>.broadcast();
  final StreamController<GatewayLinkState> _linkStates =
      StreamController<GatewayLinkState>.broadcast();

  WebSocket? _socket;
  StreamSubscription<dynamic>? _subscription;
  int _nextId = 0;
  bool _closedByUser = false;
  HelloOk? _hello;

  HelloOk? get hello => _hello;
  Stream<GatewayEvent> get events => _events.stream;
  Stream<GatewayLinkState> get linkStates => _linkStates.stream;
  bool get isConnected => _socket != null;

  /// Opens the socket, performs the connect handshake and returns `hello-ok`.
  Future<HelloOk> connect({
    required Uri endpoint,
    required String token,
  }) async {
    await close();
    _closedByUser = false;
    _linkStates.add(GatewayLinkState.connecting);

    final socket = await WebSocket.connect(endpoint.toString())
        .timeout(const Duration(seconds: 15));
    _socket = socket;
    _subscription = socket.listen(
      _onData,
      onError: _onSocketError,
      onDone: _onSocketDone,
      cancelOnError: false,
    );

    final payload = await request('connect', <String, dynamic>{
      'minProtocol': 4,
      'maxProtocol': 4,
      'client': <String, dynamic>{
        'id': clientId,
        'version': clientVersion,
        'platform': Platform.operatingSystem,
        'mode': 'app',
        'displayName': 'OpenClaw Flutter',
      },
      'caps': <String>['chat', 'sessions'],
      'auth': <String, dynamic>{'token': token},
    }).timeout(const Duration(seconds: 20));

    if (payload['type'] != 'hello-ok') {
      throw GatewayException(
        code: 'handshake_failed',
        message: 'gateway did not return hello-ok',
      );
    }

    _hello = HelloOk.fromJson(payload);
    _linkStates.add(GatewayLinkState.ready);
    return _hello!;
  }

  /// Sends a `req` frame and resolves with the `payload` of the matching `res`.
  Future<Map<String, dynamic>> request(
    String method, [
    Map<String, dynamic>? params,
  ]) {
    final socket = _socket;
    if (socket == null) {
      throw GatewayException(
        code: 'not_connected',
        message: 'gateway socket is not connected',
      );
    }

    final id = 'r${_nextId++}';
    final completer = Completer<Map<String, dynamic>>();
    _pending[id] = completer;

    socket.add(jsonEncode(<String, dynamic>{
      'type': 'req',
      'id': id,
      'method': method,
      if (params != null) 'params': params,
    }));

    return completer.future.timeout(requestTimeout, onTimeout: () {
      _pending.remove(id);
      throw GatewayException(
        code: 'timeout',
        message: 'request "$method" timed out',
      );
    });
  }

  Future<void> close() async {
    _closedByUser = true;
    _linkStates.add(GatewayLinkState.disconnected);
    _failPending(
      GatewayException(code: 'closed', message: 'gateway connection closed'),
    );
    await _subscription?.cancel();
    _subscription = null;
    await _socket?.close(WebSocketStatus.normalClosure);
    _socket = null;
    _hello = null;
  }

  /// Releases stream controllers. Call once, when the owning scope is disposed.
  Future<void> dispose() async {
    await close();
    await _events.close();
    await _linkStates.close();
  }

  void _onData(dynamic raw) {
    if (raw is! String) return;

    dynamic decoded;
    try {
      decoded = jsonDecode(raw);
    } on FormatException {
      return;
    }
    if (decoded is! Map) return;
    final frame = decoded.cast<String, dynamic>();

    switch (frame['type']) {
      case 'res':
        final id = '${frame['id']}';
        final completer = _pending.remove(id);
        if (completer == null || completer.isCompleted) return;
        if (frame['ok'] == true) {
          final payload = frame['payload'];
          completer.complete(
            payload is Map
                ? payload.cast<String, dynamic>()
                : <String, dynamic>{},
          );
        } else {
          final error = frame['error'];
          final errorMap =
              error is Map ? error.cast<String, dynamic>() : <String, dynamic>{};
          completer.completeError(GatewayException(
            code: '${errorMap['code'] ?? 'error'}',
            message: '${errorMap['message'] ?? 'unknown gateway error'}',
            details: errorMap['details'],
          ));
        }
        break;
      case 'event':
        if (!_events.isClosed) {
          _events.add(GatewayEvent.fromJson(frame));
        }
        break;
      default:
        break;
    }
  }

  void _onSocketError(Object error) {
    _failPending(
      GatewayException(code: 'socket_error', message: '$error'),
    );
    _linkStates.add(GatewayLinkState.disconnected);
  }

  void _onSocketDone() {
    _socket = null;
    _failPending(
      GatewayException(
        code: _closedByUser ? 'closed' : 'socket_closed',
        message: 'gateway connection closed',
      ),
    );
    if (!_closedByUser) {
      _linkStates.add(GatewayLinkState.disconnected);
    }
  }

  void _failPending(GatewayException error) {
    final pending = Map<String, Completer<Map<String, dynamic>>>.from(_pending);
    _pending.clear();
    for (final completer in pending.values) {
      if (!completer.isCompleted) completer.completeError(error);
    }
  }
}
