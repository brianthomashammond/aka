use std::process::Command;

use crate::error::{AkaError, Result};

/// Status of a docker container
#[derive(Debug, Clone)]
pub struct ContainerStatus {
    pub running: bool,
    pub exists: bool,
}

/// Execute a docker command and return success/failure with output
fn docker_exec(args: &[&str]) -> Result<(bool, String)> {
    let output = Command::new("docker")
        .args(args)
        .output()
        .map_err(|e| AkaError::DockerCommand(format!("failed to spawn docker: {e}")))?;

    let success = output.status.success();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !success && stderr.trim().is_empty() {
        return Ok((false, stdout.trim().to_string()));
    }

    if !success {
        return Ok((false, stderr.trim().to_string()));
    }

    Ok((true, stdout.trim().to_string()))
}

/// Check if docker is installed and accessible
pub fn is_docker_installed() -> bool {
    Command::new("docker")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Docker client for managing containers via the Docker CLI
pub struct DockerClient;

impl DockerClient {
    pub fn new() -> Self {
        Self
    }

    /// Check if docker is installed
    pub fn is_installed(&self) -> bool {
        is_docker_installed()
    }

    /// Check if a container exists (running or stopped)
    pub fn container_exists(&self, name: &str) -> bool {
        let result = docker_exec(&["ps", "-a", "--filter", &format!("name=^/{name}$"), "--format", "{{.Names}}"])
            .unwrap_or((false, String::new()));
        result.0 && !result.1.is_empty()
    }

    /// Check if a container is currently running
    pub fn is_running(&self, name: &str) -> bool {
        let result = docker_exec(&["ps", "--filter", &format!("name=^/{name}$"), "--format", "{{.Names}}"])
            .unwrap_or((false, String::new()));
        result.0 && !result.1.is_empty()
    }

    /// Get the status of a container
    pub fn get_container_status(&self, name: &str) -> ContainerStatus {
        ContainerStatus {
            running: self.is_running(name),
            exists: self.container_exists(name),
        }
    }

    /// Stop a running container
    pub fn stop_container(&self, name: &str) -> Result<()> {
        if !self.is_running(name) {
            return Ok(());
        }

        let (success, msg) = docker_exec(&["stop", name])?;
        if success {
            Ok(())
        } else {
            Err(AkaError::DockerCommand(format!("failed to stop container '{name}': {msg}")))
        }
    }

    /// Remove a container (force if still running)
    pub fn remove_container(&self, name: &str) -> Result<()> {
        if !self.container_exists(name) {
            return Ok(());
        }

        let (success, msg) = docker_exec(&["rm", "-f", name])?;
        if success {
            Ok(())
        } else {
            Err(AkaError::DockerCommand(format!("failed to remove container '{name}': {msg}")))
        }
    }

    /// Run a container with the given parameters.
    /// `ports` is a list of (host_port, container_port) pairs.
    /// `env_vars` is a list of ("KEY", "VALUE") pairs.
    /// `volumes` is a list of "host:container" mount strings.
    /// `caps` is a list of Linux capabilities to add (e.g. "NET_ADMIN").
    pub fn run_container(
        &self,
        image: &str,
        name: &str,
        ports: &[(u16, u16)],
        env_vars: &[(&str, &str)],
        volumes: &[&str],
        caps: &[&str],
    ) -> Result<()> {
        let mut args: Vec<String> = vec!["run".into(), "-d".into()];

        args.push("--name".into());
        args.push(name.into());

        for (host_port, container_port) in ports {
            args.push("-p".into());
            args.push(format!("{host_port}:{container_port}"));
        }

        for (key, value) in env_vars {
            args.push("-e".into());
            args.push(format!("{key}={value}"));
        }

        for volume in volumes {
            args.push("-v".into());
            args.push((*volume).into());
        }

        for cap in caps {
            args.push("--cap-add".into());
            args.push((*cap).into());
        }

        args.push(image.into());

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let (success, msg) = docker_exec(&args_refs)?;
        if success {
            Ok(())
        } else {
            Err(AkaError::DockerCommand(format!(
                "failed to run container '{name}' from image '{image}': {msg}"
            )))
        }
    }

    /// Idempotent container management:
    /// If the container is already running, do nothing.
    /// If it exists but is stopped, remove it first.
    /// Then run a fresh container.
    pub fn ensure_container(
        &self,
        image: &str,
        name: &str,
        ports: &[(u16, u16)],
        env_vars: &[(&str, &str)],
        volumes: &[&str],
        caps: &[&str],
    ) -> Result<()> {
        if self.is_running(name) {
            log::debug!("Container '{name}' is already running. Doing nothing.");
            return Ok(());
        }

        if self.container_exists(name) {
            log::debug!("Container '{name}' exists but is not running. Removing stale instance.");
            self.remove_container(name)?;
        }

        self.run_container(image, name, ports, env_vars, volumes, caps)
    }

    /// Get the IPv4 address of a running container
    pub fn get_container_ip(&self, name: &str) -> Result<Option<String>> {
        let result = docker_exec(&[
            "inspect",
            "--format",
            "{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}",
            name,
        ])?;

        if result.0 && !result.1.is_empty() {
            Ok(Some(result.1.trim().to_string()))
        } else {
            Ok(None)
        }
    }

    /// Pull a docker image
    pub fn pull_image(&self, image: &str) -> Result<()> {
        let (success, msg) = docker_exec(&["pull", image])?;
        if success {
            Ok(())
        } else {
            Err(AkaError::DockerCommand(format!("failed to pull image '{image}': {msg}")))
        }
    }

    /// Get logs for a container
    pub fn get_logs(&self, name: &str) -> Result<String> {
        let (_, msg) = docker_exec(&["logs", name])?;
        Ok(msg.to_string())
    }

    /// Attach to a container's output (runs interactively, does not return until detached)
    pub fn attach(&self, name: &str) -> Result<()> {
        let status = Command::new("docker")
            .args(["attach", name])
            .status()
            .map_err(|e| AkaError::DockerCommand(format!("failed to spawn docker attach: {e}")))?;

        if status.success() {
            Ok(())
        } else {
            Err(AkaError::DockerCommand(format!("docker attach for '{name}' exited with non-zero status")))
        }
    }
}

impl Default for DockerClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docker_client_new() {
        let client = DockerClient::new();
        assert_eq!(client.is_installed(), is_docker_installed());
    }

    #[test]
    fn container_status_defaults() {
        let status = ContainerStatus {
            running: false,
            exists: false,
        };
        assert!(!status.running);
        assert!(!status.exists);
    }

    #[test]
    fn run_container_args_include_caps() {
        // Verify caps are threaded through by checking the DockerClient API compiles
        // and accepts the caps parameter (functional test requires a live daemon).
        let client = DockerClient::new();
        // Calling with caps should be accepted by the type system; the actual docker
        // command will fail without a daemon, but we just verify the signature.
        let _ = client.run_container("img", "name", &[], &[], &[], &["NET_ADMIN"]);
    }
}
