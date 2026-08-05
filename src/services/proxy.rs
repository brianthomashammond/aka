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

    /// Build the docker run arguments for the nginx proxy container
    pub fn build_run_command(&self, cfg: &config::DoryConfig) -> String {
        let image = self.image_name(cfg);
        let port = cfg.nginx_proxy.port;
        let container_name = &cfg.nginx_proxy.container_name;

        let mut parts = Vec::new();

        // Basic run flags
        parts.push(format!("docker run -d -p {port}:80"));

        // TLS port if enabled
        if cfg.nginx_proxy.https_enabled {
            parts.push(format!("-p {}:443", cfg.nginx_proxy.tls_port));
        }

        // SSL certs volume mount
        if !cfg.nginx_proxy.ssl_certs_dir.is_empty() {
            parts.push(format!("-v {}:/etc/nginx/certs", shell_escape(&cfg.nginx_proxy.ssl_certs_dir)));
        }

        // Docker socket mount
        parts.push("-v /var/run/docker.sock:/tmp/docker.sock".to_string());

        // Container name env var
        parts.push(format!("-e CONTAINER_NAME={}", shell_escape(container_name)));

        // Container name flag
        parts.push(format!("--name {}", shell_escape(container_name)));

        // Image
        parts.push(shell_escape(&image));

        parts.join(" ")
    }

    /// Start the nginx proxy service. Returns true if the container is running after the call.
    pub fn start(&self, cfg: &config::DoryConfig) -> Result<bool> {
        if !self.docker.is_installed() {
            log::error!("Docker is not installed. Cannot start nginx proxy.");
            return Err(AkaError::DockerNotFound);
        }

        let container_name = &cfg.nginx_proxy.container_name;

        // Check if already running
        if self.docker.is_running(container_name) {
            log::debug!(
                "nginx proxy container '{}' is already running. Doing nothing.",
                container_name
            );
            return Ok(true);
        }

        // If container exists but is stopped, remove it first
        if self.docker.container_exists(container_name) {
            log::debug!("Removing stale nginx proxy container '{}'.", container_name);
            self.docker.remove_container(container_name)?;
        }

        // Build docker run command
        let image = self.image_name(cfg);
        let port = cfg.nginx_proxy.port;
        let mut args: Vec<String> = vec![
            "run".into(),
            "-d".into(),
            "-p".into(),
            format!("{port}:80"),
            "--name".into(),
            container_name.clone(),
            "-v".into(),
            "/var/run/docker.sock:/tmp/docker.sock".into(),
            "-e".into(),
            format!("CONTAINER_NAME={}", container_name),
        ];

        // TLS port if enabled
        if cfg.nginx_proxy.https_enabled {
            args.push("-p".into());
            args.push(format!("{}:443", cfg.nginx_proxy.tls_port));
        }

        // SSL certs volume mount
        if !cfg.nginx_proxy.ssl_certs_dir.is_empty() {
            args.push("-v".into());
            args.push(format!("{}:/etc/nginx/certs", cfg.nginx_proxy.ssl_certs_dir));
        }

        // Image (no shell escaping — args are passed directly to Command, not through a shell)
        args.push(image);

        let run_cmd = self.build_run_command(cfg);
        log::debug!("Starting nginx proxy with: {}", run_cmd);

        let output = std::process::Command::new("docker")
            .args(&args.iter().map(|s| s.as_str()).collect::<Vec<_>>())
            .output();

        match output {
            Ok(o) if o.status.success() => {
                Ok(self.docker.is_running(container_name))
            }
            Ok(o) => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                log::error!("Failed to start nginx proxy: {}", stderr.trim());
                Err(AkaError::DockerCommand(format!(
                    "nginx proxy container failed to start: {}",
                    stderr.trim()
                )))
            }
            Err(e) => {
                Err(AkaError::DockerCommand(format!("failed to spawn docker: {e}")))
            }
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

/// Shell-escape a string for safe use in docker run commands
fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace("'", "'\\''"))
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
    fn shell_escape_basic() {
        assert_eq!(shell_escape("docker"), "'docker'");
        assert_eq!(shell_escape("it's"), "'it'\\''s'");
        assert_eq!(shell_escape("127.0.0.1"), "'127.0.0.1'");
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

    #[test]
    fn build_run_command_contains_expected_parts() {
        let svc = ProxyService::new();
        let cfg = default_cfg();
        let cmd = svc.build_run_command(&cfg);

        assert!(cmd.contains("docker run"));
        assert!(cmd.contains("-d"));
        assert!(cmd.contains("-p 80:80"));
        assert!(cmd.contains("-v /var/run/docker.sock:/tmp/docker.sock"));
        assert!(cmd.contains("CONTAINER_NAME="));
        assert!(cmd.contains(DEFAULT_PROXY_IMAGE));
    }

    #[test]
    fn build_run_command_with_tls() {
        let svc = ProxyService::new();
        let mut cfg = default_cfg();
        cfg.nginx_proxy.https_enabled = true;
        cfg.nginx_proxy.tls_port = 8443;
        let cmd = svc.build_run_command(&cfg);

        assert!(cmd.contains("-p 8443:443"));
    }

    #[test]
    fn build_run_command_with_ssl_certs() {
        let svc = ProxyService::new();
        let mut cfg = default_cfg();
        cfg.nginx_proxy.ssl_certs_dir = "/my/certs".to_string();
        let cmd = svc.build_run_command(&cfg);

        assert!(cmd.contains("/my/certs"));
        assert!(cmd.contains("/etc/nginx/certs"));
        assert!(cmd.contains(CUSTOM_PROXY_IMAGE));
    }
}
