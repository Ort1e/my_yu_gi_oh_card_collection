//! Messages to a Discord channel through a webhook (`DISCORD_WEBHOOK_URL`,
//! the channel's Integrations → Webhooks → Copy URL).
//!
//! The URL is a secret (whoever has it can post): never logged, errors
//! `without_url()`, no `Debug`. Messages are plain text, at most 2000
//! characters, and never ping anyone (`allowed_mentions: {parse: []}`). A 429
//! is waited for once (Discord says how long), then it is an error.

use std::time::Duration;

use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::{json, Value};

/// Discord's limit for a message's `content`, in characters.
pub const MAX_CHARS: usize = 2000;

/// The longest a 429 is waited for.
const MAX_WAIT: Duration = Duration::from_secs(10);

/// A webhook's address.
#[derive(Clone)]
pub struct Webhook(String);

impl Webhook {
    /// The webhook of `DISCORD_WEBHOOK_URL`, `None` when unset or blank.
    /// Exits on an address that isn't a Discord webhook's (a setting typed
    /// wrong should be seen at once, not when the first round is saved).
    pub fn from_env() -> Option<Webhook> {
        let value = std::env::var("DISCORD_WEBHOOK_URL").ok()?;
        match Webhook::parse(&value) {
            Ok(webhook) => webhook,
            Err(e) => {
                eprintln!("error: DISCORD_WEBHOOK_URL: {e}");
                std::process::exit(2);
            }
        }
    }

    /// `Ok(None)` for a blank value; an error (without the value) for
    /// anything but `https://discord.com/api/webhooks/<id>/<token>`.
    pub fn parse(value: &str) -> Result<Option<Webhook>, &'static str> {
        let value = value.trim().trim_end_matches('/');
        if value.is_empty() {
            return Ok(None);
        }
        let rest = [
            "https://discord.com/api/webhooks/",
            "https://discordapp.com/api/webhooks/",
        ]
        .iter()
        .find_map(|prefix| value.strip_prefix(prefix))
        .ok_or("not a Discord webhook address (https://discord.com/api/webhooks/…)")?;
        let parts: Vec<&str> = rest.split('/').collect();
        let fine = parts.len() == 2
            && !parts[0].is_empty()
            && parts[0].bytes().all(|b| b.is_ascii_digit())
            && !parts[1].is_empty()
            && parts[1]
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        if !fine {
            return Err("the address should end with /<id>/<token>");
        }
        Ok(Some(Webhook(value.to_string())))
    }

    /// Posts `content` (cut at [`MAX_CHARS`]): the new message's id.
    pub async fn post(&self, http: &reqwest::Client, content: &str) -> Result<String, String> {
        let url = format!("{}?wait=true", self.0);
        let answer = send(|| http.post(&url), content).await?;
        if !answer.status().is_success() {
            return Err(format!("Discord answered {}", answer.status()));
        }
        #[derive(Deserialize)]
        struct Message {
            id: String,
        }
        let message: Message = answer
            .json()
            .await
            .map_err(|e| format!("Discord's answer: {}", e.without_url()))?;
        Ok(message.id)
    }

    /// Replaces the text of message `id` (one this webhook posted): `false`
    /// when the message is gone (deleted in the channel).
    pub async fn edit(
        &self,
        http: &reqwest::Client,
        id: &str,
        content: &str,
    ) -> Result<bool, String> {
        if !id.bytes().all(|b| b.is_ascii_digit()) {
            return Ok(false);
        }
        let url = format!("{}/messages/{id}", self.0);
        let answer = send(|| http.patch(&url), content).await?;
        match answer.status() {
            StatusCode::NOT_FOUND => Ok(false),
            status if status.is_success() => Ok(true),
            status => Err(format!("Discord answered {status}")),
        }
    }
}

/// `content` cut to Discord's limit, with "…" when cut.
pub fn fit(content: &str) -> String {
    if content.chars().count() <= MAX_CHARS {
        return content.to_string();
    }
    let mut cut: String = content.chars().take(MAX_CHARS - 1).collect();
    cut.push('…');
    cut
}

/// One request, sent again once after a 429.
async fn send(
    request: impl Fn() -> reqwest::RequestBuilder,
    content: &str,
) -> Result<reqwest::Response, String> {
    let body = json!({ "content": fit(content), "allowed_mentions": { "parse": [] } });
    let mut waited = false;
    loop {
        let answer = request()
            .json(&body)
            .timeout(Duration::from_secs(20))
            .send()
            .await
            .map_err(|e| format!("Discord: {}", e.without_url()))?;
        if answer.status() != StatusCode::TOO_MANY_REQUESTS || waited {
            return Ok(answer);
        }
        waited = true;
        let header = answer
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<f64>().ok());
        let body = answer.json::<Value>().await.ok();
        let seconds = body
            .as_ref()
            .and_then(|b| b.get("retry_after"))
            .and_then(Value::as_f64)
            .or(header)
            .unwrap_or(1.0);
        let wait = Duration::try_from_secs_f64(seconds.max(0.0)).unwrap_or(MAX_WAIT);
        tokio::time::sleep(wait.min(MAX_WAIT)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_webhook_addresses() {
        assert!(matches!(Webhook::parse("  "), Ok(None)));
        let hook = Webhook::parse("https://discord.com/api/webhooks/123/abc-DEF_9/\n")
            .unwrap()
            .unwrap();
        assert_eq!(hook.0, "https://discord.com/api/webhooks/123/abc-DEF_9");
        assert!(Webhook::parse("https://discordapp.com/api/webhooks/1/x").is_ok());
        assert!(Webhook::parse("http://discord.com/api/webhooks/1/x").is_err());
        assert!(Webhook::parse("https://example.com/api/webhooks/1/x").is_err());
        assert!(Webhook::parse("https://discord.com/api/webhooks/1").is_err());
        assert!(Webhook::parse("https://discord.com/api/webhooks/1/x?y=1").is_err());
        assert!(Webhook::parse("https://discord.com/api/webhooks/a/x").is_err());
    }

    #[test]
    fn fits_discords_limit() {
        assert_eq!(fit("R1: W"), "R1: W");
        let long = "é".repeat(MAX_CHARS + 5);
        let cut = fit(&long);
        assert_eq!(cut.chars().count(), MAX_CHARS);
        assert!(cut.ends_with('…'));
        assert_eq!(fit(&"a".repeat(MAX_CHARS)).chars().count(), MAX_CHARS);
    }
}
