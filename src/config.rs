use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{AkaError, Result};

/// Top-level config wrapped in "aka" key
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub aka: DoryConfig,
}

/// All service configurations
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DoryConfig {
    #[serde(default)]
    pub dnsmasq: DnsmasqConfig,

    #[serde(default)]
    pub nginx_proxy: NginxProxyConfig,

    #[serde(default)]
    pub resolv: ResolvConfig,

    #[serde(default)]
    pub debug: bool,
}

/// Dnsmasq service configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsmasqConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub domains: Vec<DomainEntry>,

    #[serde(default)]
    pub domain: Option<String>,

    #[serde(default)]
    pub address: Option<String>,

    #[serde(default = "default_dnsmasq_container_name")]
    pub container_name: String,

    #[serde(default = "default_dnsmasq_port")]
    pub port: u16,

    #[serde(default = "default_kill_others")]
    pub kill_others: String,

    #[serde(default = "default_service_start_delay")]
    pub service_start_delay: u64,
}

/// A single domain mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainEntry {
    pub domain: String,
    pub address: String,
}

/// Nginx proxy service configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NginxProxyConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,

    #[serde(default = "default_proxy_container_name")]
    pub container_name: String,

    #[serde(default = "default_true")]
    pub https_enabled: bool,

    #[serde(default)]
    pub ssl_certs_dir: String,

    #[serde(default = "default_proxy_port")]
    pub port: u16,

    #[serde(default = "default_tls_port")]
    pub tls_port: u16,
}

/// Resolv (DNS resolver) configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,

    #[serde(default = "default_nameserver")]
    pub nameserver: String,

    #[serde(default = "default_resolv_port")]
    pub port: u16,
}

// ── Defaults ────────────────────────────────────────────────────────────────

fn default_true() -> bool {
    true
}

fn default_dnsmasq_container_name() -> String {
    "aka_dnsmasq".into()
}

fn default_dnsmasq_port() -> u16 {
    53
}

fn default_kill_others() -> String {
    "ask".into()
}

fn default_service_start_delay() -> u64 {
    5
}

fn default_proxy_container_name() -> String {
    "aka_http_proxy".into()
}

fn default_proxy_port() -> u16 {
    80
}

fn default_tls_port() -> u16 {
    443
}

fn default_resolv_port() -> u16 {
    53
}

fn default_nameserver() -> String {
    "127.0.0.1".into()
}

impl Default for DnsmasqConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            domains: vec![DomainEntry {
                domain: "docker".into(),
                address: "127.0.0.1".into(),
            }],
            domain: None,
            address: None,
            container_name: default_dnsmasq_container_name(),
            port: default_dnsmasq_port(),
            kill_others: default_kill_others(),
            service_start_delay: default_service_start_delay(),
        }
    }
}

impl Default for NginxProxyConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            container_name: default_proxy_container_name(),
            https_enabled: true,
            ssl_certs_dir: String::new(),
            port: default_proxy_port(),
            tls_port: default_tls_port(),
        }
    }
}

impl Default for ResolvConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            nameserver: default_nameserver(),
            port: default_resolv_port(),
        }
    }
}

// ── Partial structs for deserialization ─────────────────────────────────────
// These use Option<T> for every field so the merge layer can distinguish
// "key absent from file" (None) from "key explicitly set to the default" (Some(default)).

