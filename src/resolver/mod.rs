pub mod linux;
pub mod macos;
pub mod os;

use std::fs;

use crate::config;
use crate::error::{AkaError, Result};

/// Configure the system DNS resolver
pub fn configure(cfg: &config::DoryConfig) -> Result<()> {
    if !cfg.resolv.enabled {
        log::info!("resolv is disabled in config, skipping");
        return Ok(());
    }

    let os_type = os::current_os();
    match os_type {
        os::OsType::Macos => {
            macos::configure(cfg)?;
            log::info!("macOS resolver configured for domains: {:?}",
                macos::resolv_file_names(cfg));
        }
        os::OsType::Linux => {
            let nameserver = &cfg.resolv.nameserver;
            linux::LinuxResolver::new().configure(nameserver)?;
            log::info!("Linux resolver configured with nameserver: {}", nameserver);
        }
        os::OsType::Unknown => {
            return Err(AkaError::OsDetection(
                "unsupported operating system".to_string()
            ));
        }
    }

    Ok(())
}

/// Clean up the system DNS resolver configuration
pub fn clean(cfg: &config::DoryConfig) -> Result<()> {
    if !cfg.resolv.enabled {
        return Ok(());
    }

    let os_type = os::current_os();
    match os_type {
        os::OsType::Macos => {
            macos::clean(cfg)?;
            log::info!("macOS resolver cleaned for domains: {:?}",
                macos::resolv_file_names(cfg));
        }
        os::OsType::Linux => {
            let nameserver = &cfg.resolv.nameserver;
            linux::LinuxResolver::new().clean(nameserver)?;
            log::info!("Linux resolver cleaned with nameserver: {}", nameserver);
        }
        os::OsType::Unknown => {
            return Err(AkaError::OsDetection(
                "unsupported operating system".to_string()
            ));
        }
    }

    Ok(())
}

/// Check if our nameserver is configured in the system resolver
pub fn has_our_nameserver(cfg: &config::DoryConfig) -> bool {
    let os_type = os::current_os();
    match os_type {
        os::OsType::Macos => macos::has_our_nameserver(cfg),
        os::OsType::Linux => {
            linux::has_our_nameserver_static(&cfg.resolv.nameserver)
        }
        os::OsType::Unknown => false,
    }
}

/// Get the resolv file path for the current OS
pub fn resolv_file() -> &'static str {
    let os_type = os::current_os();
    match os_type {
        os::OsType::Linux => linux::resolv_file(),
        os::OsType::Macos => macos::SYSTEM_RESOLV_FILE,
        os::OsType::Unknown => "/etc/resolv.conf",
    }
}

/// Get the resolv file contents for the current OS
pub fn resolv_file_contents() -> Result<String> {
    let path = resolv_file();
    fs::read_to_string(path)
        .map_err(|e| AkaError::ResolvRead(format!("failed to read {}: {e}", path)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{DnsmasqConfig, DoryConfig, NginxProxyConfig, ResolvConfig};

    fn default_cfg() -> DoryConfig {
        DoryConfig {
            dnsmasq: DnsmasqConfig::default(),
            nginx_proxy: NginxProxyConfig::default(),
            resolv: ResolvConfig {
                enabled: true,
                nameserver: "127.0.0.1".to_string(),
                port: 53,
            },
            debug: false,
        }
    }

    #[test]
    fn resolv_file_returns_path() {
        let path = resolv_file();
        assert!(!path.is_empty());
    }

    #[test]
    fn has_our_nameserver_false_by_default() {
        let cfg = default_cfg();
        // On a fresh system without aka config, this should be false
        assert!(!has_our_nameserver(&cfg));
    }

    #[test]
    fn disabled_resolv_skips() {
        let mut cfg = default_cfg();
        cfg.resolv.enabled = false;
        // configure should return Ok without doing anything
        let result = configure(&cfg);
        assert!(result.is_ok());
    }
}
