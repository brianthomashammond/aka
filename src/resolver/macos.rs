use std::fs;
use std::io::Write;
use std::path::Path;

use crate::config;
use crate::error::{AkaError, Result};

/// The /etc/resolver directory on macOS
pub const RESOLVER_DIR: &str = "/etc/resolver";

/// The system resolv.conf path on macOS
pub const SYSTEM_RESOLV_FILE: &str = "/etc/resolv.conf";

/// Get the list of resolver file names (one per domain)
pub fn resolv_file_names(cfg: &config::DoryConfig) -> Vec<String> {
    if cfg.dnsmasq.domains.is_empty() {
        let names = vec!["docker".to_string()];
        names
    } else {
        cfg.dnsmasq.domains.iter().map(|d| d.domain.clone()).collect()
    }
}

/// Get the full paths to resolver files
pub fn resolv_files(dir: &str, cfg: &config::DoryConfig) -> Vec<String> {
    resolv_file_names(cfg)
        .iter()
        .map(|name| format!("{dir}/{name}"))
        .collect()
}

/// Get the nameserver address (respects dinghy VM matching)
pub fn nameserver(cfg: &config::DoryConfig) -> String {
    let ns = &cfg.resolv.nameserver;
    // Dinghy VM IPs start with 192.168.99.x
    if ns.starts_with("192.168.99.") {
        // In the original, this would call Dory::Dinghy.ip
        // For now, return as-is since we don't have dinghy detection
        ns.clone()
    } else {
        ns.clone()
    }
}

/// Check if the resolver is configured to use a dinghy VM
pub fn configured_to_use_dinghy(cfg: &config::DoryConfig) -> bool {
    nameserver(cfg).starts_with("192.168.99.")
}

/// The nameserver line for a resolver file
pub fn file_nameserver_line(cfg: &config::DoryConfig) -> String {
    let line = format!("nameserver {}", nameserver(cfg));
    line
}

/// The comment marker
pub fn file_comment() -> String {
    "# added by aka".to_string()
}

/// Generate the full contents for a resolver file
pub fn resolv_contents(cfg: &config::DoryConfig) -> String {
    let contents = format!(
        "{}\n{}\nport {}\n",
        file_comment(),
        file_nameserver_line(cfg),
        cfg.resolv.port
    );
    contents
}

/// Write resolver files to /etc/resolver/[domain]
pub fn configure(cfg: &config::DoryConfig) -> Result<()> {
    let resolver_dir = RESOLVER_DIR;
    let files = resolv_files(resolver_dir, cfg);

    // Ensure /etc/resolver directory exists
    if !Path::new(resolver_dir).exists() {
        let output = std::process::Command::new("sudo")
            .args(["mkdir", "-p", resolver_dir])
            .output()
            .map_err(|e| AkaError::PermissionDenied(format!("failed to create {}: {e}", resolver_dir)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AkaError::PermissionDenied(format!(
                "failed to create {}: {}",
                resolver_dir,
                stderr.trim()
            )));
        }
    }

    for filename in &files {
        let contents = resolv_contents(cfg);
        let output = std::process::Command::new("sudo")
            .args(["tee", filename])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| AkaError::PermissionDenied(format!("failed to tee to {}: {e}", filename)))?;

        let mut child = output;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(contents.as_bytes())
            .map_err(|e| AkaError::PermissionDenied(format!("failed to write to {}: {e}", filename)))?;

        let result = child.wait_with_output().map_err(|e| {
            AkaError::PermissionDenied(format!("failed to tee to {}: {e}", filename))
        })?;

        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            return Err(AkaError::PermissionDenied(format!(
                "failed to write {}: {}",
                filename,
                stderr.trim()
            )));
        }
    }

    Ok(())
}

/// Remove resolver files for all domains
pub fn clean(cfg: &config::DoryConfig) -> Result<()> {
    let files = resolv_files(RESOLVER_DIR, cfg);

    for filename in &files {
        let output = std::process::Command::new("sudo")
            .args(["rm", "-f", filename])
            .output()
            .map_err(|e| AkaError::PermissionDenied(format!("failed to remove {}: {e}", filename)))?;

        if !output.status.success() {
            log::warn!("failed to remove {}: {}", filename, String::from_utf8_lossy(&output.stderr));
        }
    }

    Ok(())
}

