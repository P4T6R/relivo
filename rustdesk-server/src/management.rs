use hbb_common::{bail, ResultType};
use once_cell::sync::OnceCell;
use serde_json::{json, Value};
use std::{net::IpAddr, time::Duration};

struct Manager {
    url: String,
    key: String,
    relay: String,
    client: reqwest::Client,
}
static MANAGER: OnceCell<Option<Manager>> = OnceCell::new();

pub(crate) fn init() -> ResultType<()> {
    MANAGER.get_or_try_init(|| -> ResultType<Option<Manager>> {
        let url = crate::common::get_arg("MANAGEMENT_URL");
        if url.is_empty() {
            return Ok(None);
        }
        let parsed = reqwest::Url::parse(&url)?;
        let local = parsed
            .host_str()
            .and_then(|v| v.parse::<IpAddr>().ok())
            .is_some_and(|ip| ip.is_loopback());
        if parsed.scheme() != "http"
            || !local
            || parsed.path() != "/"
            || parsed.query().is_some()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
        {
            bail!("MANAGEMENT_URL must be an HTTP loopback origin");
        }
        let path = crate::common::get_arg("MANAGEMENT_INTERNAL_KEY_FILE");
        let key = std::fs::read_to_string(path)?.trim().to_owned();
        if key.len() < 32 {
            bail!("Management internal key is too short");
        }
        let relay = crate::common::get_arg("MANAGEMENT_RELAY_URL");
        let target = reqwest::Url::parse(&relay)?;
        let local_relay = target
            .host_str()
            .and_then(|v| v.parse::<IpAddr>().ok())
            .is_some_and(|ip| ip.is_loopback());
        if target.scheme() != "wss" && !(target.scheme() == "ws" && local_relay) {
            bail!("Managed relay requires WSS, except for loopback testing");
        }
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(3))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Some(Manager {
            url: url.trim_end_matches('/').to_owned(),
            key,
            relay: target.to_string(),
            client,
        }))
    })?;
    Ok(())
}
pub(crate) fn enabled() -> bool {
    matches!(MANAGER.get(), Some(Some(_)))
}
fn manager() -> ResultType<&'static Manager> {
    MANAGER
        .get()
        .and_then(Option::as_ref)
        .ok_or_else(|| hbb_common::anyhow::anyhow!("Management is not initialized"))
}
pub(crate) fn relay_url() -> ResultType<String> {
    Ok(manager()?.relay.clone())
}
async fn post(path: &str, body: Value) -> ResultType<Value> {
    let m = manager()?;
    let response = m
        .client
        .post(format!("{}{path}", m.url))
        .header("X-Management-Key", &m.key)
        .json(&body)
        .send()
        .await?;
    if !response.status().is_success() {
        bail!("Management authorization denied");
    }
    Ok(response.json().await?)
}
pub(crate) async fn authorize(
    token: &str,
    target: &str,
    pk: &[u8],
    conn_type: i32,
    attempt: &str,
) -> ResultType<()> {
    let value=post("/authorize",json!({"token":token,"target":target,"public_key":base64::encode(pk),"conn_type":conn_type,"attempt":attempt})).await?;
    if value["allowed"] != true {
        bail!("Management authorization denied");
    }
    Ok(())
}
pub(crate) async fn claim(ticket: &str, uuid: &str, target: &str) -> ResultType<()> {
    if ticket.len() != 64 {
        bail!("A managed relay ticket is required");
    }
    let result = post(
        "/relay/claim",
        json!({"ticket":ticket,"uuid":uuid,"target":target}),
    )
    .await?;
    if !matches!(result["role"].as_str(), Some("controller" | "device"))
        || result["target"].as_str() != Some(target)
        || result["attempt"].as_str() != Some(uuid)
    {
        bail!("Invalid relay admission");
    }
    Ok(())
}
pub(crate) async fn event(uuid: &str, event: &str) -> ResultType<()> {
    post("/relay/event", json!({"uuid":uuid,"event":event})).await?;
    Ok(())
}
