use crate::config;
use crate::docker;
use crate::error::{AkaError, Result};
use crate::port_utils;
use crate::services::systemd;

/// The default Dnsmasq Docker image
const DEFAULT_DNSMASQ_IMAGE: &str = "freedomben/dory-dnsmasq:1.1.0";

/// Check if a string looks like a dinghy VM IP
fn is_dinghy_match(addr: &str) -> bool {
    // Dinghy VM IPs are typically in the range 192.168.99.x
    // This is a heuristic - the original Ruby code delegates to Dory::Dinghy.match?
    addr.starts_with("192.168.99.")
}

/// Resolve an address, substituting dinghy VM IP if needed
fn resolve_address(addr: &str) -> String {
    if is_dinghy_match(addr) {
        // In the original code, this would call Dory::Dinghy.ip
        // For now, return the address as-is since we don't have dinghy detection
        addr.to_string()
    } else {
        addr.to_string()
    }
}

/// Get the port for dnsmasq.
/// On Linux port 53 is required. On macOS, if the port is still the Linux default (53),
/// fall back to 19323 so we don't conflict with the host resolver.
fn get_dnsmasq_port(cfg: &config::DoryConfig) -> u16 {
    #[cfg(target_os = "macos")]
    if cfg.dnsmasq.port == 53 {
        return 19323;
    }
    cfg.dnsmasq.port
}

/// Build the domain address argument string for the dnsmasq container
fn build_domain_args(cfg: &config::DoryConfig) -> String {
    cfg.dnsmasq
        .domains
        .iter()
        .map(|d| {
            let resolved = resolve_address(&d.address);
            format!("{} {}", shell_escape(&d.domain), shell_escape(&resolved))
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Shell-escape a string for safe use in docker run commands
fn shell_escape(s: &str) -> String {
    // Use single quotes and escape any single quotes within
    format!("'{}'", s.replace("'", "'\\''"))
}

/// Parse shell-escaped arguments back into a Vec<String>
fn parse_shell_args(s: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '\'' {
                if chars.peek() == Some(&'\\') {
                    chars.next(); // skip backslash
                    if chars.peek() == Some(&'\'') {
                        chars.next(); // skip escaped quote
                        current.push('\'');
                    }
                } else {
                    in_quotes = false;
                }
            } else {
                current.push(c);
            }
        } else if c == '\'' {
            in_quotes = true;
        } else if c.is_whitespace() {
            if !current.is_empty() {
                args.push(current.clone());
                current.clear();
            }
        } else {
            current.push(c);
        }
    }

    if !current.is_empty() {
        args.push(current);
    }

    args
}

/// Dnsmasq service management
pub struct DnsmasqService {
    docker: docker::DockerClient,
}

impl DnsmasqService {
    pub fn new() -> Self {
        Self {
            docker: docker::DockerClient::new(),
        }
    }

    /// Get the Docker image to use
    pub fn image_name(&self) -> String {
        // In the original, this checks config for a custom image
        DEFAULT_DNSMASQ_IMAGE.to_string()
    }

    /// Check if something is bound to the given port
    pub fn check_port_conflict(&self, port: u16) -> Result<Vec<port_utils::PortProcess>> {
        port_utils::check_port(port)
    }

    /// Get the list of systemd services that would block dnsmasq
    pub fn blocking_systemd_services(&self) -> Vec<String> {
        systemd::running_services_that_block_dnsmasq()
    }

    /// Stop blocking systemd services, returning whether it succeeded
    pub fn stop_blocking_services(&self, cfg: &config::DoryConfig) -> Result<Vec<String>> {
        let services = systemd::running_services_that_block_dnsmasq();
        if services.is_empty() {
            return Ok(Vec::new());
        }

        let mut stopped = Vec::new();
        for service in &services {
            if systemd::set_systemd_service(service, false, cfg.dnsmasq.service_start_delay) {
                stopped.push(service.clone());
            }
        }
        Ok(stopped)
    }

    /// Restart previously stopped systemd services
    pub fn restart_stopped_services(&self, services: &[String], cfg: &config::DoryConfig) {
        for service in services.iter().rev() {
            let success = systemd::set_systemd_service(service, true, cfg.dnsmasq.service_start_delay);
            if success {
                log::debug!("Successfully restarted {}", service);
            } else {
                log::error!("Failed to restart {}", service);
            }
        }
    }

    /// Start the dnsmasq service. Returns true if the container is running after the call.
    pub fn start(&self, cfg: &config::DoryConfig) -> Result<bool> {
        if !self.docker.is_installed() {
            log::error!("Docker is not installed. Cannot start dnsmasq.");
            return Err(AkaError::DockerNotFound);
        }

        // Check if already running
        if self.docker.is_running(&cfg.dnsmasq.container_name) {
            log::debug!(
                "dnsmasq container '{}' is already running. Doing nothing.",
                cfg.dnsmasq.container_name
            );
            return Ok(true);
        }

        // If container exists but is stopped, remove it first
        if self.docker.container_exists(&cfg.dnsmasq.container_name) {
            log::debug!(
                "Removing stale dnsmasq container '{}'.",
                cfg.dnsmasq.container_name
            );
            self.docker.remove_container(&cfg.dnsmasq.container_name)?;
        }

        let port = get_dnsmasq_port(cfg);
        let domain_args = build_domain_args(cfg);

        let run_cmd = format!(
            "docker run -d -p {port}:{port}/tcp -p {port}:{port}/udp --name {} {} {} {}",
            shell_escape(&cfg.dnsmasq.container_name),
            "--cap-add=NET_ADMIN",
            shell_escape(&self.image_name()),
            domain_args
        );

        log::debug!("Starting dnsmasq with: {}", run_cmd);

        // Parse domain args properly (they are shell-escaped: 'domain' 'address' ...)
        let domain_args_vec: Vec<String> = parse_shell_args(&domain_args);

        // Build the full arg list
        let mut args: Vec<String> = vec![
            "run".into(),
            "-d".into(),
            "-p".into(),
            format!("{port}:{port}/tcp"),
            "-p".into(),
            format!("{port}:{port}/udp"),
            "--name".into(),
            cfg.dnsmasq.container_name.clone(),
            "--cap-add".into(),
            "NET_ADMIN".into(),
        ];
        args.push(self.image_name());
        args.extend(domain_args_vec);

        // Execute via docker run
        let output = std::process::Command::new("docker")
            .args(&args.iter().map(|s| s.as_str()).collect::<Vec<_>>())
            .output();

        match output {
            Ok(o) if o.status.success() => {
                Ok(self.docker.is_running(&cfg.dnsmasq.container_name))
            }
            Ok(o) => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                log::error!("Failed to start dnsmasq: {}", stderr.trim());
                Err(AkaError::DockerCommand(format!(
                    "dnsmasq container failed to start: {}",
                    stderr.trim()
                )))
            }
            Err(e) => {
                Err(AkaError::DockerCommand(format!("failed to spawn docker: {e}")))
            }
        }
    }

    /// Stop the dnsmasq service
    pub fn stop(&self, cfg: &config::DoryConfig) -> Result<()> {
        self.docker.stop_container(&cfg.dnsmasq.container_name)?;
        Ok(())
    }

    /// Full start with conflict resolution (preconditions/postconditions)
    pub fn start_with_conflict_resolution(
        &self,
        cfg: &config::DoryConfig,
    ) -> Result<bool> {
        if !self.docker.is_installed() {
            return Err(AkaError::DockerNotFound);
        }

        let port = get_dnsmasq_port(cfg);

        // Check for port conflicts
        let conflicting_procs = self.check_port_conflict(port)?;
        if !conflicting_procs.is_empty() {
            log::warn!(
                "Port {} is in use by {} process(es)",
                port,
                conflicting_procs.len()
            );

            // Check if it's systemd services blocking the port
            let blocking_services = self.blocking_systemd_services();
            if !blocking_services.is_empty() {
                log::debug!(
                    "Stopping blocking systemd services: {:?}",
                    blocking_services
                );
                let stopped = self.stop_blocking_services(cfg)?;

                let result = self.start(cfg);

                // Restart systemd services regardless
                if !stopped.is_empty() {
                    self.restart_stopped_services(&stopped, cfg);
                }

                return result;
            }

            // If kill_others is 'no', fail
            if cfg.dnsmasq.kill_others == "no" || cfg.dnsmasq.kill_others == "false" {
                return Err(AkaError::PortInUse(port));
            }

            // If kill_others is 'yes', kill automatically
            if cfg.dnsmasq.kill_others == "yes" || cfg.dnsmasq.kill_others == "true" {
                log::debug!("kill_others=yes, killing port {} processes automatically", port);
                let killed = port_utils::kill_port_processes(port)?;
                log::debug!("Killed PIDs: {:?}", killed);
            }
            // If 'ask', we would prompt the user (skipped in non-interactive mode)
        }

        // Check for systemd conflicts
        let blocking_services = self.blocking_systemd_services();
        if !blocking_services.is_empty() {
            log::debug!(
                "Stopping blocking systemd services: {:?}",
                blocking_services
            );
            let stopped = self.stop_blocking_services(cfg)?;

            let result = self.start(cfg);

            if !stopped.is_empty() {
                self.restart_stopped_services(&stopped, cfg);
            }

            return result;
        }

        self.start(cfg)
    }
}

