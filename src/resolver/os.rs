/// Detect the current operating system
pub fn current_os() -> OsType {
    if cfg!(target_os = "macos") {
        OsType::Macos
    } else if cfg!(target_os = "linux") {
        OsType::Linux
    } else {
        OsType::Unknown
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsType {
    Macos,
    Linux,
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_os_returns_valid_type() {
        let os = current_os();
        assert!(matches!(os, OsType::Macos | OsType::Linux | OsType::Unknown));
    }
}
