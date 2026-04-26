use std::collections::HashMap;

use anyhow::{Context, Result, anyhow};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginMethod {
    /// 试 passkey,失败 fallback 到密码(默认)
    Auto,
    /// 强制 passkey,失败 bail
    Passkey,
    /// 跳过 passkey,只用 SMS+可选 2FA 密码
    Password,
}

impl LoginMethod {
    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "" | "auto" => Ok(Self::Auto),
            "passkey" => Ok(Self::Passkey),
            "password" => Ok(Self::Password),
            other => Err(anyhow!(
                "LOGIN_METHOD must be auto|passkey|password, got {other:?}"
            )),
        }
    }
}

/// 单个账号的认证 / session 配置 — 多账号场景下每账号一份。
#[derive(Debug, Clone)]
pub struct AccountConfig {
    pub api_id: i32,
    pub api_hash: String,
    pub session_path: String,
    pub login_method: LoginMethod,
}

/// 网络代理配置(全局,所有账号共用,因为 Telegram dc 路由是全局的)。
#[derive(Debug, Clone, Default)]
pub struct ProxyConfig {
    pub proxy_type: Option<String>,
    pub proxy_host: Option<String>,
    pub proxy_username: Option<String>,
    pub proxy_password: Option<String>,
}

/// MTProto 传输层配置(全局)。当前只有一个 IPv6 socket 开关;
/// 用 grammers 内置的官方 DC 列表,不做地址 / 密钥级覆盖。
#[derive(Debug, Clone, Default)]
pub struct NetworkConfig {
    pub use_ipv6: bool,
}

/// 单个 cron 发送任务 — 多账号场景下也是每账号(或多任务)一份。
#[derive(Debug, Clone)]
pub struct JobConfig {
    pub target_chat: String,
    pub target_topic_id: Option<i32>,
    pub target_reply_to_msg_id: Option<i32>,
    pub cron_expr: String,
    pub message: String,
}

/// 当前 v0.2 单账号容器:account + proxy + network + job 各一份。
/// 未来多账号时,可改成 `Vec<(AccountConfig, JobConfig)> + ProxyConfig + NetworkConfig`。
#[derive(Debug, Clone)]
pub struct Config {
    pub account: AccountConfig,
    pub proxy: ProxyConfig,
    pub network: NetworkConfig,
    pub job: JobConfig,
}

fn get_required<'a>(map: &'a HashMap<String, String>, key: &str) -> Result<&'a str> {
    map.get(key)
        .map(|s| s.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("missing required env: {key}"))
}

fn get_optional<'a>(map: &'a HashMap<String, String>, key: &str) -> Option<&'a str> {
    map.get(key).map(|s| s.as_str()).filter(|s| !s.is_empty())
}

fn parse_bool(s: &str) -> Result<bool> {
    match s.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        other => Err(anyhow!("invalid boolean: {other:?}")),
    }
}

fn parse_optional_i32(map: &HashMap<String, String>, key: &str) -> Result<Option<i32>> {
    match get_optional(map, key) {
        None => Ok(None),
        Some(s) => {
            Ok(Some(s.parse::<i32>().with_context(|| {
                format!("{key} must be int, got {s:?}")
            })?))
        }
    }
}

impl AccountConfig {
    pub fn from_map(map: &HashMap<String, String>) -> Result<Self> {
        let api_id = get_required(map, "API_ID")?
            .parse::<i32>()
            .with_context(|| "API_ID must be a 32-bit integer")?;
        Ok(Self {
            api_id,
            api_hash: get_required(map, "API_HASH")?.to_string(),
            session_path: get_optional(map, "SESSION_PATH")
                .unwrap_or("tg-cron-sender.session")
                .to_string(),
            login_method: LoginMethod::parse(get_optional(map, "LOGIN_METHOD").unwrap_or(""))?,
        })
    }
}

