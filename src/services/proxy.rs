use crate::config;
use crate::docker;
use crate::error::{AkaError, Result};

/// Default Nginx proxy image when no custom certs_dir
const DEFAULT_PROXY_IMAGE: &str = "freedomben/dory-http-proxy:2.6.2.2";

/// Custom proxy image when ssl_certs_dir is configured
const CUSTOM_PROXY_IMAGE: &str = "codekitchen/dinghy-http-proxy:2.5.10";

/// Nginx proxy service management
pub struct ProxyService {
    docker: docker::DockerClient,
}

impl ProxyService {
    pub fn new() -> Self {
        Self {
            docker: docker::DockerClient::new(),
        }
    }

    /// Get the Docker image to use based on configuration
    pub fn image_name(&self, cfg: &config::DoryConfig) -> String {
        if !cfg.nginx_proxy.ssl_certs_dir.is_empty() {
            CUSTOM_PROXY_IMAGE.to_string()
        } else {
            DEFAULT_PROXY_IMAGE.to_string()
        }
    }

    /// Stop the nginx proxy service
    pub fn stop(&self, cfg: &config::DoryConfig) -> Result<()> {
        self.docker.stop_container(&cfg.nginx_proxy.container_name)?;
        Ok(())
    }

    /// Full lifecycle: ensure the proxy is running
    pub fn ensure_running(&self, cfg: &config::DoryConfig) -> Result<bool> {
        if !self.docker.is_installed() {
            return Err(AkaError::DockerNotFound);
        }

        self.docker.ensure_container(
            &self.image_name(cfg),
            &cfg.nginx_proxy.container_name,
            &[(cfg.nginx_proxy.port, 80)],
            &[("CONTAINER_NAME", &cfg.nginx_proxy.container_name)],
            &["/var/run/docker.sock:/tmp/docker.sock"],
            &[],
        )?;

        Ok(self.docker.is_running(&cfg.nginx_proxy.container_name))
    }
}

impl Default for ProxyService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{DnsmasqConfig, DoryConfig, NginxProxyConfig, ResolvConfig};

    fn default_cfg() -> DoryConfig {
        DoryConfig {
            dnsmasq: DnsmasqConfig::default(),
            nginx_proxy: NginxProxyConfig::default(),
            resolv: ResolvConfig::default(),
            debug: false,
        }
    }

    #[test]
    fn new_service() {
        let svc = ProxyService::new();
        let cfg = default_cfg();
        assert_eq!(svc.image_name(&cfg), DEFAULT_PROXY_IMAGE);
    }

    #[test]
    fn image_with_custom_certs_dir() {
        let svc = ProxyService::new();
        let mut cfg = default_cfg();
        cfg.nginx_proxy.ssl_certs_dir = "/custom/certs".to_string();
        assert_eq!(svc.image_name(&cfg), CUSTOM_PROXY_IMAGE);
    }

    #[test]
    fn default_service() {
        let svc = ProxyService::default();
        assert!(svc.docker.is_installed() == docker::is_docker_installed());
    }
}
