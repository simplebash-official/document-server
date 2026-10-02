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
//                          inline, e.g. SimpleBash POS's `shopProfile.logoBase64`).
//
// SSRF note: every caller is `InternalCaller`-gated, but the URL itself
// comes from a render payload, so it is treated as untrusted. Unless
// `ALLOW_PRIVATE_IP_IMAGES=true` (local development only), `fetch_image`
// resolves the host once, refuses any loopback / private / link-local /
// reserved address (incl. IPv4-mapped IPv6 and NAT64 forms), pins the
// connection to that already-checked address (so a second DNS answer can't
// rebind it), and refuses redirects (a redirect target would skip the
// check). On top of that: http(s) only, a streamed byte cap, a wall-clock
// timeout and an `image/*` content-type check.

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
    #[error("logoUrl targets a private or forbidden IP address")]
    ForbiddenAddress,
}

fn is_forbidden_ipv4(ip: std::net::Ipv4Addr) -> bool {
    let o = ip.octets();
    o[0] == 0 // 0.0.0.0/8 "this network"
        || o[0] == 10
        || o[0] == 127
        || (o[0] == 100 && (64..=127).contains(&o[1])) // CGNAT 100.64/10
        || (o[0] == 169 && o[1] == 254) // link-local, incl. cloud metadata
        || (o[0] == 172 && (16..=31).contains(&o[1]))
        || (o[0] == 192 && o[1] == 0 && o[2] == 0) // IETF protocol assignments
        || (o[0] == 192 && o[1] == 0 && o[2] == 2) // TEST-NET-1
        || (o[0] == 192 && o[1] == 168)
        || (o[0] == 198 && (18..=19).contains(&o[1])) // benchmarking
        || (o[0] == 198 && o[1] == 51 && o[2] == 100) // TEST-NET-2
        || (o[0] == 203 && o[1] == 0 && o[2] == 113) // TEST-NET-3
        || o[0] >= 224 // multicast 224/4, reserved 240/4, broadcast
}

fn is_private_or_restricted_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(ipv4) => is_forbidden_ipv4(ipv4),
        std::net::IpAddr::V6(ipv6) => {
            // `::ffff:a.b.c.d` (and the deprecated `::a.b.c.d`) reach the
            // embedded IPv4 address, so judge them by that address.
            if let Some(v4) = ipv6.to_ipv4() {
                return is_forbidden_ipv4(v4);
            }
            let seg = ipv6.segments();
            // NAT64 64:ff9b::/96 also embeds an IPv4 address in the low 32 bits.
            if seg[0] == 0x64 && seg[1] == 0xff9b && seg[2..6].iter().all(|s| *s == 0) {
                let v4 = std::net::Ipv4Addr::new(
                    (seg[6] >> 8) as u8,
                    seg[6] as u8,
                    (seg[7] >> 8) as u8,
                    seg[7] as u8,
                );
                return is_forbidden_ipv4(v4);
            }
            ipv6.is_loopback()
                || ipv6.is_unspecified()
                || (seg[0] & 0xfe00) == 0xfc00 // unique local fc00::/7
                || (seg[0] & 0xffc0) == 0xfe80 // link-local fe80::/10
                || (seg[0] & 0xff00) == 0xff00 // multicast ff00::/8
                || seg[0] == 0x2002 // 6to4 (embeds an arbitrary IPv4)
                || (seg[0] == 0x2001 && seg[1] == 0x0db8) // documentation
        }
    }
}

