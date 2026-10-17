#!/usr/bin/env node
// End-to-end smoke test for the Rust gateway wire protocol.
//
// Usage:
//   OPENCLAW_RUST_TOKEN=test-token cargo run -p openclaw-gateway &
//   node scripts/smoke.mjs ws://127.0.0.1:18789/ test-token
//
// Requires Node 22+ (global WebSocket).

const url = process.argv[2] ?? "ws://127.0.0.1:18789/";
const token = process.argv[3] ?? process.env.OPENCLAW_RUST_TOKEN ?? "";
const sessionKey = `agent:main:smoke-${Date.now()}`;

const socket = new WebSocket(url);
let finished = false;

function send(id, method, params) {
  socket.send(JSON.stringify({ type: "req", id, method, params }));
}

function fail(message) {
  console.error(`FAIL ${message}`);
  process.exit(1);
}

socket.onopen = () => {
  send("connect-1", "connect", {
    minProtocol: 4,
    maxProtocol: 4,
    client: { id: "smoke-cli", version: "0.0.0", platform: process.platform, mode: "cli" },
    auth: { token },
  });
};

socket.onerror = (event) => fail(`socket error: ${event.message ?? "unknown"}`);

socket.onmessage = (event) => {
  const frame = JSON.parse(event.data);
  if (frame.type === "event" && frame.event === "tick") return;

  if (frame.type === "res" && frame.id === "connect-1") {
    if (!frame.ok) fail(`connect rejected: ${JSON.stringify(frame.error)}`);
    if (frame.payload?.type !== "hello-ok") fail("connect payload is not hello-ok");
    if (frame.payload.protocol !== 4) fail(`unexpected protocol ${frame.payload.protocol}`);
    console.log(`ok hello-ok methods=${frame.payload.features.methods.length}`);
    send("create-1", "sessions.create", { key: sessionKey, label: "Smoke" });
    return;
  }

  if (frame.type === "res" && frame.id === "create-1") {
    if (!frame.ok) fail(`sessions.create failed: ${JSON.stringify(frame.error)}`);
    console.log(`ok sessions.create ${frame.payload.key}`);
    send("send-1", "chat.send", { sessionKey, message: "hello from smoke test" });
    return;
  }

  if (frame.type === "res" && frame.id === "send-1") {
    if (!frame.ok) fail(`chat.send failed: ${JSON.stringify(frame.error)}`);
    console.log(`ok chat.send run=${frame.payload.runId}`);
    return;
  }

  if (frame.type === "event" && frame.event === "chat" && frame.payload.kind === "final") {
    if (!frame.payload.text.includes("hello from smoke test")) {
      fail("final reply did not include the prompt");
    }
    console.log(`ok chat final (${frame.payload.text.length} chars)`);
    finished = true;
    send("history-1", "sessions.history", { key: sessionKey, limit: 10 });
    return;
  }

  if (frame.type === "res" && frame.id === "history-1") {
    if (!frame.ok) fail(`sessions.history failed: ${JSON.stringify(frame.error)}`);
    const roles = frame.payload.messages.map((message) => message.role).join(",");
    if (roles !== "user,assistant") fail(`unexpected transcript: ${roles}`);
    console.log(`ok sessions.history roles=${roles}`);
    console.log("PASS");
    process.exit(0);
  }
};

setTimeout(() => {
  if (!finished) fail(`timeout after 20s`);
}, 20_000);
