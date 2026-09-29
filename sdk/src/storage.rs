//! Content-addressed storage behind a small trait.
//!
//! First impl is the no-auth Codex REST gateway exposed by Logos Storage
//! (`/api/storage/v1`, verified against `logos-storage-nim` v0.5.0-rc1
//! docs): `POST /data` returns the CID as plain text, `GET /data/{cid}`
//! streams the bytes back. A native-FFI impl (`storage-bindings` crate,
//! full pinning/network sync) can slot in later without touching callers.

use std::{io::Read, time::Duration};

use crate::SdkError;

pub const DEFAULT_BASE_URL: &str = "http://localhost:8080/api/storage/v1";

pub trait Storage {
    /// Upload raw bytes, get the CID string back.
    fn put(&self, bytes: &[u8]) -> Result<String, SdkError>;
    /// Download by CID.
    fn get(&self, cid: &str) -> Result<Vec<u8>, SdkError>;
    /// Whether the node has the block locally.
    fn exists(&self, cid: &str) -> Result<bool, SdkError>;
}

pub struct CodexRest {
    agent: ureq::Agent,
    base_url: String,
}

impl CodexRest {
    /// `base_url` defaults from env `LOGOS_STORAGE_URL`, else localhost.
    pub fn new(base_url: Option<String>) -> Self {
        let base_url = base_url
            .or_else(|| std::env::var("LOGOS_STORAGE_URL").ok())
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
        Self {
            agent: ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(60))
                .try_proxy_from_env(true)
                .build(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    fn check(resp: ureq::Response, what: &str) -> Result<ureq::Response, SdkError> {
        let status = resp.status();
        if (200..300).contains(&status) {
            Ok(resp)
        } else {
            Err(SdkError::Storage(format!("{what}: HTTP {status}")))
        }
    }
}

impl Storage for CodexRest {
    fn put(&self, bytes: &[u8]) -> Result<String, SdkError> {
        let resp = self
            .agent
            .post(&format!("{}/data", self.base_url))
            .send_bytes(bytes)
            .map_err(|e| SdkError::Storage(format!("POST /data: {e}")))?;
        let resp = Self::check(resp, "POST /data")?;
        let cid = resp
            .into_string()
            .map_err(|e| SdkError::Storage(format!("POST /data body: {e}")))?
            .trim()
            .to_string();
        if cid.is_empty() {
            return Err(SdkError::Storage("POST /data returned empty CID".into()));
        }
        Ok(cid)
    }

    fn get(&self, cid: &str) -> Result<Vec<u8>, SdkError> {
        let resp = self
            .agent
            .get(&format!("{}/data/{cid}", self.base_url))
            .call()
            .map_err(|e| SdkError::Storage(format!("GET /data/{cid}: {e}")))?;
        let resp = Self::check(resp, "GET /data")?;
        let mut buf = Vec::new();
        resp.into_reader()
            .read_to_end(&mut buf)
            .map_err(|e| SdkError::Storage(format!("GET /data body: {e}")))?;
        Ok(buf)
    }

    fn exists(&self, cid: &str) -> Result<bool, SdkError> {
        match self
            .agent
            .get(&format!("{}/data/{cid}/exists", self.base_url))
            .call()
        {
            Ok(resp) if (200..300).contains(&resp.status()) => {
                let body = resp
                    .into_string()
                    .map_err(|e| SdkError::Storage(format!("exists body: {e}")))?;
                Ok(body.trim().eq_ignore_ascii_case("true"))
            }
            Ok(resp) if resp.status() == 404 => Ok(false),
            Ok(resp) => Err(SdkError::Storage(format!(
                "GET /data/{cid}/exists: HTTP {}",
                resp.status()
            ))),
            Err(ureq::Error::Status(404, _)) => Ok(false),
            Err(e) => Err(SdkError::Storage(format!("GET /data/{cid}/exists: {e}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_url_defaults_and_trims() {
        std::env::remove_var("LOGOS_STORAGE_URL");
        let s = CodexRest::new(None);
        assert_eq!(s.base_url, DEFAULT_BASE_URL);
        let s = CodexRest::new(Some("http://host:1234/".to_string()));
        assert_eq!(s.base_url, "http://host:1234");
    }

    /// Real gateways answer no-auth REST on a local port; run only with one
    /// up: `BASE=http://localhost:8080/api/storage/v1 cargo test rest_smoke -- --ignored`.
    #[test]
    #[ignore]
    fn rest_smoke_against_live_gateway() {
        let s = CodexRest::new(None);
        let payload = format!("provenance-sdk smoke {}", std::time::SystemTime::now().elapsed().unwrap_or_default().as_nanos());
        let cid = s.put(payload.as_bytes()).unwrap();
        assert!(cid.starts_with('z'), "unexpected CID form: {cid}");
        assert!(s.exists(&cid).unwrap());
        assert_eq!(s.get(&cid).unwrap(), payload.as_bytes());
    }
}