impl ProxyConfig {
    pub fn from_map(map: &HashMap<String, String>) -> Self {
        Self {
            proxy_type: get_optional(map, "TG_PROXY_TYPE").map(str::to_string),
            proxy_host: get_optional(map, "TG_PROXY_HOST").map(str::to_string),
            proxy_username: get_optional(map, "TG_PROXY_USERNAME").map(str::to_string),
            proxy_password: get_optional(map, "TG_PROXY_PASSWORD").map(str::to_string),
        }
    }
}

impl NetworkConfig {
    pub fn from_map(map: &HashMap<String, String>) -> Result<Self> {
        let use_ipv6 = match get_optional(map, "USE_IPV6") {
            None => false,
            Some(s) => parse_bool(s).with_context(|| format!("USE_IPV6 invalid: {s:?}"))?,
        };
        Ok(Self { use_ipv6 })
    }
}

impl JobConfig {
    pub fn from_map(map: &HashMap<String, String>) -> Result<Self> {
        Ok(Self {
            target_chat: get_required(map, "TARGET_CHAT")?.to_string(),
            target_topic_id: parse_optional_i32(map, "TARGET_TOPIC_ID")?,
            target_reply_to_msg_id: parse_optional_i32(map, "TARGET_REPLY_TO_MSG_ID")?,
            cron_expr: get_required(map, "CRON")?.to_string(),
            message: get_required(map, "MESSAGE")?.to_string(),
        })
    }
}

