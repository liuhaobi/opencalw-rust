# opencalw-rust

OpenClaw 网关的 Rust 重写实现，协议对齐上游 `openclaw/openclaw` 的 Gateway Protocol v4。

上游是 42 万 KB TypeScript 的 pnpm monorepo（Gateway + 20+ 渠道 + 插件生态 + macOS/iOS/Android 客户端），无法一次性整体重写。本仓库采用**渐进式重构**：先用 Rust 落地控制平面核心（协议、握手、鉴权、会话、Agent 循环、事件总线），再按模块替换外围能力。

## 已实现

| 能力 | 说明 |
|---|---|
| 帧协议 | `req` / `res` / `event` 三种 JSON 帧，camelCase 序列化，与上游同构 |
| 握手 | 首帧必须是 `req#connect`，校验 `minProtocol/maxProtocol`，回 `hello-ok`（含 snapshot、features、policy） |
| 鉴权 | token / password / 显式关闭三种模式，常量时间比较；无凭据默认拒绝启动 |
| 会话存储 | SQLite（WAL），会话行 + 追加式消息，`last_seq` 保证单会话内单调 |
| Agent 循环 | 落库用户消息 → 流式生成 → 落库助手消息 → 广播 `chat` / `session.message` / `sessions.changed` |
| 模型调用 | OpenAI 兼容 `chat/completions`（SSE 增量解析）；无 API Key 时启用本地 echo 传输，开箱可跑 |
| 事件总线 | broadcast + 全局 seq，每连接一个写入泵（响应与事件共用 FIFO，避免互相饿死） |
| HTTP 面 | `/health` `/healthz` `/ready` `/readyz` `/startup` `/startupz`、`/v1/models`、`/v1/chat/completions`（流式与非流式） |
| RPC 方法 | `health` `status` `config.get` `sessions.list/create/get/history/delete` `chat.send` `chat.abort` |

## 未实现（后续路线）

- 渠道适配器（WhatsApp / Telegram / Slack / Discord / Signal / iMessage 等 20+）
- 插件 SDK 与 ClawHub 生态、Skills、工具执行（bash / 文件 / 浏览器 / 沙箱）
- Control UI 静态资源托管与插件资产路由
- 设备/节点配对（join code、SSH 校验、device token 签发）、多用户与 profiles
- 远端 node host 调度（上游 `crates/openclaw-node-host` 可直接复用为对端）
- 定时任务、看板、ACP、语音 / Canvas / 摄像头等节点能力

## 快速开始

```bash
# 必须显式给出凭据（或本地测试时显式允许匿名）
export OPENCLAW_RUST_TOKEN=dev-token

cargo run -p openclaw-gateway
```

启动后：

```bash
curl -s http://127.0.0.1:18789/health
node scripts/smoke.mjs ws://127.0.0.1:18789/ dev-token
```

接入真实模型：

```bash
export OPENCLAW_RUST_MODEL_API_KEY=sk-...
export OPENCLAW_RUST_MODEL_BASE_URL=https://api.openai.com/v1
export OPENCLAW_RUST_MODEL=gpt-4o-mini
```

### 配置项

| 变量 | 默认值 | 说明 |
|---|---|---|
| `OPENCLAW_RUST_BIND` | `127.0.0.1` | 监听地址 |
| `OPENCLAW_RUST_PORT` | `18789` | 控制平面端口 |
| `OPENCLAW_RUST_TOKEN` | 无 | 网关 token（与 `OPENCLAW_RUST_PASSWORD` 二选一） |
| `OPENCLAW_RUST_PASSWORD` | 无 | 网关密码 |
| `OPENCLAW_RUST_ALLOW_ANONYMOUS` | `false` | 本地测试时允许无鉴权启动 |
| `OPENCLAW_RUST_STATE_DIR` | `~/.openclaw-rust` | 状态目录（内含 `state.db`） |
| `OPENCLAW_RUST_MODEL_BASE_URL` | `https://api.openai.com/v1` | OpenAI 兼容端点 |
| `OPENCLAW_RUST_MODEL_API_KEY` | 回退 `OPENAI_API_KEY` | 未设置则使用 echo 传输 |
| `OPENCLAW_RUST_MODEL` | `gpt-4o-mini` | 模型名 |
| `OPENCLAW_RUST_MAX_HISTORY` | `30` | 单轮携带的历史条数 |
| `OPENCLAW_RUST_MAX_PAYLOAD` | `4194304` | 单帧上限（hello 中通告） |

