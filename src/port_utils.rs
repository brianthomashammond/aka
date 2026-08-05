use std::process::Command;

use crate::error::{AkaError, Result};

/// Process information from lsof output
#[derive(Debug, Clone)]
pub struct PortProcess {
    pub command: String,
    pub pid: String,
    pub user: String,
    pub name: String,
}

/// Check which processes are bound to `port` using `sudo lsof`.
pub fn check_port(port: u16) -> Result<Vec<PortProcess>> {
    let output = Command::new("sudo")
        .arg("lsof")
        .arg("-i")
        .arg(format!(":{port}"))
        .output()
        .map_err(|_| AkaError::PortInUse(port))?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut processes = Vec::new();

    for line in stdout.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 9 {
            processes.push(PortProcess {
                command: parts[0].to_string(),
                pid: parts[1].to_string(),
                user: parts[2].to_string(),
                name: parts[8].to_string(),
            });
        }
    }

    Ok(processes)
}

/// Kill all processes bound to `port`. Returns the list of killed PIDs.
pub fn kill_port_processes(port: u16) -> Result<Vec<String>> {
    let procs = check_port(port)?;
    let mut killed = Vec::new();

    for proc in &procs {
        let output = Command::new("sudo")
            .arg("kill")
            .arg(&proc.pid)
            .output()
            .map_err(|e| AkaError::DockerCommand(format!("failed to kill PID {}: {e}", proc.pid)))?;

        if output.status.success() {
            killed.push(proc.pid.clone());
        }
    }

    Ok(killed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_port_returns_ok() {
        // Port 39999 is almost certainly free; result should be Ok with an empty list.
        let result = check_port(39999);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }
}
