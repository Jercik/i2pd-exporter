// Generic JSON-RPC client for I2PControl

use reqwest::header::{CONTENT_LENGTH, CONTENT_TYPE};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::time::Duration;
use thiserror::Error;

// Local utility: truncate string to at most `max` chars, respecting Unicode boundaries
fn truncate_chars(s: &str, max: usize) -> String {
    let t: String = s.chars().take(max).collect();
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        t
    }
}

// Represents an error in a JSON-RPC response
#[derive(Debug, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

// Public error type for rpc_call so callers can match on structured failures
#[derive(Debug, Error)]
pub enum RpcCallError {
    #[error("transport error: {0}")]
    Transport(#[from] reqwest::Error),

    #[error("error encoding request body for {method}: {error}")]
    Encode { error: String, method: String },

    #[error("HTTP {status} calling {method}: body: {body_snippet}")]
    Http {
        status: reqwest::StatusCode,
        method: String,
        body_snippet: String,
    },

    #[error("{method} error {code}: {message}")]
    Rpc {
        code: i32,
        message: String,
        method: String,
    },

    #[error("error decoding response body for {method}: {error}; body: {body_snippet}")]
    Decode {
        error: String,
        method: String,
        body_snippet: String,
    },
}

// Exact-one-of JSON-RPC outcome
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum RpcOutcome<T> {
    Ok { result: T },
    Err { error: RpcError },
}

fn redact_sensitive_fields(mut v: serde_json::Value) -> serde_json::Value {
    match &mut v {
        serde_json::Value::Object(map) => {
            for (k, val) in map.iter_mut() {
                if matches!(k.as_str(), "Password" | "Token") {
                    *val = serde_json::Value::String("***redacted***".to_string());
                } else {
                    *val = redact_sensitive_fields(val.take());
                }
            }
            serde_json::Value::Object(map.clone())
        }
        serde_json::Value::Array(arr) => {
            let redacted: Vec<_> = arr
                .iter_mut()
                .map(|val| redact_sensitive_fields(val.take()))
                .collect();
            serde_json::Value::Array(redacted)
        }
        _ => v,
    }
}

// Generic JSON-RPC call helper
pub async fn rpc_call<T: DeserializeOwned>(
    client: &reqwest::Client,
    url: &str,
    method: &str,
    params: serde_json::Value,
    timeout: Duration,
) -> Result<T, RpcCallError> {
    let req = serde_json::json!({
        "id": 1,
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
    });
    // Serialize up front so we always send a fixed-length body (no chunked
    // transfer) — some I2PControl servers reject chunked requests as malformed
    // JSON.
    let body = serde_json::to_vec(&req).map_err(|e| RpcCallError::Encode {
        error: e.to_string(),
        method: method.to_string(),
    })?;

    if std::env::var("DEBUG_I2PCONTROL_REQ").ok().as_deref() == Some("1") {
        let redacted = redact_sensitive_fields(req.clone());
        if let Ok(body_str) = serde_json::to_string(&redacted) {
            log::info!("{} request body: {}", method, body_str);
        }
    }

    let content_length = body.len() as u64;

    let resp = client
        .post(url)
        .header(CONTENT_TYPE, "application/json")
        .header(CONTENT_LENGTH, content_length)
        .body(body)
        .timeout(timeout)
        .send()
        .await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        let body_snippet = if method == "Authenticate" {
            // Avoid including any upstream response body in error text for auth
            String::from("<omitted>")
        } else if body.chars().count() > 2048 {
            truncate_chars(&body, 2048)
        } else {
            body.clone()
        };

        return Err(RpcCallError::Http {
            status,
            method: method.to_string(),
            body_snippet,
        });
    }
    let text = resp.text().await?;
    // Optional debug logging for RouterInfo body (can be verbose). Avoid logging Authenticate to not leak secrets.
    if std::env::var("DEBUG_I2PCONTROL_BODY").ok().as_deref() == Some("1") && method == "RouterInfo"
    {
        // Truncate to avoid excessive logs
        let snippet = if text.chars().count() > 4096 {
            truncate_chars(&text, 4096)
        } else {
            text.clone()
        };
        log::debug!("{} response body: {}", method, snippet);
    }
    let parsed: Result<RpcOutcome<T>, _> = serde_json::from_str(&text);
    match parsed {
        Ok(RpcOutcome::Ok { result }) => Ok(result),
        Ok(RpcOutcome::Err { error }) => Err(RpcCallError::Rpc {
            code: error.code,
            message: error.message,
            method: method.to_string(),
        }),
        Err(e) => {
            let body_snippet = if method == "Authenticate" {
                String::from("<omitted>")
            } else if text.chars().count() > 2048 {
                truncate_chars(&text, 2048)
            } else {
                text.clone()
            };
            Err(RpcCallError::Decode {
                error: e.to_string(),
                method: method.to_string(),
                body_snippet,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_chars() {
        assert_eq!(truncate_chars("abcd", 10), "abcd");
        assert_eq!(truncate_chars("abcdef", 4), "abcd");
        assert_eq!(truncate_chars("éèà", 2), "éè");
    }
}