impl Config {
    pub fn from_map(map: &HashMap<String, String>) -> Result<Self> {
        Ok(Self {
            account: AccountConfig::from_map(map)?,
            proxy: ProxyConfig::from_map(map),
            network: NetworkConfig::from_map(map)?,
            job: JobConfig::from_map(map)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn base_map() -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("API_ID".into(), "12345".into());
        m.insert("API_HASH".into(), "abc".into());
        m.insert("TARGET_CHAT".into(), "@foo".into());
        m.insert("CRON".into(), "*/5 * * * *".into());
        m.insert("MESSAGE".into(), "hi".into());
        m
    }

    #[test]
    fn parses_required_fields() {
        let cfg = Config::from_map(&base_map()).unwrap();
        assert_eq!(cfg.account.api_id, 12345);
        assert_eq!(cfg.account.api_hash, "abc");
        assert_eq!(cfg.job.target_chat, "@foo");
        assert_eq!(cfg.job.cron_expr, "*/5 * * * *");
        assert_eq!(cfg.job.message, "hi");
    }

    #[test]
    fn defaults_session_path_when_missing() {
        let cfg = Config::from_map(&base_map()).unwrap();
        assert_eq!(cfg.account.session_path, "tg-cron-sender.session");
    }

    #[test]
    fn missing_api_id_errors() {
        let mut m = base_map();
        m.remove("API_ID");
        let err = Config::from_map(&m).unwrap_err().to_string();
        assert!(err.contains("API_ID"), "got: {err}");
    }

    #[test]
    fn invalid_api_id_errors() {
        let mut m = base_map();
        m.insert("API_ID".into(), "not-a-number".into());
        assert!(Config::from_map(&m).is_err());
    }

    #[test]
    fn empty_topic_id_is_none() {
        let mut m = base_map();
        m.insert("TARGET_TOPIC_ID".into(), "".into());
        let cfg = Config::from_map(&m).unwrap();
        assert_eq!(cfg.job.target_topic_id, None);
    }

    #[test]
    fn topic_id_parsed() {
        let mut m = base_map();
        m.insert("TARGET_TOPIC_ID".into(), "7310786".into());
        let cfg = Config::from_map(&m).unwrap();
        assert_eq!(cfg.job.target_topic_id, Some(7310786));
    }

    #[test]
    fn reply_to_msg_id_parsed() {
        let mut m = base_map();
        m.insert("TARGET_REPLY_TO_MSG_ID".into(), "42".into());
        let cfg = Config::from_map(&m).unwrap();
        assert_eq!(cfg.job.target_reply_to_msg_id, Some(42));
    }

    #[test]
    fn empty_reply_to_msg_id_is_none() {
        let mut m = base_map();
        m.insert("TARGET_REPLY_TO_MSG_ID".into(), "".into());
        let cfg = Config::from_map(&m).unwrap();
        assert_eq!(cfg.job.target_reply_to_msg_id, None);
    }

    #[test]
    fn proxy_fields_optional() {
        let cfg = Config::from_map(&base_map()).unwrap();
        assert!(cfg.proxy.proxy_type.is_none());
        assert!(cfg.proxy.proxy_host.is_none());
    }

    #[test]
    fn proxy_fields_passthrough() {
        let mut m = base_map();
        m.insert("TG_PROXY_TYPE".into(), "socks5".into());
        m.insert("TG_PROXY_HOST".into(), "127.0.0.1:1080".into());
        m.insert("TG_PROXY_USERNAME".into(), "u".into());
        m.insert("TG_PROXY_PASSWORD".into(), "p".into());
        let cfg = Config::from_map(&m).unwrap();
        assert_eq!(cfg.proxy.proxy_type.as_deref(), Some("socks5"));
        assert_eq!(cfg.proxy.proxy_host.as_deref(), Some("127.0.0.1:1080"));
        assert_eq!(cfg.proxy.proxy_username.as_deref(), Some("u"));
        assert_eq!(cfg.proxy.proxy_password.as_deref(), Some("p"));
    }

    #[test]
    fn login_method_default_is_auto() {
        let cfg = Config::from_map(&base_map()).unwrap();
        assert_eq!(cfg.account.login_method, LoginMethod::Auto);
    }

    #[test]
    fn login_method_explicit_auto() {
        let mut m = base_map();
        m.insert("LOGIN_METHOD".into(), "auto".into());
        assert_eq!(
            Config::from_map(&m).unwrap().account.login_method,
            LoginMethod::Auto
        );
    }

    #[test]
    fn login_method_explicit_password() {
        let mut m = base_map();
        m.insert("LOGIN_METHOD".into(), "password".into());
        assert_eq!(
            Config::from_map(&m).unwrap().account.login_method,
            LoginMethod::Password
        );
    }

    #[test]
    fn login_method_passkey() {
        let mut m = base_map();
        m.insert("LOGIN_METHOD".into(), "passkey".into());
        assert_eq!(
            Config::from_map(&m).unwrap().account.login_method,
            LoginMethod::Passkey
        );
    }

    #[test]
    fn login_method_case_insensitive() {
        let mut m = base_map();
        m.insert("LOGIN_METHOD".into(), "Auto".into());
        assert_eq!(
            Config::from_map(&m).unwrap().account.login_method,
            LoginMethod::Auto
        );
    }

    #[test]
    fn login_method_invalid_errors() {
        let mut m = base_map();
        m.insert("LOGIN_METHOD".into(), "totp".into());
        assert!(Config::from_map(&m).is_err());
    }

    #[test]
    fn use_ipv6_defaults_to_false() {
        let cfg = Config::from_map(&base_map()).unwrap();
        assert!(!cfg.network.use_ipv6);
    }

    #[test]
    fn use_ipv6_empty_is_default() {
        let mut m = base_map();
        m.insert("USE_IPV6".into(), "".into());
        assert!(!Config::from_map(&m).unwrap().network.use_ipv6);
    }

    #[test]
    fn use_ipv6_truthy_values() {
        for v in ["true", "1", "yes", "on", "TRUE"] {
            let mut m = base_map();
            m.insert("USE_IPV6".into(), v.into());
            assert!(
                Config::from_map(&m).unwrap().network.use_ipv6,
                "expected USE_IPV6={v} to parse as true"
            );
        }
    }

    #[test]
    fn use_ipv6_falsy_values() {
        for v in ["false", "0", "no", "off", "FALSE"] {
            let mut m = base_map();
            m.insert("USE_IPV6".into(), v.into());
            assert!(
                !Config::from_map(&m).unwrap().network.use_ipv6,
                "expected USE_IPV6={v} to parse as false"
            );
        }
    }

    #[test]
    fn use_ipv6_invalid_errors() {
        let mut m = base_map();
        m.insert("USE_IPV6".into(), "maybe".into());
        let err = Config::from_map(&m).unwrap_err().to_string();
        assert!(err.contains("USE_IPV6"), "got: {err}");
    }
}