impl Default for DnsmasqService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DoryConfig;

    #[test]
    fn shell_escape_basic() {
        assert_eq!(shell_escape("docker"), "'docker'");
        assert_eq!(shell_escape("it's"), "'it'\\''s'");
        assert_eq!(shell_escape("127.0.0.1"), "'127.0.0.1'");
    }

    #[test]
    fn test_is_dinghy_match() {
        assert!(super::is_dinghy_match("192.168.99.100"));
        assert!(!super::is_dinghy_match("127.0.0.1"));
        assert!(!super::is_dinghy_match("10.0.0.1"));
    }

    #[test]
    fn resolve_address_passthrough() {
        assert_eq!(resolve_address("127.0.0.1"), "127.0.0.1");
        assert_eq!(resolve_address("192.168.99.100"), "192.168.99.100");
    }

    #[test]
    fn new_service() {
        let svc = DnsmasqService::new();
        assert_eq!(svc.image_name(), DEFAULT_DNSMASQ_IMAGE);
    }

    #[test]
    fn default_service() {
        let svc = DnsmasqService::default();
        assert!(svc.docker.is_installed() == docker::is_docker_installed());
    }

    #[test]
    fn test_parse_shell_args() {
        let args = parse_shell_args("'docker' '127.0.0.1'");
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], "docker");
        assert_eq!(args[1], "127.0.0.1");

        let args = parse_shell_args("'it'\\''s'");
        assert_eq!(args.len(), 1);
        assert_eq!(args[0], "it's");

        let args = parse_shell_args("'domain' '127.0.0.1' 'local' '10.0.0.1'");
        assert_eq!(args.len(), 4);
        assert_eq!(args[0], "domain");
        assert_eq!(args[1], "127.0.0.1");
        assert_eq!(args[2], "local");
        assert_eq!(args[3], "10.0.0.1");
    }

    #[test]
    fn test_build_domain_args() {
        let cfg = DoryConfig::default();
        let args = build_domain_args(&cfg);
        assert!(args.contains("docker"));
        assert!(args.contains("127.0.0.1"));
    }
}