/// Check if our nameserver is configured in the resolver files
pub fn has_our_nameserver(cfg: &config::DoryConfig) -> bool {
    let files = resolv_files(RESOLVER_DIR, cfg);

    files.iter().all(|filename| {
        if let Ok(contents) = fs::read_to_string(filename) {
            contents_has_our_nameserver(&contents, cfg)
        } else {
            false
        }
    })
}

/// Check if a specific resolver file has our nameserver
pub fn contents_has_our_nameserver(contents: &str, cfg: &config::DoryConfig) -> bool {
    let comment = file_comment();
    let ns_line = file_nameserver_line(cfg);
    let port_str = format!("port {}", cfg.resolv.port);

    contents.contains(&comment)
        && contents.contains(&port_str)
        && if configured_to_use_dinghy(cfg) {
            true
        } else {
            contents.contains(&ns_line)
        }
}

/// Get the contents of a specific resolver file
pub fn resolv_file_contents(filename: &str) -> Result<String> {
    fs::read_to_string(filename)
        .map_err(|e| AkaError::ResolvRead(format!("failed to read {}: {e}", filename)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{DnsmasqConfig, DoryConfig, DomainEntry, NginxProxyConfig, ResolvConfig};

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
    fn resolv_file_names_default() {
        let cfg = default_cfg();
        let names = resolv_file_names(&cfg);
        assert_eq!(names.len(), 1);
        assert_eq!(names[0], "docker");
    }

    #[test]
    fn resolv_file_names_multiple() {
        let mut cfg = default_cfg();
        cfg.dnsmasq.domains = vec![
            DomainEntry { domain: "docker".to_string(), address: "127.0.0.1".to_string() },
            DomainEntry { domain: "local".to_string(), address: "127.0.0.1".to_string() },
        ];
        let names = resolv_file_names(&cfg);
        assert_eq!(names.len(), 2);
        assert_eq!(names[0], "docker");
        assert_eq!(names[1], "local");
    }

    #[test]
    fn resolv_files_constructed() {
        let cfg = default_cfg();
        let files = resolv_files(RESOLVER_DIR, &cfg);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0], "/etc/resolver/docker");
    }

    #[test]
    fn test_nameserver_default() {
        let cfg = default_cfg();
        assert_eq!(nameserver(&cfg), "127.0.0.1");
    }

    #[test]
    fn test_nameserver_dinghy_passthrough() {
        let mut cfg = default_cfg();
        cfg.resolv.nameserver = "192.168.99.100".to_string();
        assert_eq!(nameserver(&cfg), "192.168.99.100");
    }

    #[test]
    fn test_configured_to_use_dinghy() {
        let cfg = default_cfg();
        assert!(!configured_to_use_dinghy(&cfg));

        let mut cfg2 = default_cfg();
        cfg2.resolv.nameserver = "192.168.99.100".to_string();
        assert!(configured_to_use_dinghy(&cfg2));
    }

    #[test]
    fn test_file_nameserver_line() {
        let cfg = default_cfg();
        assert_eq!(super::file_nameserver_line(&cfg), "nameserver 127.0.0.1");
    }

    #[test]
    fn test_file_comment() {
        assert_eq!(super::file_comment(), "# added by aka");
    }

    #[test]
    fn resolv_contents_format() {
        let cfg = default_cfg();
        let contents = resolv_contents(&cfg);
        assert!(contents.contains("# added by aka"));
        assert!(contents.contains("nameserver 127.0.0.1"));
        assert!(contents.contains("port 53"));
    }

    #[test]
    fn contents_has_our_nameserver_valid() {
        let cfg = default_cfg();
        let contents = resolv_contents(&cfg);
        assert!(contents_has_our_nameserver(&contents, &cfg));
    }

    #[test]
    fn contents_has_our_nameserver_invalid() {
        let cfg = default_cfg();
        let invalid_contents = "nameserver 10.0.0.1\n# something else";
        assert!(!contents_has_our_nameserver(invalid_contents, &cfg));
    }
}
