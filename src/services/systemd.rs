use std::process::Command;

/// Check if systemctl is available on the system
pub fn has_systemd() -> bool {
    Command::new("which")
        .arg("systemctl")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Check if a systemd service is installed
pub fn systemd_service_installed(service: &str) -> bool {
    if !has_systemd() {
        return false;
    }
    let output = Command::new("sh")
        .args(["-c", &format!("systemctl status {} | head -1", service)])
        .output();

    let stdout = match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
        _ => return false,
    };

    !stdout.contains("not-found")
}

/// Check if a systemd service is currently running
pub fn systemd_service_running(service: &str) -> bool {
    if !has_systemd() {
        return false;
    }
    let output = Command::new("sh")
        .args(["-c", &format!("systemctl status {} | head -3", service)])
        .output();

    let stdout = match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
        _ => return false,
    };

    stdout.contains("Active:") && stdout.contains("running")
}

/// Check if a systemd service is enabled (starts on boot)
pub fn systemd_service_enabled(service: &str) -> bool {
    if !has_systemd() {
        return false;
    }
    let output = Command::new("sh")
        .args(["-c", &format!("systemctl status {} | head -3", service)])
        .output();

    let stdout = match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
        _ => return false,
    };

    let without_loaded = stdout.replace("Loaded;", "");
    without_loaded.contains("enabled;")
}

/// Start or stop a systemd service. Returns success status.
/// On start, waits for service_start_delay seconds to avoid race conditions.
pub fn set_systemd_service(service: &str, up: bool, service_start_delay: u64) -> bool {
    let action = if up { "start" } else { "stop" };
    log::debug!("Requesting sudo to {} {}", action, service);

    let output = Command::new("sudo")
        .args(["systemctl", action, service])
        .output();

    let success = match output {
        Ok(o) => o.status.success(),
        Err(_) => false,
    };

    if up && success {
        log::debug!("Waiting {} seconds for {} to start", service_start_delay, service);
        std::thread::sleep(std::time::Duration::from_secs(service_start_delay));
    }

    success
}

/// Services that may conflict with dnsmasq on port 53
pub fn services_that_block_dnsmasq() -> &'static [&'static str] {
    &[
        "NetworkManager.service",
        "systemd-resolved.service",
    ]
}

/// Get the list of systemd services that are running and would block dnsmasq
pub fn running_services_that_block_dnsmasq() -> Vec<String> {
    services_that_block_dnsmasq()
        .iter()
        .filter(|s| systemd_service_running(s))
        .map(|s| s.to_string())
        .collect()
}
