//! 登录链路 — 从 ClientHandle 解耦出来,接受 `&Client` 参数,
//! 这样多账号 / 多 binary 场景里可以独立调用。

use std::io::{self, BufRead, Write};

#[cfg(feature = "passkey-login")]
use anyhow::Context;
use anyhow::{Result, anyhow, bail};
use grammers_client::{Client, SignInError};
#[cfg(feature = "passkey-login")]
use grammers_tl_types as tl;

use crate::config::LoginMethod;
#[cfg(feature = "passkey-login")]
use crate::passkey;

/// 确保 `client` 已经登录;按 `method` 分发到 password / passkey / auto 路径。
///
/// 成功时,该 client 背后的 SQLite session 已经持久化了 auth_key —
/// 之后用同一 session_path 创建新 Client 会跳过认证。
///
/// 与具体运行时配置(cron / target / Config)无关:只接受逐账号的值类型,
/// 多账号场景里同一函数可以驱动每个账号的登录。
pub async fn ensure_logged_in(
    client: &Client,
    api_id: i32,
    api_hash: &str,
    method: LoginMethod,
) -> Result<()> {
    if client.is_authorized().await? {
        return Ok(());
    }
    match method {
        LoginMethod::Password => login_with_password(client, api_hash).await,
        LoginMethod::Passkey => {
            #[cfg(feature = "passkey-login")]
            {
                login_with_passkey(client, api_id, api_hash).await
            }
            #[cfg(not(feature = "passkey-login"))]
            {
                let _ = (api_id, api_hash);
                Err(anyhow!(
                    "LOGIN_METHOD=passkey requires passkey-login feature;                      rebuild with `cargo build --features passkey-login`                      or set LOGIN_METHOD=password"
                ))
            }
        }
        LoginMethod::Auto => {
            #[cfg(feature = "passkey-login")]
            {
                log::info!(
                    "LOGIN_METHOD=auto: attempting passkey first (set LOGIN_METHOD=password to skip)"
                );
                match login_with_passkey(client, api_id, api_hash).await {
                    Ok(()) => Ok(()),
                    Err(e) => {
                        log::warn!("passkey login failed ({e}); falling back to SMS+password");
                        login_with_password(client, api_hash).await
                    }
                }
            }
            #[cfg(not(feature = "passkey-login"))]
            {
                let _ = api_id;
                log::info!(
                    "LOGIN_METHOD=auto with passkey-login feature disabled at compile time; using password"
                );
                login_with_password(client, api_hash).await
            }
        }
    }
}

async fn login_with_password(client: &Client, api_hash: &str) -> Result<()> {
    let phone = prompt("Phone number (international format, e.g. +1...): ")?;
    let token = client.request_login_code(phone.trim(), api_hash).await?;
    let code = prompt("Login code: ")?;
    match client.sign_in(&token, code.trim()).await {
        Ok(_) => Ok(()),
        Err(SignInError::PasswordRequired(pwd_token)) => {
            let hint = pwd_token.hint().unwrap_or("(no hint)");
            let pw = prompt(&format!("2FA password (hint: {hint}): "))?;
            client
                .check_password(pwd_token, pw.trim())
                .await
                .map_err(|e| anyhow!("2FA failed: {e}"))?;
            Ok(())
        }
        Err(SignInError::SignUpRequired) => bail!("account requires sign-up"),
        Err(other) => Err(anyhow!("sign_in failed: {other}")),
    }
}

#[cfg(feature = "passkey-login")]
async fn login_with_passkey(client: &Client, api_id: i32, api_hash: &str) -> Result<()> {
    let init: tl::enums::auth::PasskeyLoginOptions = client
        .invoke(&tl::functions::auth::InitPasskeyLogin {
            api_id,
            api_hash: api_hash.to_string(),
        })
        .await
        .map_err(|e| anyhow!("auth.initPasskeyLogin failed: {e}"))?;
    let options_json = match init {
        tl::enums::auth::PasskeyLoginOptions::Options(o) => match o.options {
            tl::enums::DataJson::Json(d) => d.data,
        },
    };
    log::debug!("passkey options JSON: {options_json}");

    let opts = passkey::PasskeyOptions::from_json(&options_json)
        .with_context(|| "parsing PublicKeyCredentialRequestOptions")?;
    let origin = format!("https://{}", opts.rp_id);

    log::info!("triggering passkey authenticator (rp_id={})", opts.rp_id);
    let assertion = passkey::perform_authentication(&opts, &origin).await?;
    let credential = passkey::build_tl_credential(&assertion)?;

    let _: tl::enums::auth::Authorization = client
        .invoke(&tl::functions::auth::FinishPasskeyLogin {
            credential,
            from_dc_id: None,
            from_auth_key_id: None,
        })
        .await
        .map_err(|e| anyhow!("auth.finishPasskeyLogin failed: {e}"))?;
    log::info!("passkey login successful");
    Ok(())
}

fn prompt(label: &str) -> Result<String> {
    let mut out = io::stdout();
    out.write_all(label.as_bytes())?;
    out.flush()?;
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    Ok(line)
}
