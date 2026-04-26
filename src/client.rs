use std::io::{self, BufRead, Write};
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};
use grammers_client::peer::Peer;
use grammers_client::{Client, SignInError};
use grammers_mtsender::{ConnectionParams, SenderPool, SenderPoolFatHandle};
use grammers_session::storages::SqliteSession;
use grammers_tl_types as tl;
use tokio::task::JoinHandle;

use crate::config::Config;
#[cfg(feature = "passkey-login")]
use crate::passkey;
use crate::target::Target;

pub struct ClientHandle {
    pub client: Client,
    pub pool_handle: SenderPoolFatHandle,
    pub _runner_task: JoinHandle<()>,
}

impl ClientHandle {
    pub async fn build(config: &Config, proxy_url: Option<String>) -> Result<Self> {
        let session = Arc::new(
            SqliteSession::open(&config.session_path)
                .await
                .with_context(|| format!("open session at {}", config.session_path))?,
        );

        let params = ConnectionParams {
            proxy_url,
            ..Default::default()
        };

        let SenderPool {
            runner,
            handle,
            updates: _,
        } = SenderPool::with_configuration(Arc::clone(&session), config.api_id, params);
        let client = Client::new(handle.clone());
        let runner_task = tokio::spawn(async move {
            runner.run().await;
        });

        Ok(Self {
            client,
            pool_handle: handle,
            _runner_task: runner_task,
        })
    }

    pub async fn ensure_logged_in(
        &self,
        api_id: i32,
        api_hash: &str,
        method: crate::config::LoginMethod,
    ) -> Result<()> {
        use crate::config::LoginMethod;
        if self.client.is_authorized().await? {
            return Ok(());
        }
        match method {
            LoginMethod::Password => self.login_with_password(api_hash).await,
            LoginMethod::Passkey => {
                #[cfg(feature = "passkey-login")]
                {
                    self.login_with_passkey(api_id, api_hash).await
                }
                #[cfg(not(feature = "passkey-login"))]
                {
                    let _ = (api_id, api_hash);
                    Err(anyhow!(
                        "LOGIN_METHOD=passkey requires passkey-login feature; \
                         rebuild with `cargo build --features passkey-login` \
                         or set LOGIN_METHOD=password"
                    ))
                }
            }
            LoginMethod::Auto => {
                #[cfg(feature = "passkey-login")]
                {
                    log::info!(
                        "LOGIN_METHOD=auto: attempting passkey first (set LOGIN_METHOD=password to skip)"
                    );
                    match self.login_with_passkey(api_id, api_hash).await {
                        Ok(()) => Ok(()),
                        Err(e) => {
                            log::warn!("passkey login failed ({e}); falling back to SMS+password");
                            self.login_with_password(api_hash).await
                        }
                    }
                }
                #[cfg(not(feature = "passkey-login"))]
                {
                    let _ = api_id;
                    log::info!(
                        "LOGIN_METHOD=auto with passkey-login feature disabled at compile time; using password"
                    );
                    self.login_with_password(api_hash).await
                }
            }
        }
    }

    async fn login_with_password(&self, api_hash: &str) -> Result<()> {
        let phone = prompt("Phone number (international format, e.g. +1...): ")?;
        let token = self
            .client
            .request_login_code(phone.trim(), api_hash)
            .await?;
        let code = prompt("Login code: ")?;
        match self.client.sign_in(&token, code.trim()).await {
            Ok(_) => Ok(()),
            Err(SignInError::PasswordRequired(pwd_token)) => {
                let hint = pwd_token.hint().unwrap_or("(no hint)");
                let pw = prompt(&format!("2FA password (hint: {hint}): "))?;
                self.client
                    .check_password(pwd_token, pw.trim())
                    .await
                    .map_err(|e| anyhow!("2FA failed: {e}"))?;
                Ok(())
            }
            Err(SignInError::SignUpRequired) => bail!("account requires sign-up"),
            Err(other) => Err(anyhow!("sign_in failed: {other}")),
        }
    }

    pub async fn resolve_target(&self, target: &Target) -> Result<Peer> {
        match target {
            Target::Username(name) => self
                .client
                .resolve_username(name)
                .await?
                .ok_or_else(|| anyhow!("username @{name} not found")),
            Target::ChatId(id) => {
                let mut iter = self.client.iter_dialogs();
                while let Some(dialog) = iter.next().await? {
                    let peer = dialog.peer();
                    if peer.id().bot_api_dialog_id_unchecked() == *id {
                        return Ok(peer.clone());
                    }
                }
                bail!("chat id {id} not in any dialog of this account")
            }
        }
    }

    pub async fn send(
        &self,
        peer: &Peer,
        topic_id: Option<i32>,
        reply_to_msg_id: Option<i32>,
        text: &str,
    ) -> Result<()> {
        let peer_ref = peer
            .to_ref()
            .await
            .ok_or_else(|| anyhow!("cannot resolve peer to ref (need auth?)"))?;
        let input_peer: tl::enums::InputPeer = peer_ref.into();

        let reply_to = build_reply_to(reply_to_msg_id, topic_id);

        self.client
            .invoke(&tl::functions::messages::SendMessage {
                no_webpage: false,
                silent: false,
                background: false,
                clear_draft: false,
                noforwards: false,
                update_stickersets_order: false,
                invert_media: false,
                allow_paid_floodskip: false,
                peer: input_peer,
                reply_to,
                message: text.to_string(),
                random_id: rand::random::<i64>(),
                reply_markup: None,
                entities: None,
                schedule_date: None,
                schedule_repeat_period: None,
                send_as: None,
                quick_reply_shortcut: None,
                effect: None,
                allow_paid_stars: None,
                suggested_post: None,
            })
            .await
            .map(drop)
            .map_err(|e| anyhow!("messages.SendMessage failed: {e}"))
    }

    #[cfg(feature = "passkey-login")]
    pub async fn login_with_passkey(&self, api_id: i32, api_hash: &str) -> Result<()> {
        // 外层 ensure_logged_in 已检查 is_authorized;此函数也允许独立调用,
        // 所以再 guard 一次保证幂等。
        if self.client.is_authorized().await? {
            return Ok(());
        }

        let init: tl::enums::auth::PasskeyLoginOptions = self
            .client
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
        // Telegram options 一般不带 origin;统一用 https://{rp_id}
        let origin = format!("https://{}", opts.rp_id);

        log::info!("triggering passkey authenticator (rp_id={})", opts.rp_id);
        let assertion = passkey::perform_authentication(&opts, &origin).await?;

        let credential = passkey::build_tl_credential(&assertion)?;

        let _auth: tl::enums::auth::Authorization = self
            .client
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
}

fn build_reply_to(
    reply_to_msg_id: Option<i32>,
    topic_id: Option<i32>,
) -> Option<tl::enums::InputReplyTo> {
    let (rid, tid) = match (reply_to_msg_id, topic_id) {
        (None, None) => return None,
        (Some(r), tid) => (r, tid),
        (None, Some(t)) => (t, Some(t)),
    };
    Some(tl::enums::InputReplyTo::Message(
        tl::types::InputReplyToMessage {
            reply_to_msg_id: rid,
            top_msg_id: tid,
            reply_to_peer_id: None,
            quote_text: None,
            quote_entities: None,
            quote_offset: None,
            monoforum_peer_id: None,
            todo_item_id: None,
            poll_option: None,
        },
    ))
}

fn prompt(label: &str) -> Result<String> {
    let mut out = io::stdout();
    out.write_all(label.as_bytes())?;
    out.flush()?;
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    Ok(line)
}
