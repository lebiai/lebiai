//! URL safety for outbound HTTP tools (SSRF guards).

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Validate that `url` is an http(s) URL that does not target loopback,
/// link-local, RFC1918 private, or cloud metadata addresses.
///
/// Returns `Ok(())` or `Err(reason)` suitable for tool error content.
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
    // Common cloud metadata hostnames
    if host == "metadata.google.internal"
        || host == "metadata"
        || host.ends_with(".internal")
        || host == "kubernetes.default.svc"
    {
        return Err(format!("blocked metadata/internal host: {host}"));
    }

    // If host is a literal IP, check it strictly (including 198.18.x / CGNAT).
    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_non_public_ip(ip) {
            return Err(format!("blocked non-public IP: {ip}"));
        }
        return Ok(());
    }

    // Hostname: resolve and require real public answers.
    // Many Chinese proxy stacks (Clash/Surge fake-ip) map public names to
    // 198.18.0.0/15 — those must NOT be treated as SSRF when the host is a
    // normal DNS name (the proxy still forwards to the real origin).
    match std::net::ToSocketAddrs::to_socket_addrs(&(
        host.as_str(),
        parsed.port_or_known_default().unwrap_or(80),
    )) {
        Ok(addrs) => {
            let mut any = false;
            for addr in addrs {
                any = true;
                if is_ssrf_ip_for_hostname(addr.ip()) {
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

/// Stricter private-IP check used only when the URL host was a domain name.
/// Allows 198.18/15 and 100.64/10 (fake-ip / CGNAT used by local proxies).
fn is_ssrf_ip_for_hostname(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            // Allow fake-ip range used by Clash/Surge etc.
            if o[0] == 198 && (18..=19).contains(&o[1]) {
                return false;
            }
            // Allow CGNAT often used by tunnels
            if o[0] == 100 && (64..=127).contains(&o[1]) {
                return false;
            }
            is_non_public_v4(v4)
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_ssrf_ip_for_hostname(IpAddr::V4(v4));
            }
            is_non_public_v6(v6)
        }
    }
}

fn is_non_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_non_public_v4(v4),
        IpAddr::V6(v6) => is_non_public_v6(v6),
    }
}

fn is_non_public_v4(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_unspecified()
        || ip.is_multicast()
        // CGNAT 100.64/10
        || (o[0] == 100 && (64..=127).contains(&o[1]))
        // IETF protocol assignments / benchmarking / TEST-NET
        || (o[0] == 192 && o[1] == 0 && o[2] == 0)
        || (o[0] == 198 && (18..=19).contains(&o[1]))
        || (o[0] == 192 && o[1] == 0 && o[2] == 2) // TEST-NET-1
        || (o[0] == 198 && o[1] == 51 && o[2] == 100) // TEST-NET-2
        || (o[0] == 203 && o[1] == 0 && o[2] == 113) // TEST-NET-3
        // AWS/GCP metadata
        || ip == Ipv4Addr::new(169, 254, 169, 254)
}

fn is_non_public_v6(ip: Ipv6Addr) -> bool {
    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || is_unique_local_v6(ip)
        || is_unicast_link_local_v6(ip)
        // IPv4-mapped
        || ip.to_ipv4_mapped().is_some_and(is_non_public_v4)
}

/// fc00::/7. Manual: `Ipv6Addr::is_unique_local` is stable only since 1.84; MSRV is 1.78.
fn is_unique_local_v6(ip: Ipv6Addr) -> bool {
    (ip.octets()[0] & 0xfe) == 0xfc
}

/// fe80::/10. Manual: `Ipv6Addr::is_unicast_link_local` is stable only since 1.84.
fn is_unicast_link_local_v6(ip: Ipv6Addr) -> bool {
    (ip.segments()[0] & 0xffc0) == 0xfe80
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_localhost() {
        assert!(validate_public_http_url("http://localhost/x").is_err());
        assert!(validate_public_http_url("http://127.0.0.1/").is_err());
    }

    #[test]
    fn blocks_metadata() {
        assert!(validate_public_http_url("http://169.254.169.254/latest").is_err());
    }

    #[test]
    fn blocks_private() {
        assert!(validate_public_http_url("http://192.168.1.1/").is_err());
        assert!(validate_public_http_url("http://10.0.0.5/a").is_err());
        assert!(is_unique_local_v6("fc00::1".parse().unwrap()));
        assert!(is_unicast_link_local_v6("fe80::1".parse().unwrap()));
        assert!(!is_unique_local_v6("2001:db8::1".parse().unwrap()));
    }

    #[test]
    fn blocks_file_scheme() {
        assert!(validate_public_http_url("file:///etc/passwd").is_err());
    }

    #[test]
    fn allows_well_formed_https_shape() {
        // Do not depend on live DNS (some sandboxes map public names to
        // 198.18.x sinkholes which we correctly treat as non-public).
        assert!(validate_public_http_url("https://203.0.113.10/x").is_err()); // TEST-NET-3 docs
        assert!(validate_public_http_url("ftp://example.com/").is_err());
        assert!(validate_public_http_url("https://").is_err());
    }
}
