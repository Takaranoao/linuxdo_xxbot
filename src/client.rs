use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};
use grammers_client::Client;
use grammers_client::peer::Peer;
use grammers_mtsender::{ConnectionParams, SenderPool, SenderPoolFatHandle};
use grammers_session::storages::SqliteSession;
use grammers_tl_types as tl;
use tokio::task::JoinHandle;

use crate::config::{AccountConfig, NetworkConfig};
use crate::target::Target;

pub struct ClientHandle {
    pub client: Client,
    pub pool_handle: SenderPoolFatHandle,
    pub _runner_task: JoinHandle<()>,
}

impl ClientHandle {
    /// 打开 session 文件、起 SenderPool、spawn pool runner。
    /// **不做认证** — 调用方拿到后用 `crate::auth::ensure_logged_in(&handle.client, ...)` 自行登录。
    pub async fn build(
        account: &AccountConfig,
        network: &NetworkConfig,
        proxy_url: Option<String>,
    ) -> Result<Self> {
        let session = Arc::new(
            SqliteSession::open(&account.session_path)
                .await
                .with_context(|| format!("open session at {}", account.session_path))?,
        );

        let params = ConnectionParams {
            proxy_url,
            use_ipv6: network.use_ipv6,
            ..Default::default()
        };

        let SenderPool {
            runner,
            handle,
            updates: _,
        } = SenderPool::with_configuration(Arc::clone(&session), account.api_id, params);
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
