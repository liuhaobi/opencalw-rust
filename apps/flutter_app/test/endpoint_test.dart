import 'package:flutter_test/flutter_test.dart';
import 'package:openclaw_app/src/protocol/endpoint.dart';

void main() {
  group('parseGatewayEndpoint', () {
    test('defaults to localhost when empty', () {
      final uri = parseGatewayEndpoint('   ');
      expect(uri.toString(), 'ws://127.0.0.1:18789/');
    });

    test('adds ws scheme for bare host:port', () {
      final uri = parseGatewayEndpoint('192.168.1.5:18789');
      expect(uri.scheme, 'ws');
      expect(uri.host, '192.168.1.5');
      expect(uri.port, 18789);
      expect(uri.path, '/');
    });

    test('maps https to wss and keeps path', () {
      final uri = parseGatewayEndpoint('https://claw.example.com/gateway');
      expect(uri.scheme, 'wss');
      expect(uri.host, 'claw.example.com');
      expect(uri.path, '/gateway');
    });

    test('keeps explicit wss', () {
      final uri = parseGatewayEndpoint('wss://host/gateway');
      expect(uri.scheme, 'wss');
      expect(uri.path, '/gateway');
    });
  });
}
