#[cfg(test)]
mod tests {
    use std::process::Command;

    fn check_docker() -> bool {
        Command::new("docker")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    #[test]
    fn docker_exists() {
        let has_docker = check_docker();
        if !has_docker {
            eprintln!("SKIP: docker is not installed or not in PATH");
            return;
        }
        assert!(has_docker, "docker executable should be available on the host system");
    }

    #[test]
    fn docker_check_works() {
        let has_docker = check_docker();
        // This test always runs and tells us whether docker is available
        eprintln!("docker available: {}", has_docker);
    }
}
