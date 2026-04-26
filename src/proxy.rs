use anyhow::{Result, anyhow, bail};

const RESERVED: &[u8] = b":/?#[]@!$&'()*+,;= ";

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else if RESERVED.contains(&b) || !b.is_ascii() {
            out.push_str(&format!("%{b:02X}"));
        } else {
            out.push(b as char);
        }
    }
    out
}

pub fn build_proxy_url(
    proxy_type: Option<&str>,
    host: Option<&str>,
    username: Option<&str>,
    password: Option<&str>,
) -> Result<Option<String>> {
    let Some(t) = proxy_type else { return Ok(None) };
    let scheme = match t.to_ascii_lowercase().as_str() {
        "http" => "http",
        "socks5" => "socks5",
        other => bail!("unsupported TG_PROXY_TYPE: {other}"),
    };
    let host = host.ok_or_else(|| anyhow!("TG_PROXY_HOST required when TG_PROXY_TYPE set"))?;
    match (username, password) {
        (None, None) => Ok(Some(format!("{scheme}://{host}"))),
        (Some(u), Some(p)) => Ok(Some(format!(
            "{scheme}://{}:{}@{host}",
            percent_encode(u),
            percent_encode(p)
        ))),
        _ => bail!("TG_PROXY_USERNAME and TG_PROXY_PASSWORD must be set together"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn none_when_no_type() {
        assert_eq!(build_proxy_url(None, None, None, None).unwrap(), None);
    }

    #[test]
    fn socks5_no_creds() {
        let url = build_proxy_url(Some("socks5"), Some("127.0.0.1:1080"), None, None).unwrap();
        assert_eq!(url.as_deref(), Some("socks5://127.0.0.1:1080"));
    }

    #[test]
    fn http_no_creds() {
        let url = build_proxy_url(Some("http"), Some("proxy.example:8080"), None, None).unwrap();
        assert_eq!(url.as_deref(), Some("http://proxy.example:8080"));
    }

    #[test]
    fn socks5_with_creds() {
        let url = build_proxy_url(Some("socks5"), Some("h:1"), Some("user"), Some("pass")).unwrap();
        assert_eq!(url.as_deref(), Some("socks5://user:pass@h:1"));
    }

    #[test]
    fn special_chars_percent_encoded() {
        let url = build_proxy_url(Some("http"), Some("h:1"), Some("u@b"), Some("p:w")).unwrap();
        assert_eq!(url.as_deref(), Some("http://u%40b:p%3Aw@h:1"));
    }

    #[test]
    fn invalid_type_errors() {
        assert!(build_proxy_url(Some("ftp"), Some("h:1"), None, None).is_err());
    }

    #[test]
    fn type_without_host_errors() {
        assert!(build_proxy_url(Some("socks5"), None, None, None).is_err());
    }

    #[test]
    fn one_sided_creds_errors() {
        assert!(build_proxy_url(Some("socks5"), Some("h:1"), Some("u"), None).is_err());
        assert!(build_proxy_url(Some("socks5"), Some("h:1"), None, Some("p")).is_err());
    }
}
