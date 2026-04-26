//! Passkey 登录:Telegram WebAuthn options ↔ webauthn-authenticator-rs 桥接,
//! 以及 assertion → TL `InputPasskeyCredential` 转换。
//!
//! 仅在 `passkey-login` feature 启用时编译。

use anyhow::{Context, Result, anyhow};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use grammers_tl_types as tl;
use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PasskeyOptions {
    pub rp_id: String,
    pub challenge: String,
    #[serde(default)]
    pub timeout: Option<u32>,
    #[serde(default)]
    pub user_verification: Option<String>,
    #[serde(default)]
    pub allow_credentials: Vec<AllowCredential>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AllowCredential {
    #[serde(rename = "type")]
    pub ty: String,
    pub id: String,
    #[serde(default)]
    pub transports: Vec<String>,
}

impl PasskeyOptions {
    pub fn from_json(s: &str) -> Result<Self> {
        serde_json::from_str(s).with_context(|| format!("invalid passkey options JSON: {s}"))
    }

    pub fn to_request(&self) -> Result<webauthn_rs_proto::RequestChallengeResponse> {
        use webauthn_rs_proto::{
            AllowCredentials, PublicKeyCredentialRequestOptions, RequestChallengeResponse,
            UserVerificationPolicy,
        };
        let challenge = B64URL
            .decode(&self.challenge)
            .with_context(|| "challenge is not valid base64url")?;
        let allow_credentials = self
            .allow_credentials
            .iter()
            .map(|c| {
                let id = B64URL
                    .decode(&c.id)
                    .with_context(|| format!("allowCredential id not valid base64url: {}", c.id))?;
                Ok::<_, anyhow::Error>(AllowCredentials {
                    type_: c.ty.clone(),
                    id: id.into(),
                    transports: None,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let user_verification = match self.user_verification.as_deref() {
            Some("required") => UserVerificationPolicy::Required,
            Some("discouraged") => UserVerificationPolicy::Discouraged_DO_NOT_USE,
            _ => UserVerificationPolicy::Preferred,
        };
        Ok(RequestChallengeResponse {
            public_key: PublicKeyCredentialRequestOptions {
                challenge: challenge.into(),
                timeout: self.timeout,
                rp_id: self.rp_id.clone(),
                allow_credentials,
                user_verification,
                hints: None,
                extensions: None,
            },
            mediation: None,
        })
    }
}

/// 把 webauthn-authenticator-rs 返回的 assertion 转成 Telegram TL credential。
///
/// 编码约定:`raw_id` / `user_handle` 字段 TL 是 string,统一用 base64url-no-pad 编码 bytes;
/// `client_data` 是 UTF-8 JSON 直接用 String::from_utf8。
/// 若实测 Telegram 拒绝(常见替代:hex / 不编码),把对应字段编码改掉即可,
/// 测试里也改对应期望值。
pub fn build_tl_credential(
    assertion: &webauthn_rs_proto::PublicKeyCredential,
) -> Result<tl::enums::InputPasskeyCredential> {
    let raw_id = B64URL.encode(&*assertion.raw_id);
    let client_data_str = String::from_utf8(assertion.response.client_data_json.to_vec())
        .with_context(|| "clientDataJSON is not valid UTF-8")?;
    let user_handle = match &assertion.response.user_handle {
        Some(uh) => B64URL.encode(&**uh),
        None => String::new(),
    };
    let response = tl::enums::InputPasskeyResponse::Login(tl::types::InputPasskeyResponseLogin {
        client_data: tl::enums::DataJson::Json(tl::types::DataJson {
            data: client_data_str,
        }),
        authenticator_data: assertion.response.authenticator_data.to_vec(),
        signature: assertion.response.signature.to_vec(),
        user_handle,
    });
    Ok(tl::enums::InputPasskeyCredential::PublicKey(
        tl::types::InputPasskeyCredentialPublicKey {
            id: assertion.id.clone(),
            raw_id,
            response,
        },
    ))
}

/// 调起 caBLE/Hybrid authenticator 完成 WebAuthn assertion。
///
/// `origin`:Telegram 指定的 RP origin(如 `"https://web.telegram.org"`)。
/// 如果 init options 没明确给 origin,可以用 `format!("https://{rp_id}")`。
///
/// 内部协议:打印 QR + caBLE link → 用户用手机(同 iCloud / Google 账号)扫码 →
/// 手机弹"使用 passkey 登录..." → 本机指纹 / 面容 → 桌面拿到 assertion。
///
/// 返回的 PublicKeyCredential 直接喂给 [`build_tl_credential`]。
pub async fn perform_authentication(
    options: &PasskeyOptions,
    origin: &str,
) -> Result<webauthn_rs_proto::PublicKeyCredential> {
    use webauthn_authenticator_rs::AuthenticatorBackend;
    use webauthn_authenticator_rs::types::CableRequestType;
    use webauthn_authenticator_rs::ui::Cli;

    let request = options.to_request()?;
    let pk_options = request.public_key;
    let timeout_ms = pk_options.timeout.unwrap_or(60_000);
    let origin_url =
        url::Url::parse(origin).with_context(|| format!("invalid origin URL: {origin}"))?;

    log::info!("starting caBLE/hybrid authenticator (scan QR with your phone)");
    let ui = Cli {};
    let mut authenticator = webauthn_authenticator_rs::cable::connect_cable_authenticator(
        CableRequestType::GetAssertion,
        &ui,
    )
    .await
    .map_err(|e| anyhow!("caBLE connect failed: {e:?}"))?;

    let credential = authenticator
        .perform_auth(origin_url, pk_options, timeout_ms)
        .map_err(|e| anyhow!("perform_auth failed: {e:?}"))?;

    Ok(credential)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    const SAMPLE_OPTIONS: &str = include_str!("../tests/passkey_fixtures/sample_options.json");

    #[test]
    fn parses_minimal_options() {
        let json = r#"{"rpId":"web.telegram.org","challenge":"AAA"}"#;
        let opts = PasskeyOptions::from_json(json).unwrap();
        assert_eq!(opts.rp_id, "web.telegram.org");
        assert_eq!(opts.challenge, "AAA");
        assert!(opts.allow_credentials.is_empty());
    }

    #[test]
    fn parses_full_sample_fixture() {
        let opts = PasskeyOptions::from_json(SAMPLE_OPTIONS).unwrap();
        assert_eq!(opts.rp_id, "web.telegram.org");
        assert_eq!(opts.timeout, Some(60000));
        assert_eq!(opts.user_verification.as_deref(), Some("preferred"));
        assert_eq!(opts.allow_credentials.len(), 1);
        assert_eq!(opts.allow_credentials[0].ty, "public-key");
        assert_eq!(
            opts.allow_credentials[0].transports,
            vec!["internal", "hybrid"]
        );
    }

    #[test]
    fn ignores_unknown_extension_fields() {
        let json =
            r#"{"rpId":"x","challenge":"y","extensions":{"appid":"foo"},"hints":["security-key"]}"#;
        assert!(PasskeyOptions::from_json(json).is_ok());
    }

    #[test]
    fn missing_required_errors() {
        let json = r#"{"challenge":"AAA"}"#;
        assert!(PasskeyOptions::from_json(json).is_err());
    }

    #[test]
    fn to_request_round_trips_basic_fields() {
        let opts = PasskeyOptions::from_json(SAMPLE_OPTIONS).unwrap();
        let req = opts.to_request().unwrap();
        assert_eq!(req.public_key.rp_id, "web.telegram.org");
        assert!(!req.public_key.challenge.is_empty());
        assert_eq!(req.public_key.allow_credentials.len(), 1);
    }

    #[test]
    fn to_request_decodes_base64url_challenge() {
        let opts = PasskeyOptions {
            rp_id: "x".into(),
            challenge: "AAEC".into(), // base64url AAEC = [0,1,2]
            timeout: None,
            user_verification: None,
            allow_credentials: vec![],
        };
        let req = opts.to_request().unwrap();
        assert_eq!(&*req.public_key.challenge, &[0u8, 1, 2]);
    }

    #[test]
    fn to_request_invalid_base64_errors() {
        let opts = PasskeyOptions {
            rp_id: "x".into(),
            challenge: "!!!".into(),
            timeout: None,
            user_verification: None,
            allow_credentials: vec![],
        };
        assert!(opts.to_request().is_err());
    }

    fn fake_assertion() -> webauthn_rs_proto::PublicKeyCredential {
        use webauthn_rs_proto::{AuthenticatorAssertionResponseRaw, PublicKeyCredential};
        PublicKeyCredential {
            id: "credid_b64url".to_string(),
            raw_id: vec![0xAB, 0xCD, 0xEF].into(),
            response: AuthenticatorAssertionResponseRaw {
                authenticator_data: b"AUTH_DATA".to_vec().into(),
                client_data_json: br#"{"type":"webauthn.get"}"#.to_vec().into(),
                signature: vec![0x01, 0x02, 0x03].into(),
                user_handle: Some(b"user-1".to_vec().into()),
            },
            extensions: Default::default(),
            type_: "public-key".to_string(),
        }
    }

    #[test]
    fn build_tl_credential_maps_id_and_raw_id() {
        let cred = build_tl_credential(&fake_assertion()).unwrap();
        let tl::enums::InputPasskeyCredential::PublicKey(pk) = cred else {
            panic!("expected PublicKey variant");
        };
        assert_eq!(pk.id, "credid_b64url");
        // base64url(0xAB,0xCD,0xEF) = "q83v"
        assert_eq!(pk.raw_id, "q83v");
    }

    #[test]
    fn build_tl_credential_maps_response_fields() {
        let cred = build_tl_credential(&fake_assertion()).unwrap();
        let tl::enums::InputPasskeyCredential::PublicKey(pk) = cred else {
            unreachable!()
        };
        let tl::enums::InputPasskeyResponse::Login(login) = pk.response else {
            panic!("expected Login variant");
        };
        let tl::enums::DataJson::Json(cd) = login.client_data;
        assert_eq!(cd.data, r#"{"type":"webauthn.get"}"#);
        assert_eq!(login.authenticator_data, b"AUTH_DATA".to_vec());
        assert_eq!(login.signature, vec![0x01, 0x02, 0x03]);
        // base64url("user-1") = "dXNlci0x"
        assert_eq!(login.user_handle, "dXNlci0x");
    }

    #[test]
    fn build_tl_credential_user_handle_none_becomes_empty() {
        let mut a = fake_assertion();
        a.response.user_handle = None;
        let cred = build_tl_credential(&a).unwrap();
        let tl::enums::InputPasskeyCredential::PublicKey(pk) = cred else {
            unreachable!()
        };
        let tl::enums::InputPasskeyResponse::Login(login) = pk.response else {
            unreachable!()
        };
        assert_eq!(login.user_handle, "");
    }

    #[test]
    fn build_tl_credential_invalid_client_data_utf8_errors() {
        let mut a = fake_assertion();
        // 0xFF 不是 valid UTF-8 起始字节
        a.response.client_data_json = vec![0xFF, 0xFE, 0xFD].into();
        assert!(build_tl_credential(&a).is_err());
    }
}
