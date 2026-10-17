/// Session and transcript projections returned by the gateway.
library;

class SessionRow {
  const SessionRow({
    required this.key,
    this.sessionId,
    this.kind = 'direct',
    this.label,
    this.preview,
    this.updatedAt,
    this.createdAt,
    this.status,
    this.archived = false,
  });

  final String key;
  final String? sessionId;
  final String kind;
  final String? label;
  final String? preview;
  final double? updatedAt;
  final double? createdAt;
  final String? status;
  final bool archived;

  String get displayName {
    final value = label?.trim();
    if (value != null && value.isNotEmpty) return value;
    final parts = key.split(':');
    final tail = parts.isEmpty ? key : parts.last;
    return tail.isEmpty ? key : tail;
  }

  String get initial {
    final runes = displayName.trim().runes;
    if (runes.isEmpty) return '#';
    return String.fromCharCode(runes.first).toUpperCase();
  }

  DateTime? get updatedDateTime {
    final value = updatedAt;
    if (value == null) return null;
    return DateTime.fromMillisecondsSinceEpoch(value.toInt());
  }

  factory SessionRow.fromJson(Map<String, dynamic> json) {
    return SessionRow(
      key: '${json['key'] ?? ''}',
      sessionId: json['sessionId']?.toString(),
      kind: '${json['kind'] ?? 'direct'}',
      label: json['label']?.toString(),
      preview: json['lastMessagePreview']?.toString(),
      updatedAt: _double(json['updatedAt']),
      createdAt: _double(json['createdAt']),
      status: json['status']?.toString(),
      archived: json['archived'] == true,
    );
  }
}

class ChatMessage {
  const ChatMessage({
    required this.id,
    required this.sessionKey,
    required this.role,
    required this.content,
    required this.createdAt,
    required this.seq,
  });

  final String id;
  final String sessionKey;
  final String role;
  final String content;
  final double createdAt;
  final int seq;

  bool get isUser => role == 'user';
  bool get isAssistant => role == 'assistant';

  factory ChatMessage.fromJson(Map<String, dynamic> json) {
    return ChatMessage(
      id: '${json['id'] ?? ''}',
      sessionKey: '${json['sessionKey'] ?? ''}',
      role: '${json['role'] ?? 'user'}',
      content: '${json['content'] ?? ''}',
      createdAt: _double(json['createdAt']) ?? 0,
      seq: (json['seq'] as num?)?.toInt() ?? 0,
    );
  }
}

double? _double(Object? value) {
  if (value is num) return value.toDouble();
  if (value is String) return double.tryParse(value);
  return null;
}
