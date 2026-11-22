//! Public-network fetches with a bounded response body.
use anyhow::{ensure, Result};
use reqwest::{dns, redirect, Client, Response, Url};
use std::net::IpAddr;
use std::sync::Arc;

pub(super) const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, c, _] = ip.octets();
            !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_multicast()
                || a == 0
                || a >= 240
                || (a == 100 && (64..=127).contains(&b))
                || (a == 192 && b == 0 && (c == 0 || c == 2))
                || (a == 198 && (b == 18 || b == 19 || (b == 51 && c == 100)))
                || (a == 203 && b == 0 && c == 113))
        }
        IpAddr::V6(ip) => {
            if let Some(ip) = ip.to_ipv4_mapped() {
                return public_ip(IpAddr::V4(ip));
            }
            let [a, b, ..] = ip.segments();
            a & 0xe000 == 0x2000
                && !(a == 0x2001 && (b < 0x200 || b == 0xdb8))
                && a != 0x2002
                && !(a == 0x3fff && b < 0x1000)
        }
    }
}

pub(crate) fn checked_url(raw: &str) -> Result<Url> {
    let url = Url::parse(raw)?;
    ensure!(
        matches!(url.scheme(), "http" | "https"),
        "publisher URL must be HTTP(S)"
    );
    ensure!(
        url.username().is_empty() && url.password().is_none(),
        "publisher URL contains credentials"
    );
    let host = url
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("publisher URL has no host"))?;
    let host = host.trim_start_matches('[').trim_end_matches(']');
    ensure!(
        !host.trim_end_matches('.').eq_ignore_ascii_case("localhost"),
        "publisher URL is local"
    );
    if let Ok(ip) = host.parse::<IpAddr>() {
        ensure!(public_ip(ip), "publisher URL is not public");
    }
    Ok(url)
}

struct PublicDns;
impl dns::Resolve for PublicDns {
    fn resolve(&self, name: dns::Name) -> dns::Resolving {
        Box::pin(async move {
            let addresses: Vec<_> = tokio::net::lookup_host((name.as_str(), 0)).await?.collect();
            if addresses.is_empty() || addresses.iter().any(|address| !public_ip(address.ip())) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "publisher DNS must resolve only to public addresses",
                )
                .into());
            }
            Ok(Box::new(addresses.into_iter()) as dns::Addrs)
        })
    }
}

pub(super) fn builder() -> reqwest::ClientBuilder {
    Client::builder()
        .timeout(super::ARTICLE_FETCH_TIMEOUT)
        .user_agent(super::ARTICLE_FETCH_USER_AGENT)
        // A proxy would resolve destinations outside this connection's DNS guard.
        .no_proxy()
        .dns_resolver(Arc::new(PublicDns))
        .redirect(redirect_policy())
}

fn redirect_policy() -> redirect::Policy {
    redirect::Policy::custom(|attempt| {
        if attempt.previous().len() >= 10 {
            return attempt.error("too many publisher redirects");
        }
        match checked_url(attempt.url().as_str()) {
            Ok(_) => attempt.follow(),
            Err(error) => attempt.error(error),
        }
    })
}

#[derive(Debug)]
pub(super) struct ResponseTooLarge;
impl std::fmt::Display for ResponseTooLarge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("publisher response exceeds byte budget")
    }
}
impl std::error::Error for ResponseTooLarge {}

pub(crate) async fn read_body(mut response: Response, limit: usize) -> Result<String> {
    if response
        .content_length()
        .is_some_and(|size| size > limit as u64)
    {
        return Err(ResponseTooLarge.into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if chunk.len() > limit - bytes.len() {
            return Err(ResponseTooLarge.into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn rejects_local_and_special_addresses_including_obfuscated_literals() {
        for url in [
            "file:///etc/passwd",
            "http://localhost/",
            "http://localhost./",
            "http://127.1/",
            "http://2130706433/",
            "http://10.0.0.1/",
            "http://172.16.0.1/",
            "http://192.168.1.1/",
            "http://169.254.169.254/",
            "http://100.64.0.1/",
            "http://0.0.0.0/",
            "http://224.0.0.1/",
            "http://[::1]/",
            "http://[fc00::1]/",
            "http://[fe80::1]/",
            "http://[::ffff:127.0.0.1]/",
            "http://[2002:7f00:1::]/",
            "http://user:secret@example.com/",
        ] {
            assert!(checked_url(url).is_err(), "{url}");
        }
        for url in [
            "https://example.com/story",
            "http://8.8.8.8/",
            "https://[2606:4700:4700::1111]/",
        ] {
            assert!(checked_url(url).is_ok(), "{url}");
        }
        for ip in [
            Ipv4Addr::new(192, 0, 2, 1),
            Ipv4Addr::new(198, 51, 100, 1),
            Ipv4Addr::new(203, 0, 113, 1),
        ] {
            assert!(!public_ip(ip.into()));
        }
    }

    async fn response(wire: &'static str) -> Response {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0; 1024];
            stream.read(&mut request).await.unwrap();
            stream.write_all(wire.as_bytes()).await.unwrap();
        });
        Client::builder()
            .no_proxy()
            .build()
            .unwrap()
            .get(format!("http://{address}/"))
            .send()
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn bounds_declared_chunked_and_unknown_length_bodies() {
        for wire in [
            "HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\n123456789",
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\n1234\r\n5\r\n56789\r\n0\r\n\r\n",
            "HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n123456789",
        ] {
            assert!(read_body(response(wire).await, 8).await.unwrap_err().is::<ResponseTooLarge>());
        }
        assert_eq!(
            read_body(
                response("HTTP/1.1 200 OK\r\nContent-Length: 8\r\n\r\n12345678").await,
                8
            )
            .await
            .unwrap(),
            "12345678"
        );
    }

    #[tokio::test]
    async fn rejects_a_redirect_to_a_private_destination() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0; 1024];
            stream.read(&mut request).await.unwrap();
            stream.write_all(b"HTTP/1.1 302 Found\r\nLocation: http://169.254.169.254/secret\r\nContent-Length: 0\r\n\r\n").await.unwrap();
        });
        let error = Client::builder()
            .no_proxy()
            .redirect(redirect_policy())
            .build()
            .unwrap()
            .get(format!("http://{address}/"))
            .send()
            .await
            .unwrap_err();
        assert!(error.is_redirect());
    }

    #[tokio::test]
    async fn dns_rejects_localhost_before_connecting() {
        use dns::Resolve;
        assert!(PublicDns
            .resolve("localhost".parse().unwrap())
            .await
            .is_err());
    }
}