## 协议示例

连接后首帧：

```json
{"type":"req","id":"c1","method":"connect","params":{
  "minProtocol":4,"maxProtocol":4,
  "client":{"id":"smoke-cli","version":"0.0.0","platform":"darwin","mode":"cli"},
  "auth":{"token":"dev-token"}
}}
```

服务端回：

```json
{"type":"res","id":"c1","ok":true,"payload":{"type":"hello-ok","protocol":4}}
```

发起对话（立即返回 `runId`，正文通过事件流式推送）：

```json
{"type":"req","id":"s1","method":"chat.send","params":{"sessionKey":"agent:main:main","message":"hi"}}
```

事件：`chat`（`kind`: `start` / `delta` / `final` / `aborted` / `error`）、`session.message`、`sessions.changed`、`tick`、`presence`。

## 与上游模块的对应关系

| Rust | openclaw（TypeScript） |
|---|---|
| `crates/openclaw-protocol/src/frames.rs` | `packages/gateway-protocol/src/schema/frames.ts` |
| `crates/openclaw-protocol/src/connect.rs` | 同上（`ConnectParamsSchema` / `HelloOkSchema`） |
| `crates/openclaw-protocol/src/snapshot.rs` | `packages/gateway-protocol/src/schema/snapshot.ts` |
| `crates/openclaw-gateway/src/ws.rs` | `src/gateway/server-http-upgrades.ts`、`server/ws-connection/message-handler.ts` |
| `crates/openclaw-gateway/src/auth.rs` | `server/ws-connection/connect-auth.ts`、`src/gateway/auth.ts` |
| `crates/openclaw-gateway/src/methods.rs` | `src/gateway/server-methods.ts`、`methods/core-descriptors.ts` |
| `crates/openclaw-gateway/src/events.rs` | `src/gateway/server-broadcast.ts` |
| `crates/openclaw-gateway/src/store.rs` | `src/state/openclaw-state-db*.ts`、`src/sessions/session-state-events.kernel.ts` |
| `crates/openclaw-gateway/src/agent.rs` | `packages/agent-core/src/agent-loop.ts` |
| `crates/openclaw-gateway/src/llm.rs` | `packages/llm-core/src/types.ts`（`StreamFn`） |
| `crates/openclaw-gateway/src/http.rs` | `gateway-http-route-contracts.ts`、`openai-compatible-http.ts` |

## 测试

```bash
cargo test            # 单元测试 + 真实 WebSocket 端到端测试
cargo clippy --all-targets
```

`tests/ws_protocol.rs` 用真实监听端口与 WebSocket 客户端覆盖：握手、错误 token、未知方法、`sessions.create`、`chat.send` 流式收尾、`sessions.history`。

## 设计取舍

- **单一写入队列**：响应与事件共用一条 FIFO，避免事件洪峰把响应饿死；这也是上游 `server-broadcast` 的意图。
- **帧必须经 `GatewayFrame` 包装**：`res` 顶层必须带 `type:"res"`。直接序列化 `ResponseFrame` 会丢判别字段，集成测试专门锁定这一点。
- **无 Key 可运行**：没有模型凭据时走 echo 传输，协议层与 UI 联调不需要外部依赖。
- **默认拒绝无鉴权启动**：与上游“入站即不可信”的安全取向一致，避免误把网关暴露成开放代理。

## 许可

MIT
