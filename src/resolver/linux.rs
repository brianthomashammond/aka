use std::fs;
use std::io::Write;

use crate::error::{AkaError, Result};

/// Common resolv.conf path
pub const COMMON_RESOLV_FILE: &str = "/etc/resolv.conf";

/// The comment marker
pub const FILE_COMMENT: &str = "# added by aka";

/// Get the resolv.conf file path based on the system
pub fn resolv_file() -> &'static str {
    COMMON_RESOLV_FILE
}

/// Check if resolvconf utility is available
pub fn has_resolvconf() -> bool {
    std::process::Command::new("which")
        .arg("resolvconf")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Check if /etc/resolv.conf is managed by resolvconf (Ubuntu style)
pub fn is_resolvconf_managed() -> bool {
    // Check if /etc/resolv.conf is a symlink to /run/resolvconf/resolv.conf
    if let Ok(target) = fs::read_link(COMMON_RESOLV_FILE) {
        target.to_string_lossy() == "/run/resolvconf/resolv.conf"
    } else {
        false
    }
}

/// Get the nameserver line
pub fn file_nameserver_line(nameserver: &str) -> String {
    format!("nameserver {}", nameserver)
}

/// Get the full nameserver content line (with comment)
pub fn nameserver_contents(nameserver: &str) -> String {
    format!("{}  {}", file_nameserver_line(nameserver), FILE_COMMENT)
}

/// Check if the resolv.conf contents already has our nameserver
pub fn contents_has_our_nameserver(contents: &str, nameserver: &str) -> bool {
    let ns_line = file_nameserver_line(nameserver);
    contents.contains(FILE_COMMENT) && contents.contains(&ns_line)
}

/// Configure resolv.conf by prepending our nameserver
pub fn configure(nameserver: &str) -> Result<bool> {
    let resolv_path = resolv_file();

    // Read current contents
    let prev_contents = fs::read_to_string(resolv_path)
        .map_err(|e| AkaError::ResolvRead(format!("failed to read {}: {e}", resolv_path)))?;

    // Check if already configured
    if contents_has_our_nameserver(&prev_contents, nameserver) {
        return Ok(true);
    }

    let ns_line = nameserver_contents(nameserver);

    let new_contents = if prev_contents.contains("nameserver") {
        // Prepend our nameserver before the first existing nameserver line
        prev_contents.replacen(
            "nameserver",
            &format!("{}\nnameserver", ns_line),
            1,
        )
    } else {
        // No nameserver line found, append
        format!("{}\n{}", prev_contents.trim(), ns_line)
    };

    // Remove trailing whitespace
    let new_contents = new_contents.trim().to_string();

    // Write via sudo tee (we don't run as root)
    let output = std::process::Command::new("sudo")
        .args(&["tee", resolv_path])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| AkaError::ResolvWrite(format!("failed to tee to {}: {e}", resolv_path)))?;

    let mut child = output;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(new_contents.as_bytes())
        .map_err(|e| AkaError::ResolvWrite(format!("failed to write to {}: {e}", resolv_path)))?;

    let result = child.wait_with_output().map_err(|e| {
        AkaError::ResolvWrite(format!("failed to tee to {}: {e}", resolv_path))
    })?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        return Err(AkaError::ResolvWrite(format!(
            "failed to write {}: {}",
            resolv_path,
            stderr.trim()
        )));
    }

    Ok(contents_has_our_nameserver(&new_contents, nameserver))
}

/// Configure via resolvconf utility
pub fn configure_resolvconf(nameserver: &str) -> Result<bool> {
    let ns_line = nameserver_contents(nameserver);

    let output = std::process::Command::new("sudo")
        .args(&["resolvconf", "-a", "lo.dory"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| AkaError::ResolvWrite(format!("failed to run resolvconf: {e}")))?;

    let mut child = output;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(ns_line.as_bytes())
        .map_err(|e| AkaError::ResolvWrite(format!("failed to write to resolvconf: {e}")))?;

    let result = child.wait_with_output().map_err(|e| {
        AkaError::ResolvWrite(format!("failed to run resolvconf: {e}"))
    })?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        log::error!("resolvconf failed: {}", stderr.trim());
        return Err(AkaError::ResolvWrite(format!(
            "resolvconf failed: {}",
            stderr.trim()
        )));
    }

    Ok(true)
}

