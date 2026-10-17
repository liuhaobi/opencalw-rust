/// Live interoperability test against a running Rust gateway.
///
/// Skipped unless `OPENCLAW_TEST_ENDPOINT` is set:
///
///   OPENCLAW_RUST_TOKEN=test-token cargo run -p openclaw-gateway &
///   OPENCLAW_TEST_ENDPOINT=ws://127.0.0.1:18789/ \
///   OPENCLAW_TEST_TOKEN=test-token flutter test test/live_gateway_test.dart
library;

import 'dart:async';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:openclaw_app/src/protocol/endpoint.dart';
import 'package:openclaw_app/src/protocol/gateway_client.dart';

void main() {
  final rawEndpoint = Platform.environment['OPENCLAW_TEST_ENDPOINT'];
  final token = Platform.environment['OPENCLAW_TEST_TOKEN'] ?? '';

  test('handshake, session create and streamed reply', () async {
    if (rawEndpoint == null || rawEndpoint.isEmpty) {
      markTestSkipped('set OPENCLAW_TEST_ENDPOINT to run the live test');
      return;
    }

    final client = GatewayClient();
    addTearDown(client.dispose);

    final hello = await client.connect(
      endpoint: parseGatewayEndpoint(rawEndpoint),
      token: token,
    );
    expect(hello.protocol, 4);
    expect(hello.methods, contains('chat.send'));

    final sessionKey = 'agent:main:flutter-${DateTime.now().millisecondsSinceEpoch}';
    final created = await client.request('sessions.create', {
      'key': sessionKey,
      'label': 'Flutter live test',
    });
    expect(created['key'], sessionKey);

    final finalReply = Completer<String>();
    final subscription = client.events.listen((event) {
      if (event.event == 'chat' &&
          event.payload['kind'] == 'final' &&
          event.payload['sessionKey'] == sessionKey &&
          !finalReply.isCompleted) {
        finalReply.complete('${event.payload['text'] ?? ''}');
      }
    });
    addTearDown(subscription.cancel);

    final accepted = await client.request('chat.send', {
      'sessionKey': sessionKey,
      'message': 'hello from flutter',
    });
    expect(accepted['accepted'], isTrue);

    final text = await finalReply.future.timeout(const Duration(seconds: 25));
    expect(text, contains('hello from flutter'));

    final history = await client.request('sessions.history', {
      'key': sessionKey,
      'limit': 10,
    });
    final messages = (history['messages'] as List).cast<Map>();
    expect(messages.length, 2);
    expect(messages.first['role'], 'user');
    expect(messages.last['role'], 'assistant');
  }, timeout: const Timeout(Duration(seconds: 60)));
}
