use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::{path::Path, time::Duration};

#[derive(Clone)]
pub struct Api {
    client: reqwest::Client,
    base: String,
    key: String,
}
impl Api {
    pub fn new(base: &str, file: &Path) -> Result<Self> {
        let url = reqwest::Url::parse(base)?;
        if !matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "::1")) {
            bail!("The Chord API must use a loopback address")
        }
        let config: Value = serde_json::from_slice(&std::fs::read(file).with_context(|| {
            format!("Start Chord first. Config not found at {}", file.display())
        })?)?;
        let key = config["apiKey"]
            .as_str()
            .context("No Chord API key in configuration")?
            .into();
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()?;
        Ok(Self {
            client,
            base: base.trim_end_matches('/').into(),
            key,
        })
    }
    pub async fn request(&self, method: &str, route: &str, body: Option<Value>) -> Result<Value> {
        if !route.starts_with("/api/") || route.contains("..") {
            bail!("Invalid API route")
        }
        let mut req = self
            .client
            .request(method.parse()?, format!("{}{}", self.base, route))
            .bearer_auth(&self.key);
        if let Some(b) = body {
            req = req.json(&b);
        }
        let response = req
            .send()
            .await
            .context("Chord API is unavailable. Run ./chord first.")?;
        let status = response.status();
        let data: Value = response.json().await?;
        if !status.is_success() {
            bail!("{}", data["error"].as_str().unwrap_or("Request failed"))
        }
        Ok(data)
    }
    pub async fn get(&self, route: &str) -> Result<Value> {
        self.request("GET", route, None).await
    }
    pub async fn post(&self, route: &str, body: Value) -> Result<Value> {
        self.request("POST", route, Some(body)).await
    }
    pub fn conversation(id: &str, action: &str) -> String {
        format!("/api/conversations/{}/{}", urlencoding::encode(id), action)
    }
    pub async fn inbox(
        &self,
        source: &str,
        view: &str,
        query: &str,
        unread: bool,
        cursor: Option<&str>,
        server: Option<&str>,
    ) -> Result<Value> {
        let mut route = format!(
            "/api/inbox?source={}&view={}&query={}&unread={}",
            urlencoding::encode(source),
            urlencoding::encode(view),
            urlencoding::encode(query),
            unread
        );
        if let Some(cursor) = cursor {
            route.push_str(&format!("&cursor={}", urlencoding::encode(cursor)));
        }
        if let Some(server) = server {
            route.push_str(&format!("&server={}", urlencoding::encode(server)));
        }
        self.get(&route).await
    }
    pub async fn messages(&self, id: &str, cursor: Option<&str>) -> Result<Value> {
        let mut route = Self::conversation(id, "messages");
        if let Some(c) = cursor {
            route.push_str(&format!("?cursor={}", urlencoding::encode(c)))
        }
        self.get(&route).await
    }
    pub async fn send(&self, id: &str, body: Value) -> Result<Value> {
        self.post(&Self::conversation(id, "send"), body).await
    }
}