/// GETs `url` under `max_bytes` / `timeout` caps and returns the image bytes.
/// Fails closed on anything that isn't a 2xx `image/*` body within the caps.
/// With `allow_private_ips == false` (every non-development deployment) the
/// target address is checked and pinned, and redirects are refused — see the
/// SSRF note at the top of this file.
pub async fn fetch_image(
    url: &str,
    max_bytes: usize,
    timeout: Duration,
    allow_private_ips: bool,
) -> Result<FetchedImage, HttpClientError> {
    let parsed = reqwest::Url::parse(url).map_err(|_| HttpClientError::UnsupportedScheme)?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(HttpClientError::UnsupportedScheme);
    }

    let mut builder = reqwest::Client::builder().timeout(timeout);

    if allow_private_ips {
        builder = builder.redirect(reqwest::redirect::Policy::limited(5));
    } else {
        let host = parsed
            .host_str()
            .ok_or(HttpClientError::UnsupportedScheme)?;
        let port = parsed
            .port_or_known_default()
            .ok_or(HttpClientError::UnsupportedScheme)?;
        // `host_str` keeps the brackets around an IPv6 literal; the resolver
        // wants them gone.
        let lookup_host = host.trim_start_matches('[').trim_end_matches(']');
        let addrs: Vec<std::net::SocketAddr> = tokio::net::lookup_host((lookup_host, port))
            .await
            .map_err(|err| HttpClientError::Request(err.to_string()))?
            .collect();
        if addrs.is_empty() {
            return Err(HttpClientError::Request(format!("{host} did not resolve")));
        }
        // Every answer must be public — otherwise an attacker-controlled
        // name could list one public and one internal address.
        if addrs
            .iter()
            .any(|addr| is_private_or_restricted_ip(addr.ip()))
        {
            return Err(HttpClientError::ForbiddenAddress);
        }
        // Pin the connection to the address that was just checked, so reqwest
        // doesn't re-resolve (DNS rebinding), and never follow a redirect to
        // a target that skipped this check.
        builder = builder
            .resolve(lookup_host, addrs[0])
            .redirect(reqwest::redirect::Policy::none());
    }

    let client = builder
        .build()
        .map_err(|err| HttpClientError::Request(err.to_string()))?;

    let mut response = client
        .get(parsed)
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

    // ...and enforce it while streaming (Content-Length can lie or be
    // absent), so an endless chunked body is cut off at the cap instead of
    // being buffered whole.
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|err| HttpClientError::Request(err.to_string()))?
    {
        if bytes.len() + chunk.len() > max_bytes {
            return Err(HttpClientError::TooLarge { limit: max_bytes });
        }
        bytes.extend_from_slice(&chunk);
    }

    Ok(FetchedImage { bytes, extension })
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
        let err = fetch_image("file:///etc/passwd", 1024, Duration::from_secs(1), false)
            .await
            .unwrap_err();
        assert!(matches!(err, HttpClientError::UnsupportedScheme));
    }

    #[tokio::test]
    async fn rejects_forbidden_loopback_and_metadata_ips() {
        for url in [
            "http://127.0.0.1:8080/logo.png",
            "http://169.254.169.254/latest/meta-data/",
            "http://0.0.0.0/logo.png",
            "http://[::1]/logo.png",
            "http://[::ffff:169.254.169.254]/logo.png",
            "http://[64:ff9b::a9fe:a9fe]/logo.png",
            "http://localhost/logo.png",
        ] {
            let err = fetch_image(url, 1024, Duration::from_secs(1), false)
                .await
                .unwrap_err();
            assert!(
                matches!(err, HttpClientError::ForbiddenAddress),
                "{url} should be forbidden, got {err:?}"
            );
        }
    }

    #[test]
    fn classifies_addresses() {
        let forbidden = [
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "100.64.0.1",
            "0.1.2.3",
            "224.0.0.1",
            "255.255.255.255",
            "fd00::1",
            "fe80::1",
            "ff02::1",
            "::ffff:10.0.0.1",
            "2002:a9fe:a9fe::1",
        ];
        for ip in forbidden {
            assert!(
                is_private_or_restricted_ip(ip.parse().unwrap()),
                "{ip} should be forbidden"
            );
        }
        for ip in [
            "8.8.8.8",
            "1.1.1.1",
            "2606:4700:4700::1111",
            "::ffff:8.8.8.8",
        ] {
            assert!(
                !is_private_or_restricted_ip(ip.parse().unwrap()),
                "{ip} should be allowed"
            );
        }
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
