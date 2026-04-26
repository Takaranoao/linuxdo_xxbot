# tg-cron-sender

最小可用的 Telegram 定时消息发送器,基于 [grammers](https://codeberg.org/Lonami/grammers) (Rust MTProto)。
按标准 5 字段 crontab 表达式发送消息,支持四种发送场景:**直发 / 发到论坛 topic / 回复指定消息 / 在 topic 内回复指定消息**。

## 要求

- Rust 1.85+(grammers 0.9 用 edition 2024)
- 本机克隆好 grammers 仓库到 `/Users/takaranoao/git/grammers/`
- 一个能登录的 Telegram 账号(用户账号,不是 bot)
- 在 https://my.telegram.org/apps 创建应用拿到 `API_ID` / `API_HASH`

## 使用

```bash
cp .env.example .env
# 编辑 .env,至少填 API_ID / API_HASH / TARGET_CHAT / MESSAGE

cargo run --release
```

首次运行交互式提示:
1. 手机号(`+86...` 国际格式)
2. Telegram 发来的验证码
3. 如果开了两步验证,再问一次密码

成功后 `tg-cron-sender.session`(可在 `SESSION_PATH` 改名/路径)落盘,后续启动不再要求登录。

## 环境变量

| 变量 | 必填 | 说明 |
|---|---|---|
| `API_ID` | 是 | Telegram API ID(整数) |
| `API_HASH` | 是 | Telegram API hash |
| `TARGET_CHAT` | 是 | `@username`(公开)或数字 chat id(必须在你账号的 dialogs 列表里) |
| `CRON` | 是 | 5 字段标准 crontab,如 `*/5 * * * *` |
| `MESSAGE` | 是 | 要发的文本 |
| `SESSION_PATH` |  | 默认 `tg-cron-sender.session` |
| `TARGET_TOPIC_ID` |  | 论坛 topic 根 message id;非论坛留空 |
| `TARGET_REPLY_TO_MSG_ID` |  | 要回复的消息 id;留空表示不作为回复 |
| `TG_PROXY_TYPE` |  | `http` / `socks5` / 留空 |
| `TG_PROXY_HOST` |  | `host:port` |
| `TG_PROXY_USERNAME` |  | 代理用户名 |
| `TG_PROXY_PASSWORD` |  | 代理密码 |

## 发送行为矩阵

| `TARGET_REPLY_TO_MSG_ID` | `TARGET_TOPIC_ID` | 行为 |
|---|---|---|
| 空 | 空 | 直接发到 chat 根流 |
| 空 | 设置 | 发到指定论坛 topic |
| 设置 | 空 | 回复指定消息 |
| 设置 | 设置 | 在 topic 内回复指定消息 |

## 测试

```bash
cargo test                           # 全部
cargo test --lib                     # 仅单元测试
cargo test --test config_integration # 仅集成
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

## 目录

- `src/config.rs` `src/proxy.rs` `src/schedule.rs` `src/target.rs` —— 纯逻辑模块,内联单测
- `src/client.rs` —— grammers 薄封装,I/O 边界,无单测;`send()` 走 raw `messages.SendMessage` 以支持 topic + reply 全组合
- `src/main.rs` —— 入口与 cron 主循环
- `tests/config_integration.rs` —— 端到端 config 集成测试
