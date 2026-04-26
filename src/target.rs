use anyhow::{Result, anyhow, bail};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Username(String),
    ChatId(i64),
}

pub fn parse_target(s: &str) -> Result<Target> {
    let s = s.trim();
    if s.is_empty() {
        bail!("TARGET_CHAT is empty");
    }
    if let Some(rest) = s.strip_prefix('@') {
        if rest.is_empty() {
            bail!("TARGET_CHAT '@' without name");
        }
        return Ok(Target::Username(rest.to_string()));
    }
    let id: i64 = s
        .parse()
        .map_err(|_| anyhow!("TARGET_CHAT must be @username or numeric id, got {s:?}"))?;
    Ok(Target::ChatId(id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn username_with_at() {
        assert_eq!(
            parse_target("@some_channel").unwrap(),
            Target::Username("some_channel".into())
        );
    }

    #[test]
    fn channel_id() {
        assert_eq!(
            parse_target("-1001680975844").unwrap(),
            Target::ChatId(-1001680975844)
        );
    }

    #[test]
    fn user_id_positive() {
        assert_eq!(parse_target("12345").unwrap(), Target::ChatId(12345));
    }

    #[test]
    fn empty_errors() {
        assert!(parse_target("").is_err());
    }

    #[test]
    fn at_only_errors() {
        assert!(parse_target("@").is_err());
    }

    #[test]
    fn garbage_errors() {
        assert!(parse_target("not a chat").is_err());
    }
}
