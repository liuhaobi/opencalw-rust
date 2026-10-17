import 'package:flutter/material.dart';

import 'src/state/gateway_controller.dart';
import 'src/theme.dart';
import 'src/ui/connect_page.dart';
import 'src/ui/home_page.dart';

void main() {
  runApp(const OpenClawApp());
}

class OpenClawApp extends StatefulWidget {
  const OpenClawApp({super.key});

  @override
  State<OpenClawApp> createState() => _OpenClawAppState();
}

class _OpenClawAppState extends State<OpenClawApp> {
  final GatewayController _controller = GatewayController();

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'OpenClaw',
      debugShowCheckedModeBanner: false,
      theme: buildOpenClawTheme(),
      home: AnimatedBuilder(
        animation: _controller,
        builder: (context, _) {
          if (_controller.isReady) {
            return HomePage(
              key: const ValueKey('home'),
              controller: _controller,
            );
          }
          return ConnectPage(
            key: const ValueKey('connect'),
            controller: _controller,
          );
        },
      ),
    );
  }
}
