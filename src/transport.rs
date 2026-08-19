use crate::error::{Error, Result};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Delete,
}

#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpRequest {
    pub fn get(url: impl Into<String>) -> Self {
        Self {
            method: Method::Get,
            url: url.into(),
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    pub fn post(url: impl Into<String>, body: Vec<u8>, content_type: &str) -> Self {
        Self {
            method: Method::Post,
            url: url.into(),
            headers: vec![("Content-Type".into(), content_type.into())],
            body,
        }
    }
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn header(&self, name: &str) -> Option<&str> {
        let want = name.to_ascii_lowercase();
        self.headers
            .iter()
            .find(|(k, _)| k.to_ascii_lowercase() == want)
            .map(|(_, v)| v.as_str())
    }

    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

pub trait Transport: Send + Sync {
    fn execute(&self, req: HttpRequest) -> Result<HttpResponse>;

    fn get(&self, url: &str) -> Result<HttpResponse> {
        self.execute(HttpRequest::get(url))
    }

    fn post(&self, url: &str, body: &[u8], content_type: &str) -> Result<HttpResponse> {
        self.execute(HttpRequest::post(url, body.to_vec(), content_type))
    }
}

/// Blocking HTTP via ureq. Used by the CLI, GUI, and eSCL facade backend.
pub struct UreqTransport {
    timeout: Duration,
}

impl Default for UreqTransport {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(60),
        }
    }
}

impl UreqTransport {
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }

    fn agent(&self) -> ureq::Agent {
        ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(5))
            .timeout(self.timeout)
            .try_proxy_from_env(false)
            .user_agent("hp-m177/0.1")
            .build()
    }
}

impl Transport for UreqTransport {
    fn execute(&self, req: HttpRequest) -> Result<HttpResponse> {
        let agent = self.agent();
        let mut call = match req.method {
            Method::Get => agent.get(&req.url),
            Method::Post => agent.post(&req.url),
            Method::Delete => agent.request("DELETE", &req.url),
        };
        for (k, v) in &req.headers {
            call = call.set(k, v);
        }
        let result = if req.method == Method::Post {
            call.send_bytes(&req.body)
        } else {
            call.call()
        };
        match result {
            Ok(resp) => read_ureq(resp, &req.url),
            Err(ureq::Error::Status(code, resp)) => {
                let parsed = read_ureq(resp, &req.url)?;
                Err(Error::Http {
                    status: code as u16,
                    url: req.url,
                    detail: parsed.text(),
                })
            }
            Err(ureq::Error::Transport(t)) => Err(Error::Transport {
                url: req.url,
                detail: t.to_string(),
            }),
        }
    }
}

fn read_ureq(resp: ureq::Response, url: &str) -> Result<HttpResponse> {
    let status = resp.status();
    let mut headers = Vec::new();
    for name in [
        "Content-Type",
        "Location",
        "Content-Length",
        "Server",
        "SOAPAction",
    ] {
        if let Some(v) = resp.header(name) {
            headers.push((name.to_string(), v.to_string()));
        }
    }
    let mut body = Vec::new();
    resp.into_reader()
        .read_to_end(&mut body)
        .map_err(|e| Error::Transport {
            url: url.to_string(),
            detail: e.to_string(),
        })?;
    Ok(HttpResponse {
        status,
        headers,
        body,
    })
}

/// Transport that maps URLs to a callback. Used by unit tests that do not
/// want a listening socket.
pub struct FnTransport<F>
where
    F: Fn(HttpRequest) -> Result<HttpResponse> + Send + Sync,
{
    pub f: F,
}

impl<F> Transport for FnTransport<F>
where
    F: Fn(HttpRequest) -> Result<HttpResponse> + Send + Sync,
{
    fn execute(&self, req: HttpRequest) -> Result<HttpResponse> {
        (self.f)(req)
    }
}
