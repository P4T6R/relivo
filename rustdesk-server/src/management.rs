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
/// Why a management API call did not succeed.
///
/// The rendezvous and relay servers report these two cases differently to the
/// client and in the logs, so a refusal must never be confused with an outage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ManagementError {
    /// The API answered and refused the request (4xx status, or a 2xx body that
    /// does not grant it). The text never contains a token or a ticket.
    Denied(String),
    /// The API could not be used: network error, timeout, 5xx or other
    /// unexpected status, unreadable body, or management not initialized.
    Unavailable(String),
}

impl std::fmt::Display for ManagementError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Denied(reason) => write!(f, "refused by the management API: {reason}"),
            Self::Unavailable(reason) => write!(f, "management API unavailable: {reason}"),
        }
    }
}

impl std::error::Error for ManagementError {}

/// Maps an HTTP status to `None` (success) or the matching error.
fn classify_status(status: reqwest::StatusCode) -> Option<ManagementError> {
    if status.is_success() {
        None
    } else if status.is_client_error() {
        Some(ManagementError::Denied(format!("HTTP {}", status.as_u16())))
    } else {
        Some(ManagementError::Unavailable(format!("HTTP {}", status.as_u16())))
    }
}

async fn post(path: &str, body: Value) -> Result<Value, ManagementError> {
    let m = manager().map_err(|e| ManagementError::Unavailable(e.to_string()))?;
    let response = m
        .client
        .post(format!("{}{path}", m.url))
        .header("X-Management-Key", &m.key)
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            // reqwest errors name the loopback URL and the cause, never the body or headers.
            let kind = if e.is_timeout() { "timeout" } else { "request failed" };
            ManagementError::Unavailable(format!("{kind} on {path}: {e}"))
        })?;
    if let Some(err) = classify_status(response.status()) {
        return Err(match err {
            ManagementError::Denied(r) => match api_error(response).await {
                Some(detail) => ManagementError::Denied(format!("{r} on {path}: {detail}")),
                None => ManagementError::Denied(format!("{r} on {path}")),
            },
            ManagementError::Unavailable(r) => {
                ManagementError::Unavailable(format!("{r} on {path}"))
            }
        });
    }
    response
        .json()
        .await
        .map_err(|e| ManagementError::Unavailable(format!("unreadable response on {path}: {e}")))
}
/// The API's own refusal reason (its `error` field: a fixed text, never a token),
/// bounded and stripped of control characters for the log.
async fn api_error(response: reqwest::Response) -> Option<String> {
    refusal_detail(&response.json::<Value>().await.ok()?)
}
fn refusal_detail(value: &Value) -> Option<String> {
    let detail: String = value["error"]
        .as_str()?
        .chars()
        .filter(|c| !c.is_control())
        .take(120)
        .collect();
    (!detail.is_empty()).then_some(detail)
}
pub(crate) async fn authorize(
    token: &str,
    target: &str,
    pk: &[u8],
    conn_type: i32,
    attempt: &str,
) -> Result<(), ManagementError> {
    let value=post("/authorize",json!({"token":token,"target":target,"public_key":base64::encode(pk),"conn_type":conn_type,"attempt":attempt})).await?;
    if value["allowed"] != true {
        return Err(ManagementError::Denied("authorization not granted".to_owned()));
    }
    Ok(())
}
pub(crate) async fn claim(ticket: &str, uuid: &str, target: &str) -> Result<(), ManagementError> {
    if ticket.len() != 64 {
        return Err(ManagementError::Denied(
            "missing or malformed relay ticket".to_owned(),
        ));
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
        return Err(ManagementError::Denied(
            "relay admission does not match the request".to_owned(),
        ));
    }
    Ok(())
}
pub(crate) async fn event(uuid: &str, event: &str) -> Result<(), ManagementError> {
    post("/relay/event", json!({"uuid":uuid,"event":event})).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hbb_common::tokio;
    use reqwest::StatusCode;

    #[test]
    fn status_classification_separates_refusal_from_outage() {
        assert_eq!(classify_status(StatusCode::OK), None);
        assert_eq!(classify_status(StatusCode::NO_CONTENT), None);
        for status in [
            StatusCode::BAD_REQUEST,
            StatusCode::UNAUTHORIZED,
            StatusCode::FORBIDDEN,
            StatusCode::NOT_FOUND,
            StatusCode::CONFLICT,
            StatusCode::TOO_MANY_REQUESTS,
        ] {
            assert!(
                matches!(classify_status(status), Some(ManagementError::Denied(_))),
                "{status}"
            );
        }
        for status in [
            StatusCode::INTERNAL_SERVER_ERROR,
            StatusCode::BAD_GATEWAY,
            StatusCode::SERVICE_UNAVAILABLE,
            StatusCode::GATEWAY_TIMEOUT,
            StatusCode::MOVED_PERMANENTLY,
        ] {
            assert!(
                matches!(classify_status(status), Some(ManagementError::Unavailable(_))),
                "{status}"
            );
        }
    }

    #[test]
    fn refusal_detail_is_the_bounded_api_error_text() {
        assert_eq!(
            refusal_detail(&json!({"error": "session expired"})).as_deref(),
            Some("session expired")
        );
        // Control characters cannot forge extra log lines.
        assert_eq!(
            refusal_detail(&json!({"error": "no connect\ngrant\r\u{1b}[31m"})).as_deref(),
            Some("no connectgrant[31m")
        );
        let long = "x".repeat(500);
        assert_eq!(refusal_detail(&json!({ "error": long })).map(|d| d.chars().count()), Some(120));
        for value in [
            json!({}),
            json!({"error": ""}),
            json!({"error": "\n\t"}),
            json!({"error": 403}),
            json!({"error": null}),
            json!({"message": "session expired"}),
            json!("session expired"),
            json!([]),
        ] {
            assert_eq!(refusal_detail(&value), None, "{value}");
        }
    }

    #[hbb_common::tokio::test]
    async fn malformed_ticket_is_a_refusal() {
        assert!(matches!(
            claim("short", "e4c181d0-41c7-4ae6-8ef0-36581b6b2b12", "123456789").await,
            Err(ManagementError::Denied(_))
        ));
    }
}
