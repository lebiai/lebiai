//! SSRF guard for remote `skill_install` URLs.
//! Same rules as `hermes-tools` web_fetch: no loopback / private / metadata.

use std::net::{IpAddr, Ipv4Addr};

pub fn validate_public_http_url(url: &str) -> Result<(), String> {
    let url = url.trim();
    if url.is_empty() {
        return Err("empty URL".into());
    }
    let parsed = reqwest::Url::parse(url).map_err(|e| format!("invalid URL: {e}"))?;
    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(format!("scheme `{scheme}` not allowed (only http/https)"));
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| "URL has no host".to_string())?
        .to_ascii_lowercase();

    if host == "localhost" || host.ends_with(".localhost") || host == "0.0.0.0" {
        return Err("localhost / loopback host blocked".into());
    }
    if host == "metadata.google.internal"
        || host == "metadata"
        || host.ends_with(".internal")
        || host == "kubernetes.default.svc"
    {
        return Err(format!("blocked metadata/internal host: {host}"));
    }

    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_non_public_ip(ip) {
            return Err(format!("blocked non-public IP: {ip}"));
        }
        return Ok(());
    }

    match std::net::ToSocketAddrs::to_socket_addrs(&(
        host.as_str(),
        parsed.port_or_known_default().unwrap_or(80),
    )) {
        Ok(addrs) => {
            let mut any = false;
            for addr in addrs {
                any = true;
                if is_non_public_ip(addr.ip()) {
                    return Err(format!(
                        "host {host} resolves to non-public IP {} (SSRF blocked)",
                        addr.ip()
                    ));
                }
            }
            if !any {
                return Err(format!("host {host} did not resolve"));
            }
            Ok(())
        }
        Err(e) => Err(format!("DNS resolution failed for {host}: {e}")),
    }
}

fn is_non_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_unspecified()
                || v4.is_multicast()
                || (o[0] == 100 && (64..=127).contains(&o[1]))
                || v4 == Ipv4Addr::new(169, 254, 169, 254)
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (v6.octets()[0] & 0xfe) == 0xfc
                || (v6.segments()[0] & 0xffc0) == 0xfe80
                || v6
                    .to_ipv4_mapped()
                    .is_some_and(|v4| is_non_public_ip(IpAddr::V4(v4)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_localhost_and_metadata() {
        assert!(validate_public_http_url("http://localhost/x").is_err());
        assert!(validate_public_http_url("http://127.0.0.1/").is_err());
        assert!(validate_public_http_url("http://169.254.169.254/latest").is_err());
        assert!(validate_public_http_url("http://192.168.1.1/a").is_err());
    }
}
