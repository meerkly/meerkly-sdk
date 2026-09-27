//! SSRF protection for the exit node's outbound dials.
//!
//! The exit connects to whatever host the customer named, from the earner's own
//! machine — so without a guard a proxied request could reach the earner's LAN, a
//! router admin page, or the cloud metadata service (`169.254.169.254`). We
//! resolve the target ourselves, reject any address that is not a routable public
//! one, and connect to the *validated* address — never re-resolving the name, so
//! DNS rebinding (resolve public for the check, private for the connect) can't
//! slip through.
//!
//! A dev/test escape hatch, `MEERKLY_ALLOW_PRIVATE_TARGETS=1`, disables the
//! filter so the local-target smoke tests can run. It only ever exposes the
//! machine that sets it, so it is safe as an opt-in.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::OnceLock;

use tokio::net::TcpStream;

/// Resolve `host:port`, reject non-public addresses, and connect to a validated
/// one. The connect targets a concrete address, not the hostname, so the checked
/// address is the one actually dialled.
pub async fn connect(host: &str, port: u16) -> std::io::Result<TcpStream> {
    use std::io::{Error, ErrorKind};

    let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host, port)).await?.collect();
    if addrs.is_empty() {
        return Err(Error::new(ErrorKind::NotFound, "host did not resolve"));
    }

    let candidates: Vec<SocketAddr> = if allow_private() {
        addrs
    } else {
        let allowed: Vec<SocketAddr> = addrs.into_iter().filter(|a| !is_blocked(a.ip())).collect();
        if allowed.is_empty() {
            return Err(Error::new(
                ErrorKind::PermissionDenied,
                "destination is not an allowed public address",
            ));
        }
        allowed
    };

    let mut last_err = None;
    for addr in candidates {
        match TcpStream::connect(addr).await {
            Ok(stream) => return Ok(stream),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| Error::new(ErrorKind::AddrNotAvailable, "no address connected")))
}

/// Whether an IP must not be dialled: anything that is not a routable public
/// address (loopback, private, link-local incl. cloud metadata, CGNAT, ULA,
/// multicast, unspecified, broadcast, documentation).
pub fn is_blocked(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_blocked_v4(v4),
        // Treat IPv4-mapped addresses (::ffff:a.b.c.d) as their IPv4 form so they
        // cannot be used to smuggle a private v4 address past the v6 checks.
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => is_blocked_v4(v4),
            None => is_blocked_v6(v6),
        },
    }
}

fn is_blocked_v4(v4: Ipv4Addr) -> bool {
    v4.is_loopback()
        || v4.is_private()
        || v4.is_link_local() // 169.254.0.0/16, incl. 169.254.169.254 metadata
        || v4.is_broadcast()
        || v4.is_documentation()
        || v4.is_multicast()
        || v4.is_unspecified()
        || is_shared_v4(v4) // 100.64.0.0/10 carrier-grade NAT
}

fn is_blocked_v6(v6: Ipv6Addr) -> bool {
    v6.is_loopback()
        || v6.is_multicast()
        || v6.is_unspecified()
        || is_unique_local_v6(v6) // fc00::/7
        || is_unicast_link_local_v6(v6) // fe80::/10
}

// 100.64.0.0/10
fn is_shared_v4(v4: Ipv4Addr) -> bool {
    let o = v4.octets();
    o[0] == 100 && (o[1] & 0b1100_0000) == 0b0100_0000
}

// fc00::/7
fn is_unique_local_v6(v6: Ipv6Addr) -> bool {
    (v6.segments()[0] & 0xfe00) == 0xfc00
}

// fe80::/10
fn is_unicast_link_local_v6(v6: Ipv6Addr) -> bool {
    (v6.segments()[0] & 0xffc0) == 0xfe80
}

/// Read once: whether private/local targets are permitted (dev/test only).
fn allow_private() -> bool {
    static ALLOW: OnceLock<bool> = OnceLock::new();
    *ALLOW.get_or_init(|| {
        matches!(
            std::env::var("MEERKLY_ALLOW_PRIVATE_TARGETS")
                .ok()
                .as_deref()
                .map(str::trim),
            Some("1" | "true" | "yes")
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn ip(s: &str) -> IpAddr {
        IpAddr::from_str(s).unwrap()
    }

    #[test]
    fn blocks_private_and_local_ranges() {
        for s in [
            "127.0.0.1",
            "10.1.2.3",
            "192.168.0.5",
            "172.16.9.9",
            "169.254.169.254", // cloud metadata
            "100.64.1.1",      // CGNAT
            "0.0.0.0",
            "255.255.255.255",
            "224.0.0.1", // multicast
            "::1",
            "fe80::1",
            "fc00::1",
            "fd00::1",
            "::ffff:127.0.0.1", // IPv4-mapped loopback
            "::ffff:10.0.0.1",  // IPv4-mapped private
        ] {
            assert!(is_blocked(ip(s)), "{s} should be blocked");
        }
    }

    #[test]
    fn allows_public_addresses() {
        for s in [
            "8.8.8.8",
            "1.1.1.1",
            "93.184.216.34",
            "2606:4700:4700::1111",
        ] {
            assert!(!is_blocked(ip(s)), "{s} should be allowed");
        }
    }
}
