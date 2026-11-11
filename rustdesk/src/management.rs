use hbb_common::{
    bail,
    config::{Config, LocalConfig, CONNECT_TIMEOUT},
    websocket::WsFramedStream,
    ResultType, Stream,
};
use serde_json::{json, Value};

#[cfg(feature = "relivo-managed")]
pub(crate) mod package;

pub(crate) fn enabled() -> bool {
    Config::get_option("management-required") == "Y"
        || !Config::get_option("management-device-token").is_empty()
}

fn api_url() -> ResultType<url::Url> {
    let url = url::Url::parse(&Config::get_option("api-server"))?;
    let local = url
        .host_str()
        .and_then(|v| v.parse::<std::net::IpAddr>().ok())
        .is_some_and(|ip| ip.is_loopback());
    if url.scheme() != "https" && !(url.scheme() == "http" && local) {
        bail!("Managed mode requires an HTTPS API server");
    }
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.path() != "/"
    {
        bail!("Use an API origin without credentials, path or query");
    }
    Ok(url)
}
pub(crate) fn websocket_url(path: &str) -> ResultType<String> {
    let mut url = api_url()?;
    let scheme = if url.scheme() == "https" { "wss" } else { "ws" };
    url.set_scheme(scheme)
        .map_err(|_| hbb_common::anyhow::anyhow!("Invalid managed WebSocket scheme"))?;
    url.set_path(path);
    Ok(url.to_string())
}
pub(crate) async fn connect_websocket(path: &str) -> ResultType<Stream> {
    Ok(Stream::WebSocket(
        WsFramedStream::new_strict(websocket_url(path)?, CONNECT_TIMEOUT).await?,
    ))
}
pub(crate) async fn relay_ticket(target: &str, uuid: &str, device: bool) -> ResultType<String> {
    let mut url = api_url()?;
    url.set_path("/api/management/relay-ticket");
    let credential = if device {
        Config::get_option("management-device-token")
    } else {
        LocalConfig::get_option("access_token")
    };
    if credential.is_empty() {
        bail!("Managed mode requires device enrollment and an account login on the controlling computer");
    }
    // Use normal certificate validation and no raw-TCP API fallback for credentials.
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    let response = client
        .post(url)
        .bearer_auth(credential)
        .json(&json!({"target":target,"uuid":uuid,"role":if device{"device"}else{"controller"}}))
        .send()
        .await?;
    if !response.status().is_success() {
        bail!("Managed connection denied: check device approval and group permissions");
    }
    let value: Value = response.json().await?;
    let ticket = value["ticket"]
        .as_str()
        .filter(|s| s.len() == 64)
        .ok_or_else(|| hbb_common::anyhow::anyhow!("Invalid management relay ticket"))?;
    Ok(ticket.to_owned())
}
pub(crate) fn enrich_report(value: &mut Value) {
    if enabled() {
        value["management_pk"] = json!(crate::encode64(Config::get_key_pair().1));
    }
}
pub(crate) async fn report(url: String, body: String) -> ResultType<String> {
    if !enabled() {
        return crate::post_request(url, body, "").await;
    }
    report_managed(url, body).await
}
pub(crate) async fn report_managed(url: String, body: String) -> ResultType<String> {
    let (status, body) = report_with_status(url, body).await?;
    if !(200..300).contains(&status) {
        bail!("Managed device report rejected");
    }
    Ok(body)
}
pub(crate) async fn report_with_status(url: String, body: String) -> ResultType<(u16, String)> {
    let target = url::Url::parse(&url)?;
    let expected = api_url()?;
    if target.origin() != expected.origin() {
        bail!("Managed device reports must use the configured API origin");
    }
    let token = Config::get_option("management-device-token");
    if token.is_empty() {
        bail!("Device enrollment credential missing");
    }
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    let response = client
        .post(target)
        .bearer_auth(token)
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .await?;
    let status = response.status().as_u16();
    Ok((status, response.text().await?))
}
