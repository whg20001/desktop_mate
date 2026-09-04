use std::{
    net::{SocketAddr, ToSocketAddrs},
    time::Duration,
};

use reqwest::{redirect::Policy, Client, Response};
use url::{Host, Url};

#[derive(Clone, Debug)]
pub struct LocalEndpoint {
    url: Url,
    resolved_addresses: Vec<SocketAddr>,
}

impl LocalEndpoint {
    pub fn parse(value: &str) -> Result<Self, String> {
        let mut url = Url::parse(value).map_err(|error| format!("invalid URL: {error}"))?;
        if url.scheme() != "http" {
            return Err("local endpoints must use plain HTTP over loopback".into());
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err("credentials are not allowed in a local endpoint URL".into());
        }
        if url.query().is_some() || url.fragment().is_some() {
            return Err("query strings and fragments are not allowed in a local endpoint".into());
        }

        let resolved_addresses = resolve_loopback_addresses(&url)?;
        if !url.path().ends_with('/') {
            url.set_path(&format!("{}/", url.path()));
        }
        Ok(Self {
            url,
            resolved_addresses,
        })
    }

    pub fn join(&self, relative: &str) -> Result<Url, String> {
        let url = self
            .url
            .join(relative.trim_start_matches('/'))
            .map_err(|error| error.to_string())?;
        resolve_loopback_addresses(&url)?;
        Ok(url)
    }

    pub fn http_client_with_timeout(&self, timeout: Duration) -> Result<Client, String> {
        let mut builder = Client::builder()
            .no_proxy()
            .redirect(Policy::none())
            .connect_timeout(Duration::from_secs(2))
            .timeout(timeout);
        if matches!(self.url.host(), Some(Host::Domain(_))) {
            builder = builder.resolve_to_addrs("localhost", &self.resolved_addresses);
        }
        builder.build().map_err(|error| error.to_string())
    }

    pub fn validate_response(&self, response: &Response) -> Result<(), String> {
        resolve_loopback_addresses(response.url())?;
        if response.status().is_redirection() {
            return Err("redirects are disabled for local services".into());
        }
        Ok(())
    }
}

fn resolve_loopback_addresses(url: &Url) -> Result<Vec<SocketAddr>, String> {
    let port = url
        .port_or_known_default()
        .ok_or_else(|| "local endpoint has no port".to_string())?;
    match url.host() {
        Some(Host::Ipv4(address)) if address.is_loopback() => {
            Ok(vec![SocketAddr::new(address.into(), port)])
        }
        Some(Host::Ipv6(address)) if address.is_loopback() => {
            Ok(vec![SocketAddr::new(address.into(), port)])
        }
        Some(Host::Domain(host)) if host.eq_ignore_ascii_case("localhost") => {
            resolve_localhost(port)
        }
        Some(_) => Err("services may only use 127.0.0.0/8, ::1, or localhost".into()),
        None => Err("local endpoint has no host".into()),
    }
}

fn resolve_localhost(port: u16) -> Result<Vec<SocketAddr>, String> {
    let addresses: Vec<SocketAddr> = ("localhost", port)
        .to_socket_addrs()
        .map_err(|error| format!("cannot resolve localhost: {error}"))?
        .collect();
    if addresses.is_empty() || addresses.iter().any(|address| !address.ip().is_loopback()) {
        return Err("localhost did not resolve exclusively to loopback addresses".into());
    }
    Ok(addresses)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_ipv4_ipv6_and_localhost_loopback() {
        assert!(LocalEndpoint::parse("http://127.0.0.1:8765").is_ok());
        assert!(LocalEndpoint::parse("http://127.8.9.10:8765/api").is_ok());
        assert!(LocalEndpoint::parse("http://[::1]:8765").is_ok());
        assert!(LocalEndpoint::parse("http://localhost:8765").is_ok());
    }

    #[test]
    fn rejects_non_loopback_and_embedded_credentials() {
        assert!(LocalEndpoint::parse("https://127.0.0.1:8765").is_err());
        assert!(LocalEndpoint::parse("http://192.168.1.2:8765").is_err());
        assert!(LocalEndpoint::parse("http://8.8.8.8:8765").is_err());
        assert!(LocalEndpoint::parse("http://token@127.0.0.1:8765").is_err());
        assert!(LocalEndpoint::parse("http://example.com:8765").is_err());
    }

    #[test]
    fn creates_a_hardened_client() {
        let endpoint = LocalEndpoint::parse("http://127.0.0.1:8765").unwrap();
        assert!(endpoint
            .http_client_with_timeout(Duration::from_secs(8))
            .is_ok());
    }
}