#[derive(Debug, Default, Deserialize)]
struct PartialDnsmasqConfig {
    enabled: Option<bool>,
    domains: Option<Vec<DomainEntry>>,
    container_name: Option<String>,
    port: Option<u16>,
    kill_others: Option<String>,
    service_start_delay: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
struct PartialNginxProxyConfig {
    enabled: Option<bool>,
    container_name: Option<String>,
    https_enabled: Option<bool>,
    ssl_certs_dir: Option<String>,
    port: Option<u16>,
    tls_port: Option<u16>,
}

#[derive(Debug, Default, Deserialize)]
struct PartialResolvConfig {
    enabled: Option<bool>,
    nameserver: Option<String>,
    port: Option<u16>,
}

#[derive(Debug, Default, Deserialize)]
struct PartialDoryConfig {
    #[serde(default)]
    dnsmasq: Option<PartialDnsmasqConfig>,
    #[serde(default)]
    nginx_proxy: Option<PartialNginxProxyConfig>,
    #[serde(default)]
    resolv: Option<PartialResolvConfig>,
    debug: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
struct PartialConfig {
    #[serde(default)]
    aka: PartialDoryConfig,
}

// ── Default config YAML ─────────────────────────────────────────────────────

pub fn default_config_yaml() -> String {
    r#"---
aka:
  # Be careful if you change the settings of some of
  # these services.  They may not talk to each other
  # if you change IP Addresses.
  # For example, resolv expects a nameserver listening at
  # the specified address.  dnsmasq normally does this,
  # but if you disable dnsmasq, it
  # will make your system look for a name server that
  # doesn't exist.
  dnsmasq:
    enabled: true
    domains:               # array of domains that will be resolved to the specified address
      - domain: docker     # you can set '#' for a wildcard
        address: 127.0.0.1 # return for queries against the domain
    container_name: aka_dnsmasq
    port: 53  # port to listen for dns requests on.  must be 53 on linux. can be anything that's open on macos
    # kill_others: kill processes bound to the port we need (see previous setting 'port')
    #   Possible values:
    #     ask (prompt about killing each time. User can accept/reject)
    #     yes|true (go ahead and kill without asking)
    #     no|false (don't kill, and don't even ask)
    kill_others: ask
    service_start_delay: 5  # max seconds to wait for a systemd service to confirm it stopped/started
  nginx_proxy:
    enabled: true
    container_name: aka_http_proxy
    https_enabled: true
    ssl_certs_dir: ''  # leave as empty string to use default certs
    port: 80           # port 80 is default for http
    tls_port: 443      # port 443 is default for https
  resolv:
    enabled: true
    nameserver: 127.0.0.1
    port: 53  # port where the nameserver listens. On linux it must be 53
"#
    .into()
}

// ── Config file discovery ───────────────────────────────────────────────────

/// Find the first `.aka.yml` starting from PWD and walking up to root.
/// Returns the home config path as a fallback (but doesn't require it to exist).
pub fn find_project_config(starting_dir: &Path) -> Option<PathBuf> {
    let mut current = starting_dir.to_path_buf();

    loop {
        let candidate = current.join(".aka.yml");
        if candidate.exists() {
            return Some(candidate);
        }

        match current.parent() {
            Some(parent) if parent == current.as_path() => break,
            Some(parent) => current = parent.to_path_buf(),
            None => break,
        }
    }

    None
}

/// Returns the home config file path (~/.aka.yml)
pub fn home_config_path() -> PathBuf {
    let home = dirs::home_dir().expect("could not determine home directory");
    home.join(".aka.yml")
}

// ── Deep merge ──────────────────────────────────────────────────────────────

/// Deep-merge `source` into `target`. Only `Some` fields in `source` are applied,
/// so a field explicitly set to its default value in a config file still wins over
/// whatever the lower-priority layer had.
fn merge_dory_config(target: &mut DoryConfig, source: &PartialDoryConfig) {
    if let Some(dns) = &source.dnsmasq {
        if let Some(v) = dns.enabled { target.dnsmasq.enabled = v; }
        if let Some(v) = &dns.domains { target.dnsmasq.domains = v.clone(); }
        if let Some(v) = &dns.container_name { target.dnsmasq.container_name = v.clone(); }
        if let Some(v) = dns.port { target.dnsmasq.port = v; }
        if let Some(v) = &dns.kill_others { target.dnsmasq.kill_others = v.clone(); }
        if let Some(v) = dns.service_start_delay { target.dnsmasq.service_start_delay = v; }
    }

    if let Some(proxy) = &source.nginx_proxy {
        if let Some(v) = proxy.enabled { target.nginx_proxy.enabled = v; }
        if let Some(v) = &proxy.container_name { target.nginx_proxy.container_name = v.clone(); }
        if let Some(v) = proxy.https_enabled { target.nginx_proxy.https_enabled = v; }
        if let Some(v) = &proxy.ssl_certs_dir { target.nginx_proxy.ssl_certs_dir = v.clone(); }
        if let Some(v) = proxy.port { target.nginx_proxy.port = v; }
        if let Some(v) = proxy.tls_port { target.nginx_proxy.tls_port = v; }
    }

    if let Some(resolv) = &source.resolv {
        if let Some(v) = resolv.enabled { target.resolv.enabled = v; }
        if let Some(v) = &resolv.nameserver { target.resolv.nameserver = v.clone(); }
        if let Some(v) = resolv.port { target.resolv.port = v; }
    }

    if let Some(v) = source.debug { target.debug = v; }
}

// ── Load ────────────────────────────────────────────────────────────────────

/// Load the full merged configuration:
///   1. Start with hardcoded defaults
///   2. Overlay ~/.aka.yml (if it exists)
///   3. Overlay project-specific .aka.yml (if found)
pub fn load_config(starting_dir: &Path) -> Result<Config> {
    // Layer 1: defaults
    let mut config = Config {
        aka: DoryConfig::default(),
    };

    // Layer 2: home config
    let home_path = home_config_path();
    if home_path.exists() {
        let home_yaml = fs::read_to_string(&home_path)
            .map_err(|e| AkaError::ConfigRead(home_path.to_string_lossy().into(), e))?;
        let home_config: PartialConfig = serde_yml::from_str(&home_yaml)
            .map_err(|e| AkaError::ConfigParse(home_path.to_string_lossy().into(), e))?;
        merge_dory_config(&mut config.aka, &home_config.aka);
    }

    // Layer 3: project config (closest to PWD)
    if let Some(project_path) = find_project_config(starting_dir) {
        let project_yaml = fs::read_to_string(&project_path).map_err(|e| {
            AkaError::ConfigRead(project_path.to_string_lossy().into(), e)
        })?;
        let project_config: PartialConfig = serde_yml::from_str(&project_yaml).map_err(|e| {
            AkaError::ConfigParse(project_path.to_string_lossy().into(), e)
        })?;
        merge_dory_config(&mut config.aka, &project_config.aka);
    }

    Ok(config)
}

// ── Write default config file ───────────────────────────────────────────────

/// Write a default config file to the given path.
/// Returns true if the file was created, false if it already existed.
pub fn write_default_config(path: &Path, force: bool) -> Result<bool> {
    if path.exists() && !force {
        return Ok(false);
    }

    let yaml = default_config_yaml();
    fs::write(path, yaml)
        .map_err(|e| AkaError::ConfigRead(path.to_string_lossy().into(), e))?;
    Ok(true)
}

/// Write default config to ~/.aka.yml. Returns the path written to.
pub fn write_default_home_config(force: bool) -> Result<PathBuf> {
    let path = home_config_path();
    write_default_config(&path, force)?;
    Ok(path)
}

// ── Config upgrade ──────────────────────────────────────────────────────────

/// Upgrade a config to the latest format, preserving user settings.
/// This migrates old single-domain format to the array format, adds missing fields.
pub fn upgrade_config(old: &Config) -> Config {
    let mut upgraded = old.clone();

    // Migrate old single domain to array format
    if let Some(ref old_domain) = old.aka.dnsmasq.domain {
        let address = old.aka.dnsmasq.address.as_deref().unwrap_or("127.0.0.1");
        upgraded.aka.dnsmasq.domains = vec![DomainEntry {
            domain: old_domain.clone(),
            address: address.to_string(),
        }];
        upgraded.aka.dnsmasq.domain = None;
        upgraded.aka.dnsmasq.address = None;
    }

    // Ensure kill_others has a default
    if upgraded.aka.dnsmasq.kill_others.is_empty() {
        upgraded.aka.dnsmasq.kill_others = "ask".to_string();
    }

    // Ensure service_start_delay has a default
    if upgraded.aka.dnsmasq.service_start_delay == 0 {
        upgraded.aka.dnsmasq.service_start_delay = 5;
    }

    // Ensure nginx proxy ports have defaults
    if upgraded.aka.nginx_proxy.port == 0 {
        upgraded.aka.nginx_proxy.port = 80;
    }
    if upgraded.aka.nginx_proxy.tls_port == 0 {
        upgraded.aka.nginx_proxy.tls_port = 443;
    }

    upgraded
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_dir(name: &str) -> PathBuf {
        let p = PathBuf::from(format!("/tmp/aka_test_{}", name));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn write_yaml(dir: &Path, filename: &str, content: &str) {
        let path = dir.join(filename);
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f.flush().unwrap();
        drop(f);
        // Ensure it exists
        assert!(path.exists());
    }

    #[test]
    fn default_config_has_expected_values() {
        let d = DoryConfig::default();
        assert!(d.dnsmasq.enabled);
        assert_eq!(d.dnsmasq.domains.len(), 1);
        assert_eq!(d.dnsmasq.domains[0].domain, "docker");
        assert_eq!(d.dnsmasq.port, 53);
        assert!(d.nginx_proxy.enabled);
        assert_eq!(d.nginx_proxy.port, 80);
        assert_eq!(d.nginx_proxy.tls_port, 443);
        assert!(d.resolv.enabled);
        assert_eq!(d.resolv.nameserver, "127.0.0.1");
    }

    #[test]
    fn deserialize_full_config() {
        let yaml = r#"
---
aka:
  dnsmasq:
    enabled: true
    domains:
      - domain: docker
        address: 127.0.0.1
      - domain: local
        address: 127.0.0.1
    container_name: my_dnsmasq
    port: 5353
    kill_others: yes
    service_start_delay: 10
  nginx_proxy:
    enabled: true
    container_name: my_proxy
    https_enabled: false
    ssl_certs_dir: /custom/certs
    port: 8080
    tls_port: 8443
  resolv:
    enabled: true
    nameserver: 127.0.0.1
    port: 5353
"#;
        let config: Config = serde_yml::from_str(yaml).unwrap();
        assert_eq!(config.aka.dnsmasq.port, 5353);
        assert_eq!(config.aka.dnsmasq.domains.len(), 2);
        assert_eq!(config.aka.dnsmasq.domains[1].domain, "local");
        assert_eq!(config.aka.dnsmasq.container_name, "my_dnsmasq");
        assert_eq!(config.aka.nginx_proxy.port, 8080);
        assert_eq!(config.aka.nginx_proxy.tls_port, 8443);
        assert!(!config.aka.nginx_proxy.https_enabled);
        assert_eq!(config.aka.nginx_proxy.ssl_certs_dir, "/custom/certs");
        assert_eq!(config.aka.resolv.port, 5353);
    }

    #[test]
    fn deserialize_minimal_config() {
        let yaml = r#"---
aka:
  dnsmasq:
    enabled: false
"#;
        let config: Config = serde_yml::from_str(yaml).unwrap();
        assert!(!config.aka.dnsmasq.enabled);
        // All other fields should be defaults
        assert!(config.aka.nginx_proxy.enabled);
        assert_eq!(config.aka.nginx_proxy.port, 80);
    }

    #[test]
    fn find_project_config_finds_in_current_dir() {
        let dir = temp_dir("find_in_cwd");
        write_yaml(&dir, ".aka.yml", "aka:\n  dnsmasq:\n    enabled: false\n");
        assert_eq!(find_project_config(&dir), Some(dir.join(".aka.yml")));
    }

    #[test]
    fn find_project_config_walks_up() {
        let parent = temp_dir("find_walk_up");
        let child = parent.join("subdir");
        fs::create_dir_all(&child).unwrap();
        write_yaml(&parent, ".aka.yml", "aka:\n  dnsmasq:\n    enabled: false\n");
        assert_eq!(find_project_config(&child), Some(parent.join(".aka.yml")));
    }

    #[test]
    fn find_project_config_returns_none_when_missing() {
        let dir = temp_dir("find_none");
        assert_eq!(find_project_config(&dir), None);
    }

    #[test]
    fn merge_home_overlays_defaults() {
        let mut target = DoryConfig::default();
        let source = PartialDoryConfig {
            dnsmasq: Some(PartialDnsmasqConfig {
                enabled: Some(false),
                ..Default::default()
            }),
            ..Default::default()
        };
        merge_dory_config(&mut target, &source);
        assert!(!target.dnsmasq.enabled);
        assert!(target.nginx_proxy.enabled);
        assert_eq!(target.resolv.port, 53);
    }

    #[test]
    fn merge_project_overlays_home() {
        let mut target = DoryConfig {
            dnsmasq: DnsmasqConfig { port: 5353, ..Default::default() },
            ..Default::default()
        };
        let source = PartialDoryConfig {
            dnsmasq: Some(PartialDnsmasqConfig {
                port: Some(19323),
                ..Default::default()
            }),
            ..Default::default()
        };
        merge_dory_config(&mut target, &source);
        assert_eq!(target.dnsmasq.port, 19323);
    }

    #[test]
    fn merge_can_reenable_service_disabled_by_home_config() {
        // home config disables dnsmasq
        let mut target = DoryConfig::default();
        merge_dory_config(&mut target, &PartialDoryConfig {
            dnsmasq: Some(PartialDnsmasqConfig { enabled: Some(false), ..Default::default() }),
            ..Default::default()
        });
        assert!(!target.dnsmasq.enabled);

        // project config re-enables it (true == default, old code would silently drop this)
        merge_dory_config(&mut target, &PartialDoryConfig {
            dnsmasq: Some(PartialDnsmasqConfig { enabled: Some(true), ..Default::default() }),
            ..Default::default()
        });
        assert!(target.dnsmasq.enabled);
    }

    #[test]
    fn merge_can_reset_field_to_default_value() {
        // home config sets a non-default port
        let mut target = DoryConfig::default();
        merge_dory_config(&mut target, &PartialDoryConfig {
            dnsmasq: Some(PartialDnsmasqConfig { port: Some(5353), ..Default::default() }),
            ..Default::default()
        });
        assert_eq!(target.dnsmasq.port, 5353);

        // project config resets it to 53 (the default — old code would silently drop this)
        merge_dory_config(&mut target, &PartialDoryConfig {
            dnsmasq: Some(PartialDnsmasqConfig { port: Some(53), ..Default::default() }),
            ..Default::default()
        });
        assert_eq!(target.dnsmasq.port, 53);
    }

    #[test]
    fn write_default_config_creates_file() {
        let dir = temp_dir("write_config");
        let path = dir.join(".aka.yml");
        let created = write_default_config(&path, false).unwrap();
        assert!(created);
        assert!(path.exists());
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("aka:"));
        assert!(content.contains("dnsmasq:"));
    }

    #[test]
    fn write_default_config_skips_when_exists() {
        let dir = temp_dir("write_config_skip");
        let path = dir.join(".aka.yml");
        fs::File::create(&path).unwrap();
        let created = write_default_config(&path, false).unwrap();
        assert!(!created);
        assert!(path.exists());
    }

    #[test]
    fn write_default_config_overwrites_with_force() {
        let dir = temp_dir("write_config_force");
        let path = dir.join(".aka.yml");
        fs::write(&path, "old content").unwrap();
        let created = write_default_config(&path, true).unwrap();
        assert!(created);
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("aka:"));
        assert!(!content.contains("old content"));
    }

