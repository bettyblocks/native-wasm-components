use std::time::Duration;

use url::Url;
use wstd::future::FutureExt;

const MAX_REDIRECTS: usize = 5;

pub struct RawResponse {
    pub status: u16,
    pub location: Option<String>,
    pub body: Vec<u8>,
}

pub trait Transport {
    async fn get(&self, url: &Url) -> Result<RawResponse, String>;
}

pub struct Fetched {
    pub final_url: Url,
    pub bytes: Vec<u8>,
}

pub struct Fetcher<T> {
    transport: T,
}

impl<T: Transport> Fetcher<T> {
    pub fn new(transport: T) -> Self {
        Self { transport }
    }

    pub fn resolve(&self, base: Option<&Url>, href: &str) -> Option<Url> {
        resolve_http_url(base, href).unwrap_or_else(|warning| {
            eprintln!("generate-pdf: {warning}");
            None
        })
    }

    #[cfg(test)]
    pub fn transport(&self) -> &T {
        &self.transport
    }

    pub async fn fetch(&self, url: &Url) -> Result<Fetched, String> {
        let mut url = url.clone();
        for _ in 0..=MAX_REDIRECTS {
            check_scheme(&url)?;
            let response = self
                .transport
                .get(&url)
                .await
                .map_err(|e| format!("{}: {e}", redact(&url)))?;
            match response.status {
                200..=299 => {
                    return Ok(Fetched {
                        final_url: url,
                        bytes: response.body,
                    });
                }
                301 | 302 | 303 | 307 | 308 => {
                    let location = response.location.ok_or_else(|| {
                        format!("{}: redirect without a Location header", redact(&url))
                    })?;
                    url = url
                        .join(&location)
                        .map_err(|e| format!("{}: bad redirect: {e}", redact(&url)))?;
                }
                status => return Err(format!("{}: HTTP {status}", redact(&url))),
            }
        }
        Err(format!(
            "{}: more than {MAX_REDIRECTS} redirects",
            redact(&url)
        ))
    }
}

fn check_scheme(url: &Url) -> Result<(), String> {
    if is_http(url) {
        Ok(())
    } else {
        Err(format!(
            "{}: scheme {} is not supported",
            redact(url),
            url.scheme()
        ))
    }
}

pub fn is_http(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https")
}

pub fn resolve_http_url(base: Option<&Url>, href: &str) -> Result<Option<Url>, String> {
    let href = href.trim();
    if needs_no_download(href) {
        return Ok(None);
    }
    let url = match base {
        Some(base) => base.join(href),
        None if href.starts_with("//") => Url::parse(&format!("https:{href}")),
        None => Url::parse(href),
    };
    match url {
        Ok(url) if is_http(&url) => Ok(Some(url)),
        Ok(_) => Err(format!("{href}: not an http(s) URL, left out")),
        Err(url::ParseError::RelativeUrlWithoutBase) => Err(format!(
            "{href}: relative URL without <base href>, left out"
        )),
        Err(_) => Err(format!("{href}: not a valid URL, left out")),
    }
}

fn needs_no_download(href: &str) -> bool {
    href.is_empty() || href.starts_with('#') || href.starts_with("data:")
}

pub fn redact(url: &Url) -> String {
    let mut url = url.clone();
    url.set_query(None);
    url.set_fragment(None);
    url.to_string()
}

const DOCUMENT_DOWNLOAD_DEADLINE: Duration = Duration::from_secs(20);
const NON_BROWSER_USER_AGENT: &str = "betty-blocks-pdf-generator";

pub struct WstdTransport {
    client: wstd::http::Client,
    deadline: wstd::time::Instant,
}

impl WstdTransport {
    pub fn new() -> Self {
        let mut client = wstd::http::Client::new();
        client.set_connect_timeout(Duration::from_secs(5));
        client.set_first_byte_timeout(Duration::from_secs(10));
        client.set_between_bytes_timeout(Duration::from_secs(10));
        let deadline =
            wstd::time::Instant::now() + wstd::time::Duration::from(DOCUMENT_DOWNLOAD_DEADLINE);
        Self { client, deadline }
    }

    async fn get(&self, url: &Url) -> Result<RawResponse, String> {
        let request = wstd::http::Request::get(url.as_str())
            .header("user-agent", NON_BROWSER_USER_AGENT)
            .body(wstd::http::Body::empty())
            .map_err(|e| e.to_string())?;
        let mut response = self.client.send(request).await.map_err(|e| e.to_string())?;
        let status = response.status().as_u16();
        let location = response
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let body = response
            .body_mut()
            .contents()
            .await
            .map_err(|e| e.to_string())?
            .to_vec();
        Ok(RawResponse {
            status,
            location,
            body,
        })
    }
}

