use std::process::Command;

/// Check if systemctl is available on the system
pub fn has_systemd() -> bool {
    Command::new("which")
        .arg("systemctl")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
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

/// Start or stop a systemd service. Returns whether the service actually reached
/// the desired state (running for start, stopped for stop) within `timeout_secs`.
pub fn set_systemd_service(service: &str, up: bool, timeout_secs: u64) -> bool {
    let action = if up { "start" } else { "stop" };
    log::debug!("Requesting sudo to {} {}", action, service);

    let output = Command::new("sudo")
        .args(["systemctl", action, service])
        .output();

    let success = match output {
        Ok(o) => o.status.success(),
        Err(_) => false,
    };

    if !success {
        return false;
    }

    wait_for_service_state(service, up, timeout_secs)
}

/// Poll `systemd_service_running` until it matches `want_running`, or `timeout_secs` elapses.
fn wait_for_service_state(service: &str, want_running: bool, timeout_secs: u64) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    let poll_interval = std::time::Duration::from_millis(200);

    loop {
        if systemd_service_running(service) == want_running {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            log::warn!(
                "Timed out waiting for {} to reach {} state",
                service,
                if want_running { "running" } else { "stopped" }
            );
            return false;
        }
        std::thread::sleep(poll_interval);
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocking_services_list() {
        let services = services_that_block_dnsmasq();
        assert_eq!(services.len(), 2);
        assert!(services.contains(&"NetworkManager.service"));
        assert!(services.contains(&"systemd-resolved.service"));
    }
}