/// Clean resolv.conf by removing our nameserver line
pub fn clean(nameserver: &str) -> Result<bool> {
    let resolv_path = resolv_file();

    let mut contents = fs::read_to_string(resolv_path)
        .map_err(|e| AkaError::ResolvRead(format!("failed to read {}: {e}", resolv_path)))?;

    if !contents_has_our_nameserver(&contents, nameserver) {
        return Ok(true);
    }

    let ns_line = nameserver_contents(nameserver);

    // Remove our nameserver line
    contents = contents.replace(&format!("{}\n", ns_line), "");

    // Remove trailing whitespace
    contents = contents.trim().to_string();

    // Write via sudo tee
    let output = std::process::Command::new("sudo")
        .args(&["tee", resolv_path])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| AkaError::ResolvWrite(format!("failed to tee to {}: {e}", resolv_path)))?;

    let mut child = output;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(contents.as_bytes())
        .map_err(|e| AkaError::ResolvWrite(format!("failed to write to {}: {e}", resolv_path)))?;

    let result = child.wait_with_output().map_err(|e| {
        AkaError::ResolvWrite(format!("failed to tee to {}: {e}", resolv_path))
    })?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        return Err(AkaError::ResolvWrite(format!(
            "failed to write {}: {}",
            resolv_path,
            stderr.trim()
        )));
    }

    Ok(!contents_has_our_nameserver(&contents, nameserver))
}

/// Clean via resolvconf
pub fn clean_resolvconf() -> Result<bool> {
    let output = std::process::Command::new("sudo")
        .args(&["resolvconf", "-d", "lo.dory"])
        .output()
        .map_err(|e| AkaError::ResolvWrite(format!("failed to run resolvconf: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        log::warn!("resolvconf clean failed: {}", stderr.trim());
    }

    Ok(true)
}

/// Linux-specific resolver management
pub struct LinuxResolver;

impl LinuxResolver {
    pub fn new() -> Self {
        Self
    }

    pub fn configure(&self, nameserver: &str) -> Result<bool> {
        if has_resolvconf() && is_resolvconf_managed() {
            configure_resolvconf(nameserver)
        } else {
            configure(nameserver)
        }
    }

    pub fn clean(&self, nameserver: &str) -> Result<bool> {
        if has_resolvconf() && is_resolvconf_managed() {
            clean_resolvconf()
        } else {
            clean(nameserver)
        }
    }

    pub fn has_our_nameserver(&self, nameserver: &str) -> bool {
        if let Ok(contents) = fs::read_to_string(resolv_file()) {
            contents_has_our_nameserver(&contents, nameserver)
        } else {
            false
        }
    }

    pub fn resolv_file(&self) -> &'static str {
        resolv_file()
    }
}

/// Standalone function for use from mod.rs
pub fn has_our_nameserver_static(nameserver: &str) -> bool {
    let resolver = LinuxResolver::new();
    resolver.has_our_nameserver(nameserver)
}

impl Default for LinuxResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_nameserver_line() {
        assert_eq!(super::file_nameserver_line("127.0.0.1"), "nameserver 127.0.0.1");
    }

    #[test]
    fn test_nameserver_contents() {
        let contents = nameserver_contents("127.0.0.1");
        assert!(contents.contains("nameserver 127.0.0.1"));
        assert!(contents.contains(FILE_COMMENT));
    }

    #[test]
    fn test_contents_has_our_nameserver_valid() {
        let valid = "nameserver 8.8.8.8\nnameserver 127.0.0.1  # added by aka";
        assert!(super::contents_has_our_nameserver(valid, "127.0.0.1"));
    }

    #[test]
    fn test_contents_has_our_nameserver_invalid() {
        let invalid = "nameserver 8.8.8.8\nnameserver 10.0.0.1";
        assert!(!contents_has_our_nameserver(invalid, "127.0.0.1"));
    }

    #[test]
    fn test_contents_has_our_nameserver_missing_comment() {
        let no_comment = "nameserver 8.8.8.8\nnameserver 127.0.0.1";
        assert!(!contents_has_our_nameserver(no_comment, "127.0.0.1"));
    }

    #[test]
    fn resolv_file_is_common() {
        assert_eq!(resolv_file(), "/etc/resolv.conf");
    }
}
