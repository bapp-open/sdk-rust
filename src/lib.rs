use rand::Rng;
use reqwest::{multipart, Client, Method, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::time::Duration;
use url::Url;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::time::sleep;

/// Paginated list response.
#[derive(Debug, Deserialize, Serialize)]
pub struct PagedList {
    pub results: Vec<Value>,
    pub count: i64,
    pub next: Option<String>,
    pub previous: Option<String>,
}

/// BAPP Auto API client.
#[derive(Debug)]
pub struct BappApiClient {
    pub host: String,
    pub tenant: Option<String>,
    pub app: String,
    auth_header: Option<String>,
    user_agent: Option<String>,
    max_retries: usize,
    client: Client,
}

impl BappApiClient {
    /// Create a new client with the default host.
    pub fn new() -> Self {
        Self {
            host: "https://panel.bapp.ro/api".to_string(),
            tenant: None,
            app: "account".to_string(),
            auth_header: None,
            user_agent: None,
            max_retries: 3,
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("failed to build HTTP client"),
        }
    }

    /// Create a new client pointing at `host`.
    pub fn with_host(mut self, host: &str) -> Self {
        self.host = host.trim_end_matches('/').to_string();
        self
    }

    /// Set Bearer token authentication.
    pub fn with_bearer(mut self, token: &str) -> Self {
        self.auth_header = Some(format!("Bearer {}", token));
        self
    }

    /// Set Token-based authentication.
    pub fn with_token(mut self, token: &str) -> Self {
        self.auth_header = Some(format!("Token {}", token));
        self
    }

    /// Set the default tenant ID.
    pub fn with_tenant(mut self, tenant: &str) -> Self {
        self.tenant = Some(tenant.to_string());
        self
    }

    /// Set the default app slug.
    pub fn with_app(mut self, app: &str) -> Self {
        self.app = app.to_string();
        self
    }

    /// Set a custom User-Agent header.
    pub fn with_user_agent(mut self, ua: &str) -> Self {
        self.user_agent = Some(ua.to_string());
        self
    }

    /// Set the HTTP client timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.client = Client::builder()
            .timeout(timeout)
            .build()
            .expect("failed to build HTTP client");
        self
    }

    /// Set the maximum number of retries on transient errors.
    pub fn with_max_retries(mut self, n: usize) -> Self {
        self.max_retries = n;
        self
    }

    fn build_request(
        &self,
        method: Method,
        path: &str,
        params: Option<&[(&str, &str)]>,
        extra_headers: Option<&[(&str, &str)]>,
    ) -> reqwest::RequestBuilder {
        let url = format!("{}{}", self.host, path);
        let mut req = self.client.request(method, &url);
        if let Some(p) = params {
            req = req.query(p);
        }
        if let Some(ua) = &self.user_agent {
            req = req.header("User-Agent", ua);
        }
        if let Some(auth) = &self.auth_header {
            req = req.header("Authorization", auth);
        }
        if let Some(t) = &self.tenant {
            req = req.header("x-tenant-id", t);
        }
        req = req.header("x-app-slug", &self.app);
        if let Some(extra) = extra_headers {
            for (k, v) in extra {
                req = req.header(*k, *v);
            }
        }
        req
    }

    async fn request(
        &self,
        method: Method,
        path: &str,
        params: Option<&[(&str, &str)]>,
        body: Option<&Value>,
        extra_headers: Option<&[(&str, &str)]>,
    ) -> Result<Option<Value>, reqwest::Error> {
        let mut rng = rand::rng();
        let mut last_err = None;

        for attempt in 0..=self.max_retries {
            let mut req = self.build_request(method.clone(), path, params, extra_headers);
            if let Some(b) = body {
                req = req.json(b);
            }

            match req.send().await {
                Ok(resp) => {
                    let status = resp.status();
                    if (status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error())
                        && attempt < self.max_retries
                    {
                        let backoff = f64::min(
                            2f64.powi(attempt as i32) + rng.random::<f64>(),
                            10.0,
                        );
                        sleep(Duration::from_secs_f64(backoff)).await;
                        continue;
                    }
                    let resp = resp.error_for_status()?;
                    if resp.status() == StatusCode::NO_CONTENT {
                        return Ok(None);
                    }
                    let data = resp.json::<Value>().await?;
                    return Ok(Some(data));
                }
                Err(e) => {
                    last_err = Some(e);
                    if attempt < self.max_retries {
                        let backoff = f64::min(
                            2f64.powi(attempt as i32) + rng.random::<f64>(),
                            10.0,
                        );
                        sleep(Duration::from_secs_f64(backoff)).await;
                        continue;
                    }
                }
            }
        }
        Err(last_err.unwrap())
    }

    /// Send a multipart/form-data request. Use for file uploads.
    /// `fields` are plain text fields, `files` are `(field_name, file_path)` pairs.
    pub async fn request_multipart(
        &self,
        method: Method,
        path: &str,
        fields: &[(&str, &str)],
        files: &[(&str, &str)],
    ) -> Result<Option<Value>, reqwest::Error> {
        let req = self.build_request(method, path, None, None);
        let mut form = multipart::Form::new();
        for (k, v) in fields {
            form = form.text(k.to_string(), v.to_string());
        }
        for (field, file_path) in files {
            let path = Path::new(file_path);
            let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();
            let bytes = fs::read(path).await.expect("failed to read file");
            let part = multipart::Part::bytes(bytes).file_name(filename);
            form = form.part(field.to_string(), part);
        }
        let resp = req.multipart(form).send().await?.error_for_status()?;
        if resp.status() == StatusCode::NO_CONTENT {
            return Ok(None);
        }
        let data = resp.json::<Value>().await?;
        Ok(Some(data))
    }

    // -- user ---------------------------------------------------------------

    /// Get current user profile.
    pub async fn me(&self) -> Result<Option<Value>, reqwest::Error> {
        self.request(
            Method::GET,
            "/tasks/bapp_framework.me",
            None, None,
            Some(&[("x-app-slug", "")]),
        ).await
    }

    // -- app ----------------------------------------------------------------

    /// Get app configuration by slug.
    pub async fn get_app(&self, app_slug: &str) -> Result<Option<Value>, reqwest::Error> {
        self.request(
            Method::GET,
            "/tasks/bapp_framework.getapp",
            None, None,
            Some(&[("x-app-slug", app_slug)]),
        ).await
    }

    // -- entity introspect --------------------------------------------------

    /// Get entity list introspect for a content type.
    pub async fn list_introspect(&self, content_type: &str) -> Result<Option<Value>, reqwest::Error> {
        self.request(
            Method::GET,
            "/tasks/bapp_framework.listintrospect",
            Some(&[("ct", content_type)]),
            None, None,
        ).await
    }

    /// Get entity detail introspect for a content type.
    pub async fn detail_introspect(
        &self, content_type: &str, pk: Option<&str>,
    ) -> Result<Option<Value>, reqwest::Error> {
        let mut params = vec![("ct", content_type)];
        if let Some(pk) = pk {
            params.push(("pk", pk));
        }
        self.request(
            Method::GET,
            "/tasks/bapp_framework.detailintrospect",
            Some(&params), None, None,
        ).await
    }

    // -- entity CRUD --------------------------------------------------------

    /// List entities of a content type. Returns a [PagedList].
    pub async fn list(
        &self, content_type: &str, filters: Option<&[(&str, &str)]>,
    ) -> Result<PagedList, Box<dyn std::error::Error>> {
        let path = format!("/content-type/{}/", content_type);
        let req = self.build_request(Method::GET, &path, filters, None);
        let resp = req.send().await?.error_for_status()?;
        let paged: PagedList = resp.json().await?;
        Ok(paged)
    }

    /// Get a single entity by content type and ID.
    pub async fn get(
        &self, content_type: &str, id: &str,
    ) -> Result<Option<Value>, reqwest::Error> {
        let path = format!("/content-type/{}/{}/", content_type, id);
        self.request(Method::GET, &path, None, None, None).await
    }

    /// Create a new entity.
    pub async fn create(
        &self, content_type: &str, data: Option<&Value>,
    ) -> Result<Option<Value>, reqwest::Error> {
        let path = format!("/content-type/{}/", content_type);
        self.request(Method::POST, &path, None, data, None).await
    }

    /// Full update of an entity.
    pub async fn update(
        &self, content_type: &str, id: &str, data: Option<&Value>,
    ) -> Result<Option<Value>, reqwest::Error> {
        let path = format!("/content-type/{}/{}/", content_type, id);
        self.request(Method::PUT, &path, None, data, None).await
    }

    /// Partial update of an entity.
    pub async fn patch(
        &self, content_type: &str, id: &str, data: Option<&Value>,
    ) -> Result<Option<Value>, reqwest::Error> {
        let path = format!("/content-type/{}/{}/", content_type, id);
        self.request(Method::PATCH, &path, None, data, None).await
    }

    /// Delete an entity.
    pub async fn delete(
        &self, content_type: &str, id: &str,
    ) -> Result<Option<Value>, reqwest::Error> {
        let path = format!("/content-type/{}/{}/", content_type, id);
        self.request(Method::DELETE, &path, None, None, None).await
    }

    // -- document views -----------------------------------------------------

    /// Extract available document views from a record.
    ///
    /// Works with both `public_view` (new) and `view_token` (legacy) formats.
    /// Returns a Vec of JSON objects with keys: `label`, `token`, `type`,
    /// `variations`, and `default_variation`.
    pub fn get_document_views(record: &Value) -> Vec<Value> {
        let mut views = Vec::new();

        if let Some(public_views) = record.get("public_view").and_then(|v| v.as_array()) {
            for entry in public_views {
                views.push(serde_json::json!({
                    "label": entry.get("label").and_then(|v| v.as_str()).unwrap_or(""),
                    "token": entry.get("view_token").and_then(|v| v.as_str()).unwrap_or(""),
                    "type": "public_view",
                    "variations": entry.get("variations").cloned().unwrap_or(Value::Null),
                    "default_variation": entry.get("default_variation").cloned().unwrap_or(Value::Null),
                }));
            }
        }

        if let Some(view_tokens) = record.get("view_token").and_then(|v| v.as_array()) {
            for entry in view_tokens {
                views.push(serde_json::json!({
                    "label": entry.get("label").and_then(|v| v.as_str()).unwrap_or(""),
                    "token": entry.get("view_token").and_then(|v| v.as_str()).unwrap_or(""),
                    "type": "view_token",
                    "variations": null,
                    "default_variation": null,
                }));
            }
        }

        views
    }

    /// Build a document render/download URL from a record.
    ///
    /// Works with both `public_view` and `view_token` formats.
    /// Prefers `public_view` when both are present on a record.
    ///
    /// - `output`: `"html"`, `"pdf"`, `"jpg"`, or `"context"`.
    /// - `label`: select a specific view by label (`None` = first available).
    /// - `variation`: variation code for `public_view` entries (e.g. `"v4"`).
    pub fn get_document_url(
        &self,
        record: &Value,
        output: &str,
        label: Option<&str>,
        variation: Option<&str>,
        download: bool,
    ) -> Option<String> {
        let views = Self::get_document_views(record);
        if views.is_empty() {
            return None;
        }

        let view = if let Some(label) = label {
            views.iter()
                .find(|v| v.get("label").and_then(|l| l.as_str()) == Some(label))
                .unwrap_or(&views[0])
        } else {
            &views[0]
        };

        let token = view.get("token").and_then(|v| v.as_str()).unwrap_or("");
        if token.is_empty() {
            return None;
        }

        let view_type = view.get("type").and_then(|v| v.as_str()).unwrap_or("");

        if view_type == "public_view" {
            let base = format!("{}/render/{}", self.host, token);
            let mut u = Url::parse(&base).ok()?;
            u.query_pairs_mut().append_pair("output", output);
            let effective_variation = variation
                .map(|s| s.to_string())
                .or_else(|| {
                    view.get("default_variation")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                });
            if let Some(v) = effective_variation {
                u.query_pairs_mut().append_pair("variation", &v);
            }
            if download {
                u.query_pairs_mut().append_pair("download", "true");
            }
            return Some(u.to_string());
        }

        // Legacy view_token
        let action = match (output, download) {
            ("pdf", true) => "pdf.download",
            ("pdf", false) => "pdf.view",
            ("context", _) => "pdf.context",
            _ => "pdf.preview",
        };
        let base = format!("{}/documents/{}", self.host, action);
        let mut u = Url::parse(&base).ok()?;
        u.query_pairs_mut().append_pair("token", token);
        Some(u.to_string())
    }

    /// Fetch document content (PDF, HTML, JPG, etc.) as bytes.
    ///
    /// Builds the URL via [`get_document_url`] and performs a plain GET request.
    /// Returns `Ok(None)` when the record has no view tokens.
    pub async fn get_document_content(
        &self,
        record: &Value,
        output: &str,
        label: Option<&str>,
        variation: Option<&str>,
        download: bool,
    ) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error>> {
        let url = match self.get_document_url(record, output, label, variation, download) {
            Some(u) => u,
            None => return Ok(None),
        };
        let resp = self.client.get(&url).send().await?.error_for_status()?;
        let bytes = resp.bytes().await?;
        Ok(Some(bytes.to_vec()))
    }

    /// Stream document content directly to a file.
    ///
    /// Like [`get_document_content`] but streams to `dest` without buffering
    /// the entire document in memory.
    pub async fn download_document(
        &self,
        record: &Value,
        dest: &str,
        output: &str,
        label: Option<&str>,
        variation: Option<&str>,
        download: bool,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let url = match self.get_document_url(record, output, label, variation, download) {
            Some(u) => u,
            None => return Ok(false),
        };
        let resp = self.client.get(&url).send().await?.error_for_status()?;
        let mut file = tokio::fs::File::create(dest).await?;
        let bytes = resp.bytes().await?;
        file.write_all(&bytes).await?;
        Ok(true)
    }

    // -- tasks --------------------------------------------------------------

    /// List all available task codes.
    pub async fn list_tasks(&self) -> Result<Option<Value>, reqwest::Error> {
        self.request(Method::GET, "/tasks", None, None, None).await
    }

    /// Get task configuration by code.
    pub async fn detail_task(&self, code: &str) -> Result<Option<Value>, reqwest::Error> {
        let path = format!("/tasks/{}", code);
        self.request(Method::OPTIONS, &path, None, None, None).await
    }

    /// Run a task. Uses GET when no payload, POST otherwise.
    pub async fn run_task(
        &self, code: &str, payload: Option<&Value>,
    ) -> Result<Option<Value>, reqwest::Error> {
        let path = format!("/tasks/{}", code);
        let method = if payload.is_some() { Method::POST } else { Method::GET };
        self.request(method, &path, None, payload, None).await
    }

    /// Run a long-running task and poll until finished.
    /// Returns the final task data which includes "file" when the task produces a download.
    pub async fn run_task_async(
        &self,
        code: &str,
        payload: Option<&Value>,
        poll_interval: Option<Duration>,
        timeout: Option<Duration>,
    ) -> Result<Value, Box<dyn std::error::Error>> {
        let poll = poll_interval.unwrap_or(Duration::from_secs(1));
        let tout = timeout.unwrap_or(Duration::from_secs(300));

        let result = self.run_task(code, payload).await?;
        let task_id = result
            .as_ref()
            .and_then(|v| v.get("id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let task_id = match task_id {
            Some(id) => id,
            None => return Ok(result.unwrap_or(Value::Null)),
        };

        let deadline = tokio::time::Instant::now() + tout;
        loop {
            sleep(poll).await;
            if tokio::time::Instant::now() > deadline {
                return Err(format!("Task {} ({}) did not finish within {:?}", code, task_id, tout).into());
            }
            let page = self.list("bapp_framework.taskdata", Some(&[("id", &task_id)])).await?;
            if page.results.is_empty() {
                continue;
            }
            let data = &page.results[0];
            if data.get("failed").and_then(|v| v.as_bool()).unwrap_or(false) {
                let msg = data.get("message").and_then(|v| v.as_str()).unwrap_or("");
                return Err(format!("Task {} failed: {}", code, msg).into());
            }
            if data.get("finished").and_then(|v| v.as_bool()).unwrap_or(false) {
                return Ok(data.clone());
            }
        }
    }
}

impl Default for BappApiClient {
    fn default() -> Self {
        Self::new()
    }
}
