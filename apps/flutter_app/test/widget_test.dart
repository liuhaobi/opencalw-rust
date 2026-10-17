import 'package:flutter_test/flutter_test.dart';
import 'package:openclaw_app/src/state/gateway_controller.dart';
import 'package:openclaw_app/src/theme.dart';
import 'package:openclaw_app/src/ui/connect_page.dart';
import 'package:flutter/material.dart';

void main() {
  testWidgets('connect page renders endpoint and token fields',
      (WidgetTester tester) async {
    final controller = GatewayController();
    addTearDown(controller.dispose);

    await tester.pumpWidget(MaterialApp(
      theme: buildOpenClawTheme(),
      home: ConnectPage(controller: controller),
    ));

    expect(find.text('OpenClaw'), findsOneWidget);
    expect(find.text('网关地址'), findsOneWidget);
    expect(find.text('访问令牌'), findsOneWidget);
    expect(find.text('连接'), findsOneWidget);
  });
}
