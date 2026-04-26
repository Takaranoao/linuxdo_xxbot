# tg-cron-sender

> Telegram 定时消息发送器 · 用户账号 · 标准 crontab · 支持代理与论坛话题

基于 [grammers](https://codeberg.org/Lonami/grammers)(Rust MTProto 客户端)的极简定时发送工具。一份 `.env` + 一行 `cargo run`,挂在后台按 crontab 自动发送固定文本到任意 chat。

## 特性

- **标准 5 字段 crontab** 表达式(`分 时 日 月 周`,与 Linux 一致)
- **用户账号登录**(非 bot;首次手机+验证码,支持两步验证)
- **四种发送场景**:直发 / 发到论坛 topic / 回复指定消息 / topic 内回复
- **HTTP / SOCKS5 代理**,支持用户名密码认证
- **SQLite session 持久化**,登录一次永久复用
- **Ctrl+C 优雅退出**
- **零运行时配置文件**:全部走 `.env`

## 快速开始

### 1. 准备依赖

```bash
# 克隆本项目
git clone <this repo url> tg-cron-sender
cd tg-cron-sender

# 同级目录克隆 grammers(本项目通过本地 path 引用)
cd ..
git clone https://codeberg.org/Lonami/grammers.git
cd tg-cron-sender
```

> 当前 `Cargo.toml` 中 grammers 路径是绝对路径(`/Users/takaranoao/git/grammers/`)。
> 如果你的 grammers 不在这个位置,改成你本机的实际路径,例如 `../grammers/grammers-client`。

### 2. 创建 Telegram 应用

去 https://my.telegram.org/apps 申请,拿到 `API_ID` 和 `API_HASH`。

### 3. 配置 `.env`

```bash
cp .env.example .env
$EDITOR .env  # 至少填 API_ID / API_HASH / TARGET_CHAT / MESSAGE
```

### 4. 编译运行

```bash
cargo run --release
```

首次启动会交互式提示:

1. 手机号(国际格式,如 `+8613800138000`)
2. Telegram 发来的验证码
3. 如果开了两步验证 → 输入 2FA 密码

成功后 `tg-cron-sender.session` 落盘,后续启动直接进入主循环,不再要求登录。

## 配置项

完整示例见 [`.env.example`](.env.example)。

| 变量 | 必填 | 说明 |
|---|---|---|
| `API_ID` | ✓ | Telegram API ID |
| `API_HASH` | ✓ | Telegram API hash |
| `TARGET_CHAT` | ✓ | `@username` 或数字 chat id(`-1001234567890` 格式;必须在你账号的 dialogs 中) |
| `CRON` | ✓ | 5 字段 crontab,如 `*/5 * * * *`(每 5 分钟) |
| `MESSAGE` | ✓ | 要发送的固定文本 |
| `SESSION_PATH` |  | session 文件路径(默认 `tg-cron-sender.session`) |
| `TARGET_TOPIC_ID` |  | 论坛 topic 根 message id;非论坛留空 |
| `TARGET_REPLY_TO_MSG_ID` |  | 要回复的消息 id;留空表示不作为回复 |
| `TG_PROXY_TYPE` |  | `http` / `socks5` / 留空 |
| `TG_PROXY_HOST` |  | `host:port` 格式 |
| `TG_PROXY_USERNAME` |  | 代理用户名(可选) |
| `TG_PROXY_PASSWORD` |  | 代理密码(可选) |

### 发送行为矩阵

| `TARGET_REPLY_TO_MSG_ID` | `TARGET_TOPIC_ID` | 实际行为 |
|---|---|---|
| 空 | 空 | 直接发到 chat 根流 |
| 空 | 设置 | 发到指定论坛 topic |
| 设置 | 空 | 回复指定消息 |
| 设置 | 设置 | 在 topic 内回复指定消息 |

### crontab 示例

| 表达式 | 触发频率 |
|---|---|
| `*/5 * * * *` | 每 5 分钟 |
| `0 9 * * *` | 每天 09:00 |
| `0 */2 * * *` | 每 2 小时整点 |
| `0 9 * * 1-5` | 工作日 09:00 |
| `30 8,18 * * *` | 每天 08:30 与 18:30 |

时区:**UTC**。如需按本地时间触发,自己换算。

## 开发

```bash
# 跑全部测试(28 单测 + 1 集成测试)
cargo test

# 只跑某模块
cargo test --lib config::

# 只跑集成测试
cargo test --test config_integration

# 静态检查
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

### 项目结构

```
src/
├── lib.rs          # 模块导出
├── main.rs         # 入口、cron 主循环、Ctrl+C 处理
├── config.rs       # .env 解析,纯函数,内联单测
├── proxy.rs        # 代理 URL 构造 + percent-encode,内联单测
├── schedule.rs     # saffron crontab 包装,内联单测
├── target.rs       # @username / 数字 id 解析,内联单测
└── client.rs       # grammers 薄封装,无单测;send() 走 raw messages.SendMessage

tests/
└── config_integration.rs  # 端到端配置 round-trip
```

设计原则:
- 纯逻辑模块走 TDD + 内联单测
- I/O 边界(`client.rs`)缩到最小,靠真账号联调验证,不写 mock
- 所有 cron 字段都是 UTC,避免时区坑

## 要求

- **Rust 1.85+**(grammers 0.9 用 edition 2024)
- 能直连 Telegram 或可用的 http/socks5 代理
- 一个真实的 Telegram 用户账号

## 许可

待定(用户尚未指定)。grammers 自身使用 MIT / Apache-2.0 双协议。
