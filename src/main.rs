use std::collections::HashMap;
use std::env;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use chrono::Utc;
use simple_logger::SimpleLogger;
use tokio::signal;
use tokio::time::{Instant, sleep_until};

use tg_cron_sender::auth;
use tg_cron_sender::client::ClientHandle;
use tg_cron_sender::config::Config;
use tg_cron_sender::proxy::build_proxy_url;
use tg_cron_sender::schedule::Schedule;
use tg_cron_sender::target::parse_target;

#[tokio::main]
async fn main() -> Result<()> {
    SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .init()
        .ok();

    let _ = dotenvy::dotenv();

    let env_map: HashMap<String, String> = env::vars().collect();
    let config = Config::from_map(&env_map).context("load config from env")?;
    let proxy_url = build_proxy_url(
        config.proxy.proxy_type.as_deref(),
        config.proxy.proxy_host.as_deref(),
        config.proxy.proxy_username.as_deref(),
        config.proxy.proxy_password.as_deref(),
    )?;
    if let Some(ref u) = proxy_url {
        log::info!("using proxy: {}", redact_creds(u));
    }

    let target = parse_target(&config.job.target_chat)?;
    let schedule = Schedule::parse(&config.job.cron_expr)?;

    let handle = ClientHandle::build(&config.account, &config.network, proxy_url).await?;
    auth::ensure_logged_in(
        &handle.client,
        config.account.api_id,
        &config.account.api_hash,
        config.account.login_method,
    )
    .await?;

    let peer = handle.resolve_target(&target).await?;
    log::info!(
        "resolved target id={:?}, topic={:?}, reply_to={:?}",
        peer.id(),
        config.job.target_topic_id,
        config.job.target_reply_to_msg_id
    );

    log::info!("entering cron loop ({})", config.job.cron_expr);
    loop {
        let now = Utc::now();
        let next = schedule
            .next_after(now)
            .ok_or_else(|| anyhow!("cron has no next occurrence after {now}"))?;
        let wait = (next - now)
            .to_std()
            .unwrap_or_else(|_| Duration::from_secs(0));
        log::info!("next fire at {next} (in {wait:?})");
        let deadline = Instant::now() + wait;
        tokio::select! {
            _ = signal::ctrl_c() => {
                log::info!("ctrl-c received, shutting down");
                break;
            }
            _ = sleep_until(deadline) => {
                if let Err(e) = handle
                    .send(
                        &peer,
                        config.job.target_topic_id,
                        config.job.target_reply_to_msg_id,
                        &config.job.message,
                    )
                    .await
                {
                    log::error!("send failed: {e:?}");
                } else {
                    log::info!("sent");
                }
            }
        }
    }

    handle.pool_handle.quit();
    Ok(())
}

fn redact_creds(url: &str) -> String {
    if let Some((scheme, rest)) = url.split_once("://")
        && let Some((_creds, host)) = rest.split_once('@')
    {
        return format!("{scheme}://***@{host}");
    }
    url.to_string()
}