impl Transport for WstdTransport {
    async fn get(&self, url: &Url) -> Result<RawResponse, String> {
        WstdTransport::get(self, url)
            .timeout(self.deadline)
            .await
            .map_err(|_| {
                format!(
                    "not done within the {}s download deadline",
                    DOCUMENT_DOWNLOAD_DEADLINE.as_secs()
                )
            })?
    }
}

#[cfg(test)]
pub mod mock {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use super::*;

    #[derive(Default)]
    pub struct MockTransport {
        pub routes: HashMap<String, (u16, Option<String>, Vec<u8>)>,
        pub requested: RefCell<Vec<String>>,
    }

    impl MockTransport {
        pub fn ok(mut self, url: &str, body: impl Into<Vec<u8>>) -> Self {
            self.routes
                .insert(url.to_string(), (200, None, body.into()));
            self
        }

        pub fn redirect(mut self, url: &str, location: &str) -> Self {
            self.routes.insert(
                url.to_string(),
                (302, Some(location.to_string()), Vec::new()),
            );
            self
        }
    }

    impl Transport for MockTransport {
        async fn get(&self, url: &Url) -> Result<RawResponse, String> {
            self.requested.borrow_mut().push(url.to_string());
            let Some((status, location, body)) = self.routes.get(url.as_str()).cloned() else {
                return Ok(RawResponse {
                    status: 404,
                    location: None,
                    body: Vec::new(),
                });
            };
            Ok(RawResponse {
                status,
                location,
                body,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::mock::MockTransport;
    use super::*;

    fn fetch(fetcher: &Fetcher<MockTransport>, url: &str) -> Result<Fetched, String> {
        futures::executor::block_on(fetcher.fetch(&Url::parse(url).unwrap()))
    }

    #[test]
    fn follows_relative_redirects_and_reports_the_final_url() {
        let transport = MockTransport::default()
            .redirect("https://a.test/old/logo.png", "../new/logo.png")
            .ok("https://a.test/new/logo.png", b"png".to_vec());
        let fetcher = Fetcher::new(transport);
        let fetched = fetch(&fetcher, "https://a.test/old/logo.png").unwrap();
        assert_eq!(fetched.final_url.as_str(), "https://a.test/new/logo.png");
        assert_eq!(fetched.bytes, b"png");
    }

    #[test]
    fn stops_redirect_loops_and_non_http_redirects() {
        let transport = MockTransport::default()
            .redirect("https://a.test/a", "/b")
            .redirect("https://a.test/b", "/a")
            .redirect("https://a.test/file", "file:///etc/passwd");
        let fetcher = Fetcher::new(transport);
        let err = fetch(&fetcher, "https://a.test/a").err().unwrap();
        assert!(err.contains("more than 5 redirects"), "{err}");
        let err = fetch(&fetcher, "https://a.test/file").err().unwrap();
        assert!(err.contains("scheme file is not supported"), "{err}");
    }

    #[test]
    fn resolves_references_and_names_why_one_cannot_be() {
        let base = Url::parse("https://a.test/css/site.css").unwrap();
        assert_eq!(
            resolve_http_url(Some(&base), "../img/x.png")
                .unwrap()
                .unwrap()
                .as_str(),
            "https://a.test/img/x.png"
        );
        assert_eq!(
            resolve_http_url(None, "//cdn.test/x.png")
                .unwrap()
                .unwrap()
                .as_str(),
            "https://cdn.test/x.png"
        );
        assert_eq!(resolve_http_url(Some(&base), "data:,x"), Ok(None));
        assert_eq!(resolve_http_url(Some(&base), "#top"), Ok(None));
        assert_eq!(
            resolve_http_url(None, "img/x.png").unwrap_err(),
            "img/x.png: relative URL without <base href>, left out"
        );
        assert_eq!(
            resolve_http_url(Some(&base), "ftp://a.test/x").unwrap_err(),
            "ftp://a.test/x: not an http(s) URL, left out"
        );
        assert_eq!(
            resolve_http_url(None, "http://[bad").unwrap_err(),
            "http://[bad: not a valid URL, left out"
        );
    }

    #[test]
    fn redacts_query_strings_from_errors() {
        let fetcher = Fetcher::new(MockTransport::default());
        let err = fetch(&fetcher, "https://a.test/x.png?sig=SECRET")
            .err()
            .unwrap();
        assert_eq!(err, "https://a.test/x.png: HTTP 404");
    }
}
