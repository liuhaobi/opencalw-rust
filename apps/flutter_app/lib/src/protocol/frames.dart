/// Wire protocol models, mirroring `crates/openclaw-protocol`.
///
/// Frames are JSON objects discriminated by `type`:
/// `req` (client to server), `res` and `event` (server to client).
library;

/// Raised when the gateway answers a request with `ok: false`.
class GatewayException implements Exception {
  GatewayException({required this.code, required this.message, this.details});

  final String code;
  final String message;
  final Object? details;

  @override
  String toString() => message.isEmpty ? code : '$code: $message';
}

/// Payload of the `res` frame that answers the connect request.
class HelloOk {
  const HelloOk({
    required this.protocol,
    required this.serverVersion,
    required this.bootId,
    required this.connId,
    required this.methods,
    required this.events,
    required this.role,
    required this.scopes,
    required this.authMethod,
    required this.tickIntervalMs,
  });

  final int protocol;
  final String serverVersion;
  final String bootId;
  final String connId;
  final List<String> methods;
  final List<String> events;
  final String role;
  final List<String> scopes;
  final String authMethod;
  final int tickIntervalMs;

  factory HelloOk.fromJson(Map<String, dynamic> json) {
    final server = _map(json['server']);
    final features = _map(json['features']);
    final auth = _map(json['auth']);
    final policy = _map(json['policy']);
    return HelloOk(
      protocol: _int(json['protocol']),
      serverVersion: '${server['version'] ?? 'unknown'}',
      bootId: '${server['bootId'] ?? ''}',
      connId: '${server['connId'] ?? ''}',
      methods: _strings(features['methods']),
      events: _strings(features['events']),
      role: '${auth['role'] ?? 'operator'}',
      scopes: _strings(auth['scopes']),
      authMethod: '${auth['method'] ?? 'none'}',
      tickIntervalMs: _int(policy['tickIntervalMs'], fallback: 30000),
    );
  }
}

/// A server-pushed event frame.
class GatewayEvent {
  const GatewayEvent({
    required this.event,
    required this.payload,
    required this.seq,
  });

  final String event;
  final Map<String, dynamic> payload;
  final int? seq;

  factory GatewayEvent.fromJson(Map<String, dynamic> json) {
    return GatewayEvent(
      event: '${json['event'] ?? ''}',
      payload: _map(json['payload']),
      seq: json['seq'] is num ? (json['seq'] as num).toInt() : null,
    );
  }
}

/// Connection phase reported by [GatewayClient].
enum GatewayLinkState { disconnected, connecting, ready }

Map<String, dynamic> _map(Object? value) {
  if (value is Map<String, dynamic>) return value;
  if (value is Map) return value.cast<String, dynamic>();
  return const <String, dynamic>{};
}

int _int(Object? value, {int fallback = 0}) {
  if (value is num) return value.toInt();
  if (value is String) return int.tryParse(value) ?? fallback;
  return fallback;
}

List<String> _strings(Object? value) {
  if (value is List) return value.map((item) => '$item').toList();
  return const <String>[];
}
