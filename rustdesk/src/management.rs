use hbb_common::{
    anyhow::anyhow,
    bail,
    config::{Config, LocalConfig, CONNECT_TIMEOUT},
    log,
    websocket::WsFramedStream,
    ResultType, Stream,
};
use serde_json::{json, Value};

#[cfg(feature = "relivo-managed")]
pub(crate) mod package;

pub(crate) const DEVICE_TOKEN: &str = "management-device-token";
const REPORT_LOG_INTERVAL: std::time::Duration = std::time::Duration::from_secs(300);

pub(crate) fn enabled() -> bool {
    Config::get_option("management-required") == "Y" || !device_token().is_empty()
}
// The API requires exactly 64 hex characters; tolerate whitespace from copied files.
pub(crate) fn device_token() -> String {
    Config::get_option(DEVICE_TOKEN).trim().to_owned()
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
        device_token()
    } else {
        LocalConfig::get_option("access_token").trim().to_owned()
    };
    if credential.is_empty() {
        if device {
            bail!("Device enrollment credential missing: this device is not enrolled");
        }
        bail!("Sign in to Relivo on this computer before connecting to a managed device");
    }
    // Use normal certificate validation and no raw-TCP API fallback for credentials.
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    let origin = url.origin().ascii_serialization();
    let response = client
        .post(url)
        .bearer_auth(credential)
        .json(&json!({"target":target,"uuid":uuid,"role":if device{"device"}else{"controller"}}))
        .send()
        .await
        .map_err(|e| anyhow!("Cannot reach the Relivo API ({origin}) for a relay ticket: {e}"))?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        let body = response.json::<Value>().await.ok();
        bail!("{}", ticket_refusal(status, device, body.as_ref()));
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
/// The message shown when the API refuses a relay ticket. The API's `error` field is a
/// short, non-secret reason such as "no connect grant"; it is bounded and stripped of
/// control characters.
fn ticket_refusal(status: u16, device: bool, body: Option<&Value>) -> String {
    let reason = match (status, device) {
        (401, false) => "your Relivo session has expired or you are not signed in; sign in again",
        (401, true) => "the device enrollment credential is missing, invalid or does not match this device ID",
        (403, false) => "the device is not approved, or your account lacks the connect permission for it",
        (403, true) => "this device is not approved or is disabled in Relivo Console, or the request is not authorized",
        _ => "the management service rejected the request",
    };
    let detail = body
        .and_then(|v| v["error"].as_str())
        .map(|s| s.chars().filter(|c| !c.is_control()).take(120).collect::<String>())
        .filter(|s| !s.is_empty());
    match detail {
        Some(detail) => format!("Managed connection denied (HTTP {status}: {detail}): {reason}"),
        None => format!("Managed connection denied (HTTP {status}): {reason}"),
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
        bail!("Managed device report rejected (HTTP {status})");
    }
    Ok(body)
}
pub(crate) async fn report_with_status(url: String, body: String) -> ResultType<(u16, String)> {
    let target = url::Url::parse(&url)?;
    let expected = api_url()?;
    if target.origin() != expected.origin() {
        bail!("Managed device reports must use the configured API origin");
    }
    let token = device_token();
    if token.is_empty() {
        static MISSING: std::sync::Once = std::sync::Once::new();
        MISSING.call_once(|| {
            log::error!("Device enrollment credential missing: this device is not enrolled")
        });
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
    match status {
        401 => hbb_common::throttled_log!(REPORT_LOG_INTERVAL, error,
            "Relivo rejected the device credential (HTTP 401): the enrollment token is missing or invalid, or this agent ID ({}) does not match the device created in Relivo Console", Config::get_id()),
        403 => hbb_common::throttled_log!(REPORT_LOG_INTERVAL, error,
            "Relivo refused this device (HTTP 403): it is disabled in Relivo Console, or its identity (UUID / public key) differs from the pinned one, typically after a reinstall; rotate the device credential in Relivo Console and enroll again"),
        _ => {}
    }
    Ok((status, response.text().await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticket_refusal_names_the_cause_and_the_next_step() {
        let detail = json!({"error": "session expired"});
        assert_eq!(
            ticket_refusal(401, false, Some(&detail)),
            "Managed connection denied (HTTP 401: session expired): your Relivo session has expired or you are not signed in; sign in again"
        );
        assert_eq!(
            ticket_refusal(403, false, Some(&json!({"error": "no connect grant"}))),
            "Managed connection denied (HTTP 403: no connect grant): the device is not approved, or your account lacks the connect permission for it"
        );
        assert_eq!(
            ticket_refusal(401, true, None),
            "Managed connection denied (HTTP 401): the device enrollment credential is missing, invalid or does not match this device ID"
        );
        assert_eq!(
            ticket_refusal(403, true, Some(&json!({}))),
            "Managed connection denied (HTTP 403): this device is not approved or is disabled in Relivo Console, or the request is not authorized"
        );
        for status in [400, 404, 409, 429, 500, 502] {
            assert_eq!(
                ticket_refusal(status, false, None),
                format!("Managed connection denied (HTTP {status}): the management service rejected the request")
            );
        }
    }

    #[test]
    fn ticket_refusal_detail_is_bounded_and_single_line() {
        let forged = json!({"error": "no connect grant\n[ERROR] forged line\r\u{1b}[0m"});
        let message = ticket_refusal(403, false, Some(&forged));
        assert!(!message.chars().any(char::is_control), "{message:?}");
        assert!(message.starts_with("Managed connection denied (HTTP 403: no connect grant[ERROR] forged line[0m): "));
        let long = json!({ "error": "y".repeat(1000) });
        assert!(ticket_refusal(401, false, Some(&long)).contains(&format!("HTTP 401: {}): ", "y".repeat(120))));
        for body in [json!({"error": ""}), json!({"error": "\n"}), json!({"error": 7}), json!("text"), json!(null)] {
            assert_eq!(
                ticket_refusal(500, true, Some(&body)),
                "Managed connection denied (HTTP 500): the management service rejected the request",
                "{body}"
            );
        }
    }
}
