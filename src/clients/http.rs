// One job: turn a render payload's `logoUrl` into image bytes a template can
// reference as a local file. Typst compiles fully offline —
// `image("https://…")` fails with "network access is not supported" — so the
// bytes are produced here, `modules::render::service` hands them to that one
// compile, and they are dropped afterwards. Nothing is cached or written to
// disk by this module.
//
// Two `logoUrl` shapes are accepted:
//   - `http(s)://…`     — `fetch_image`: a real download (per-call
//                          `reqwest::Client`; remote images are rare).
//   - `data:image/…;base64,…` — `decode_data_uri`: no network at all, just
//                          a base64 decode (a shop that stores its logo
//                          inline, e.g. MyroLogic POS's `shopProfile.logoBase64`).
//
// SSRF note: every caller is `InternalCaller`-gated (the trusted backend is
// the only one), so the http path's guards are deliberately modest — http(s)
// only, a byte cap, a wall-clock timeout, and an `image/*` content-type
// check. It does **not** block private-IP or link-local targets; a
// deployment that exposes this to less-trusted callers must add that.

use std::time::Duration;

use base64::Engine as _;

/// A downloaded image plus the file extension implied by its `Content-Type`
/// — the extension matters because Typst picks the decoder from it (or from
/// explicit `format:`), and `modules::render::service` names the in-memory
/// file `logo.<extension>`.
#[derive(Debug)]
pub struct FetchedImage {
    pub bytes: Vec<u8>,
    pub extension: &'static str,
}

#[derive(Debug, thiserror::Error)]
pub enum HttpClientError {
    #[error("logoUrl must be an http(s) URL")]
    UnsupportedScheme,
    #[error("could not reach logoUrl: {0}")]
    Request(String),
    #[error("logoUrl responded {0}")]
    Status(u16),
    #[error("logoUrl is not an image (Content-Type: {0})")]
    NotAnImage(String),
    #[error("logoUrl image is larger than the {limit}-byte limit")]
    TooLarge { limit: usize },
    #[error("logoUrl is a malformed data: URI ({0})")]
    MalformedDataUri(String),
}

/// GETs `url` under `max_bytes` / `timeout` caps and returns the image bytes.
/// Follows redirects (reqwest's default policy). Fails closed on anything
/// that isn't a 2xx `image/*` body within the caps.
pub async fn fetch_image(
    url: &str,
    max_bytes: usize,
    timeout: Duration,
) -> Result<FetchedImage, HttpClientError> {
    let scheme_ok = url
        .split_once("://")
        .map(|(scheme, _)| {
            scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")
        })
        .unwrap_or(false);
    if !scheme_ok {
        return Err(HttpClientError::UnsupportedScheme);
    }

    let client = reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|err| HttpClientError::Request(err.to_string()))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|err| HttpClientError::Request(err.to_string()))?;

    if !response.status().is_success() {
        return Err(HttpClientError::Status(response.status().as_u16()));
    }

    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string();
    let extension = image_extension(&content_type)
        .ok_or_else(|| HttpClientError::NotAnImage(content_type.clone()))?;

    // Reject early when the server declares an oversized body...
    if let Some(len) = response.content_length()
        && len as usize > max_bytes
    {
        return Err(HttpClientError::TooLarge { limit: max_bytes });
    }

    // ...and enforce it again on the actual bytes (Content-Length can lie or
    // be absent).
    let bytes = response
        .bytes()
        .await
        .map_err(|err| HttpClientError::Request(err.to_string()))?;
    if bytes.len() > max_bytes {
        return Err(HttpClientError::TooLarge { limit: max_bytes });
    }

    Ok(FetchedImage {
        bytes: bytes.to_vec(),
        extension,
    })
}

/// Decodes a `data:<mediatype>;base64,<payload>` URI into image bytes — no
/// network. Only base64 payloads are supported (a percent-encoded `data:`
/// URI carrying an image is not a shape anything sends here). The `mediatype`
/// must be a supported `image/*`, and the decoded length is held to the same
/// `max_bytes` cap as `fetch_image`.
pub fn decode_data_uri(uri: &str, max_bytes: usize) -> Result<FetchedImage, HttpClientError> {
    let rest = uri
        .strip_prefix("data:")
        .ok_or_else(|| HttpClientError::MalformedDataUri("missing 'data:' prefix".to_string()))?;
    let (meta, payload) = rest
        .split_once(',')
        .ok_or_else(|| HttpClientError::MalformedDataUri("missing ',' separator".to_string()))?;

    let mediatype = meta.split(';').next().unwrap_or("").trim();
    if !meta
        .split(';')
        .any(|token| token.trim().eq_ignore_ascii_case("base64"))
    {
        return Err(HttpClientError::MalformedDataUri(
            "only ';base64' data URIs are supported".to_string(),
        ));
    }
    let extension = image_extension(mediatype)
        .ok_or_else(|| HttpClientError::NotAnImage(mediatype.to_string()))?;

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload.trim())
        .map_err(|err| HttpClientError::MalformedDataUri(err.to_string()))?;
    if bytes.len() > max_bytes {
        return Err(HttpClientError::TooLarge { limit: max_bytes });
    }

    Ok(FetchedImage { bytes, extension })
}

/// Maps an image `Content-Type` / data-URI mediatype (ignoring any `; …`
/// suffix) to the file extension Typst expects. `None` for anything not a
/// supported image.
fn image_extension(content_type: &str) -> Option<&'static str> {
    let mime = content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    match mime.as_str() {
        "image/png" => Some("png"),
        "image/jpeg" | "image/jpg" => Some("jpg"),
        "image/svg+xml" => Some("svg"),
        "image/webp" => Some("webp"),
        "image/gif" => Some("gif"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rejects_non_http_scheme() {
        let err = fetch_image("file:///etc/passwd", 1024, Duration::from_secs(1))
            .await
            .unwrap_err();
        assert!(matches!(err, HttpClientError::UnsupportedScheme));
    }

    #[test]
    fn maps_known_image_types() {
        assert_eq!(image_extension("image/png"), Some("png"));
        assert_eq!(image_extension("image/jpeg; charset=binary"), Some("jpg"));
        assert_eq!(image_extension("image/svg+xml"), Some("svg"));
        assert_eq!(image_extension("text/html"), None);
    }

    // A 1x1 transparent PNG.
    const TINY_PNG_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

    #[test]
    fn decodes_a_valid_base64_data_uri() {
        let uri = format!("data:image/png;base64,{TINY_PNG_B64}");
        let img = decode_data_uri(&uri, 1024).expect("valid data URI decodes");
        assert_eq!(img.extension, "png");
        assert!(img.bytes.starts_with(b"\x89PNG"));
    }

    #[test]
    fn rejects_data_uri_without_base64_marker() {
        let err = decode_data_uri("data:image/png,notbase64", 1024).unwrap_err();
        assert!(matches!(err, HttpClientError::MalformedDataUri(_)));
    }

    #[test]
    fn rejects_data_uri_with_non_image_mediatype() {
        let err = decode_data_uri("data:text/plain;base64,aGk=", 1024).unwrap_err();
        assert!(matches!(err, HttpClientError::NotAnImage(_)));
    }

    #[test]
    fn rejects_oversized_data_uri() {
        let uri = format!("data:image/png;base64,{TINY_PNG_B64}");
        let err = decode_data_uri(&uri, 8).unwrap_err();
        assert!(matches!(err, HttpClientError::TooLarge { limit: 8 }));
    }
}
