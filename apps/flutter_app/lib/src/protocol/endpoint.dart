/// Normalises user-entered gateway addresses.
///
/// Accepts `host:port`, `ws://host:port`, `wss://host/path`, and the HTTP
/// spellings (`http://`, `https://`) people often paste from a browser.
Uri parseGatewayEndpoint(String input) {
  var text = input.trim();
  if (text.isEmpty) {
    text = 'ws://127.0.0.1:18789/';
  }
  if (!text.contains('://')) {
    text = 'ws://$text';
  }

  final uri = Uri.parse(text);
  final scheme = switch (uri.scheme) {
    'wss' || 'https' => 'wss',
    'ws' || 'http' => 'ws',
    _ => 'ws',
  };
  final path = uri.path.isEmpty ? '/' : uri.path;
  return uri.replace(scheme: scheme, path: path);
}
