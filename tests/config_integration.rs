use std::collections::HashMap;

use pretty_assertions::assert_eq;
use tg_cron_sender::config::{Config, LoginMethod};
use tg_cron_sender::proxy::build_proxy_url;

fn full_env() -> HashMap<String, String> {
    [
        ("API_ID", "10000"),
        ("API_HASH", "deadbeef"),
        ("SESSION_PATH", "/tmp/tg.session"),
        ("TARGET_CHAT", "-1001680975844"),
        ("TARGET_TOPIC_ID", "7310786"),
        ("TARGET_REPLY_TO_MSG_ID", "98765"),
        ("CRON", "0 9 * * *"),
        ("MESSAGE", "good morning"),
        ("LOGIN_METHOD", "passkey"),
        ("TG_PROXY_TYPE", "socks5"),
        ("TG_PROXY_HOST", "127.0.0.1:1080"),
        ("TG_PROXY_USERNAME", "user"),
        ("TG_PROXY_PASSWORD", "p@ss"),
        ("USE_IPV6", "true"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect()
}

#[test]
fn full_env_round_trip() {
    let map = full_env();
    let cfg = Config::from_map(&map).unwrap();
    assert_eq!(cfg.account.api_id, 10000);
    assert_eq!(cfg.account.session_path, "/tmp/tg.session");
    assert_eq!(cfg.account.login_method, LoginMethod::Passkey);
    assert_eq!(cfg.job.target_topic_id, Some(7310786));
    assert_eq!(cfg.job.target_reply_to_msg_id, Some(98765));
    let url = build_proxy_url(
        cfg.proxy.proxy_type.as_deref(),
        cfg.proxy.proxy_host.as_deref(),
        cfg.proxy.proxy_username.as_deref(),
        cfg.proxy.proxy_password.as_deref(),
    )
    .unwrap();
    assert_eq!(url.as_deref(), Some("socks5://user:p%40ss@127.0.0.1:1080"));
    assert!(cfg.network.use_ipv6);
}
