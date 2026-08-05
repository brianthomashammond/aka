use thiserror::Error;

#[derive(Debug, Error)]
pub enum AkaError {
    #[error("failed to read config file '{0}': {1}")]
    ConfigRead(String, #[source] std::io::Error),

    #[error("failed to parse config file '{0}': {1}")]
    ConfigParse(String, #[source] serde_yml::Error),

    #[error("docker is not installed or not in PATH")]
    DockerNotFound,

    #[error("docker command failed: {0}")]
    DockerCommand(String),

    #[error("port {0} is already in use")]
    PortInUse(u16),

    #[error("failed to detect operating system: {0}")]
    OsDetection(String),

    #[error("resolv.conf write failed: {0}")]
    ResolvWrite(String),

    #[error("resolv.conf read failed: {0}")]
    ResolvRead(String),

    #[error("permission denied: {0}")]
    PermissionDenied(String),

    #[error("service '{0}' is not valid. Must be one of: {1}")]
    InvalidService(String, String),

    #[error("container '{0}' not found")]
    ContainerNotFound(String),
}

pub type Result<T> = std::result::Result<T, AkaError>;