    #[test]
    fn default_config_yaml_is_valid() {
        let yaml = default_config_yaml();
        let config: Config = serde_yml::from_str(&yaml).unwrap();
        assert!(config.aka.dnsmasq.enabled);
        assert_eq!(config.aka.dnsmasq.domains[0].domain, "docker");
        assert!(config.aka.nginx_proxy.enabled);
        assert!(config.aka.resolv.enabled);
    }

    #[test]
    fn test_upgrade_config_migrates_single_domain() {
        let yaml = r#"
---
aka:
  dnsmasq:
    enabled: true
    domain: docker
    address: 10.0.0.1
    container_name: my_dnsmasq
    port: 53
    kill_others: ask
    service_start_delay: 5
  nginx_proxy:
    enabled: true
    container_name: my_proxy
    https_enabled: true
    ssl_certs_dir: ''
    port: 80
    tls_port: 443
  resolv:
    enabled: true
    nameserver: 127.0.0.1
    port: 53
"#;
        let config: Config = serde_yml::from_str(yaml).unwrap();
        assert!(config.aka.dnsmasq.domain.is_some());
        assert!(config.aka.dnsmasq.address.is_some());
        assert!(config.aka.dnsmasq.domains.is_empty());

        let upgraded = upgrade_config(&config);
        assert!(upgraded.aka.dnsmasq.domain.is_none());
        assert!(upgraded.aka.dnsmasq.address.is_none());
        assert_eq!(upgraded.aka.dnsmasq.domains.len(), 1);
        assert_eq!(upgraded.aka.dnsmasq.domains[0].domain, "docker");
        assert_eq!(upgraded.aka.dnsmasq.domains[0].address, "10.0.0.1");
    }

    #[test]
    fn test_upgrade_config_adds_defaults() {
        let yaml = r#"
---
aka:
  dnsmasq:
    enabled: true
    domains:
      - domain: docker
        address: 127.0.0.1
  nginx_proxy:
    enabled: true
  resolv:
    enabled: true
"#;
        let config: Config = serde_yml::from_str(yaml).unwrap();
        let upgraded = upgrade_config(&config);
        assert_eq!(upgraded.aka.nginx_proxy.port, 80);
        assert_eq!(upgraded.aka.nginx_proxy.tls_port, 443);
        assert_eq!(upgraded.aka.dnsmasq.kill_others, "ask");
        assert_eq!(upgraded.aka.dnsmasq.service_start_delay, 5);
    }
}
