mod config;
mod docker;
mod error;
mod port_utils;
mod resolver;
mod services;

use clap::{Parser, Subcommand};
use error::AkaError;

/// Aka - Rust reimplementation of Dory (local DNS + Nginx reverse proxy for Docker)
#[derive(Parser, Debug)]
#[command(name = "aka", version, about)]
pub struct Cli {
    #[arg(short, long, help = "Enable verbose output")]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Bring up aka services (nginx-proxy, dnsmasq, resolv)
    Up {
        #[arg(help = "Optional service names to start: proxy, dns, resolv")]
        services: Vec<String>,
    },

    /// Stop all aka services
    #[command(alias = "stop")]
    Down {
        #[arg(short, long, default_value_t = true, help = "Destroy containers after stopping")]
        destroy: bool,
        #[arg(help = "Optional service names to stop: proxy, dns, resolv")]
        services: Vec<String>,
    },

    /// Stop and restart all aka services
    Restart {
        #[arg(short, long, default_value_t = true, help = "Destroy containers after stopping")]
        destroy: bool,
    },

    /// Report status of the aka services
    Status,

    /// Write a default config file
    ConfigFile {
        #[arg(short, long, help = "Upgrade existing config file")]
        upgrade: bool,
        #[arg(short, long, help = "Overwrite existing config file")]
        force: bool,
    },

    /// Attach to the output of a docker service container
    Attach {
        #[arg(help = "Service to attach to: proxy, dns")]
        service: Option<String>,
    },

    /// Print the logs of a docker service container
    Logs {
        #[arg(help = "Service to get logs for: proxy, dns")]
        service: Option<String>,
    },

    /// Pull down the docker images that aka uses
    Pull {
        #[arg(help = "Optional service names to pull: proxy, dns")]
        services: Vec<String>,
    },

    /// Grab the IPv4 address of a running aka service
    Ip {
        #[arg(help = "Service to get IP for: proxy, dns")]
        service: Option<String>,
    },

    /// Upgrade aka to the latest version
    Upgrade,
}

fn setup_logging(verbose: bool) {
    if verbose {
        env_logger::builder()
            .filter_level(log::LevelFilter::Debug)
            .init();
    } else {
        env_logger::init();
    }
}

/// Validate that every requested service name is recognized. Returns the first
/// invalid name as an error rather than silently ignoring it.
fn validate_services(services: &[String], valid: &[&str]) -> error::Result<()> {
    for s in services {
        if !valid.contains(&s.as_str()) {
            return Err(AkaError::InvalidService(s.clone(), valid.join(", ")));
        }
    }
    Ok(())
}

fn get_images_for_services(services: &[String], cfg: &config::DoryConfig) -> Vec<(String, String)> {
    let mut images = Vec::new();

    for svc in services {
        match svc.as_str() {
            "proxy" => {
                if cfg.nginx_proxy.enabled {
                    images.push((
                        "nginx-proxy".into(),
                        services::proxy::ProxyService::new().image_name(cfg),
                    ));
                }
            }
            "dns" => {
                if cfg.dnsmasq.enabled {
                    images.push((
                        "dnsmasq".into(),
                        services::dnsmasq::DnsmasqService::new().image_name(),
                    ));
                }
            }
            _ => {}
        }
    }

    if services.is_empty() {
        if cfg.nginx_proxy.enabled {
            images.push((
                "nginx-proxy".into(),
                services::proxy::ProxyService::new().image_name(cfg),
            ));
        }
        if cfg.dnsmasq.enabled {
            images.push((
                "dnsmasq".into(),
                services::dnsmasq::DnsmasqService::new().image_name(),
            ));
        }
    }

    images
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let starting_dir = std::env::current_dir()?;

    // Best-effort: a malformed config shouldn't prevent startup (e.g. `aka config-file
    // --force` needs to work even when the existing config is broken), so config-driven
    // debug logging just falls back to `--verbose` alone if loading fails here.
    let debug_from_config = config::load_config(&starting_dir)
        .map(|cfg| cfg.aka.debug)
        .unwrap_or(false);
    setup_logging(cli.verbose || debug_from_config);
    log::info!("aka starting: {:?}", cli.command);

    let docker = docker::DockerClient::new();

    match cli.command {
        Command::Up { services } => {
            validate_services(&services, &["dns", "proxy", "resolv"])?;
            let cfg = config::load_config(&starting_dir)?;
            log::info!("config loaded: dnsmasq.enabled={}, nginx_proxy.enabled={}, resolv.enabled={}",
                cfg.aka.dnsmasq.enabled, cfg.aka.nginx_proxy.enabled, cfg.aka.resolv.enabled);

            if !docker.is_installed() {
                log::error!("docker is not installed or not in PATH");
                return Err(AkaError::DockerNotFound.into());
            }
            log::info!("docker is available");

            let mut started_services = Vec::new();

            if services.is_empty() || services.iter().any(|s| s == "dns") {
                if cfg.aka.dnsmasq.enabled {
                    let dnsmasq = services::dnsmasq::DnsmasqService::new();
                    match dnsmasq.start_with_conflict_resolution(&cfg.aka) {
                        Ok(running) => {
                            if running {
                                println!("dnsmasq started");
                                started_services.push("dns");
                            } else {
                                eprintln!("dnsmasq failed to start");
                            }
                        }
                        Err(e) => {
                            eprintln!("failed to start dnsmasq: {}", e);
                        }
                    }
                } else {
                    log::info!("dnsmasq is disabled in config");
                }
            }

            if services.is_empty() || services.iter().any(|s| s == "proxy") {
                if cfg.aka.nginx_proxy.enabled {
                    let proxy = services::proxy::ProxyService::new();
                    match proxy.ensure_running(&cfg.aka) {
                        Ok(running) => {
                            if running {
                                println!("nginx-proxy started");
                                started_services.push("proxy");
                            } else {
                                eprintln!("nginx-proxy failed to start");
                            }
                        }
                        Err(e) => {
                            eprintln!("failed to start nginx-proxy: {}", e);
                        }
                    }
                } else {
                    log::info!("nginx proxy is disabled in config");
                }
            }

            if !started_services.is_empty() {
                log::info!("started services: {:?}", started_services);
            }

            // Configure system resolver
            if (services.is_empty() || services.iter().any(|s| s == "resolv")) && cfg.aka.resolv.enabled {
                match resolver::configure(&cfg.aka) {
                    Ok(_) => {
                        println!("resolver configured");
                        if !started_services.contains(&"resolv") {
                            started_services.push("resolv");
                        }
                    }
                    Err(e) => {
                        eprintln!("failed to configure resolver: {}", e);
                    }
                }
            }
        }

        Command::Down { destroy, services } => {
            validate_services(&services, &["dns", "proxy", "resolv"])?;
            let cfg = config::load_config(&starting_dir)?;
            log::info!("config loaded: destroy={}, dnsmasq.enabled={}, nginx_proxy.enabled={}, resolv.enabled={}",
                destroy, cfg.aka.dnsmasq.enabled, cfg.aka.nginx_proxy.enabled, cfg.aka.resolv.enabled);

            if !docker.is_installed() {
                log::error!("docker is not installed or not in PATH");
                return Err(AkaError::DockerNotFound.into());
            }

            let mut stopped_services = Vec::new();

            if (services.is_empty() || services.iter().any(|s| s == "dns")) && cfg.aka.dnsmasq.enabled {
                let dnsmasq = services::dnsmasq::DnsmasqService::new();
                if destroy {
                    if let Err(e) = docker.remove_container(&cfg.aka.dnsmasq.container_name) {
                        log::error!("failed to remove dnsmasq container: {}", e);
                    } else {
                        log::info!("dnsmasq container removed");
                        stopped_services.push("dns");
                    }
                } else if let Err(e) = dnsmasq.stop(&cfg.aka) {
                    log::error!("failed to stop dnsmasq: {}", e);
                } else if docker.container_exists(&cfg.aka.dnsmasq.container_name) {
                    log::info!("dnsmasq container stopped");
                    stopped_services.push("dns");
                }
            }

            if (services.is_empty() || services.iter().any(|s| s == "proxy")) && cfg.aka.nginx_proxy.enabled {
                let proxy = services::proxy::ProxyService::new();
                if destroy {
                    if let Err(e) = docker.remove_container(&cfg.aka.nginx_proxy.container_name) {
                        log::error!("failed to remove nginx proxy container: {}", e);
                    } else {
                        log::info!("nginx proxy container removed");
                        stopped_services.push("proxy");
                    }
                } else if let Err(e) = proxy.stop(&cfg.aka) {
                    log::error!("failed to stop nginx proxy: {}", e);
                } else if docker.container_exists(&cfg.aka.nginx_proxy.container_name) {
                    log::info!("nginx proxy container stopped");
                    stopped_services.push("proxy");
                }
            }

            if !stopped_services.is_empty() {
                log::info!("stopped services: {:?}", stopped_services);
            }

            // Clean up system resolver
            if (services.is_empty() || services.iter().any(|s| s == "resolv")) && cfg.aka.resolv.enabled {
                match resolver::clean(&cfg.aka) {
                    Ok(_) => {
                        log::info!("system resolver cleaned");
                        if !stopped_services.contains(&"resolv") {
                            stopped_services.push("resolv");
                        }
                    }
                    Err(e) => {
                        log::error!("failed to clean system resolver: {}", e);
                    }
                }
            }

            if !stopped_services.is_empty() {
                log::info!("stopped services: {:?}", stopped_services);
            }
        }

        Command::Restart { destroy } => {
            let cfg = config::load_config(&starting_dir)?;
            log::info!("config loaded: destroy={}, dnsmasq.enabled={}, nginx_proxy.enabled={}, resolv.enabled={}",
                destroy, cfg.aka.dnsmasq.enabled, cfg.aka.nginx_proxy.enabled, cfg.aka.resolv.enabled);

            if !docker.is_installed() {
                log::error!("docker is not installed or not in PATH");
                return Err(AkaError::DockerNotFound.into());
            }

            log::info!("stopping services...");

            if cfg.aka.dnsmasq.enabled {
                if destroy {
                    let _ = docker.remove_container(&cfg.aka.dnsmasq.container_name);
                } else {
                    let _ = docker.stop_container(&cfg.aka.dnsmasq.container_name);
                }
            }

            if cfg.aka.nginx_proxy.enabled {
                if destroy {
                    let _ = docker.remove_container(&cfg.aka.nginx_proxy.container_name);
                } else {
                    let _ = docker.stop_container(&cfg.aka.nginx_proxy.container_name);
                }
            }

            log::info!("services stopped, now starting...");

            // Clean resolver first
            if cfg.aka.resolv.enabled {
                let _ = resolver::clean(&cfg.aka);
            }

            let cfg = config::load_config(&starting_dir)?;
            let mut started_services = Vec::new();

            if cfg.aka.dnsmasq.enabled {
                let dnsmasq = services::dnsmasq::DnsmasqService::new();
                if let Ok(running) = dnsmasq.start_with_conflict_resolution(&cfg.aka)
                    && running
                {
                    log::info!("dnsmasq container restarted successfully");
                    started_services.push("dns");
                }
            }

            if cfg.aka.nginx_proxy.enabled {
                let proxy = services::proxy::ProxyService::new();
                if let Ok(running) = proxy.ensure_running(&cfg.aka)
                    && running
                {
                    log::info!("nginx proxy container restarted successfully");
                    started_services.push("proxy");
                }
            }

            // Reconfigure resolver
            if cfg.aka.resolv.enabled {
                match resolver::configure(&cfg.aka) {
                    Ok(_) => {
                        log::info!("system resolver reconfigured");
                        if !started_services.contains(&"resolv") {
                            started_services.push("resolv");
                        }
                    }
                    Err(e) => {
                        log::error!("failed to reconfigure system resolver: {}", e);
                    }
                }
            }

            if !started_services.is_empty() {
                log::info!("restarted services: {:?}", started_services);
            }
        }

        Command::Status => {
            let cfg = config::load_config(&starting_dir)?;
            let proxy_status = docker.get_container_status(&cfg.aka.nginx_proxy.container_name);
            let dns_status = docker.get_container_status(&cfg.aka.dnsmasq.container_name);
            println!("dnsmasq   ({}): exists={}, running={}",
                cfg.aka.dnsmasq.container_name, dns_status.exists, dns_status.running);
            println!("nginx-proxy ({}): exists={}, running={}",
                cfg.aka.nginx_proxy.container_name, proxy_status.exists, proxy_status.running);
            println!("resolver   ({}): configured={}",
                resolver::resolv_file(), resolver::has_our_nameserver(&cfg.aka));

            if cli.verbose {
                match resolver::os::current_os() {
                    resolver::os::OsType::Linux => {
                        if let Ok(contents) = resolver::resolv_file_contents() {
                            println!("--- {} ---\n{}", resolver::resolv_file(), contents);
                        }
                    }
                    resolver::os::OsType::Macos => {
                        for filename in resolver::macos::resolv_files(resolver::macos::RESOLVER_DIR, &cfg.aka) {
                            if let Ok(contents) = resolver::macos::resolv_file_contents(&filename) {
                                println!("--- {} ---\n{}", filename, contents);
                            }
                        }
                    }
                    resolver::os::OsType::Unknown => {}
                }
            }
        }

        Command::ConfigFile { upgrade, force } => {
            let home_path = config::home_config_path();
            if upgrade {
                if home_path.exists() {
                    log::info!("upgrading existing config file: {}", home_path.display());
                    let current_yaml = std::fs::read_to_string(&home_path)
                        .map_err(|e| anyhow::anyhow!("failed to read config: {}", e))?;
                    let current_config: config::Config = serde_yml::from_str(&current_yaml)
                        .map_err(|e| anyhow::anyhow!("failed to parse config: {}", e))?;

                    let upgraded = config::upgrade_config(&current_config);
                    let yaml = serde_yml::to_string(&upgraded)
                        .map_err(|e| anyhow::anyhow!("failed to serialize config: {}", e))?;
                    std::fs::write(&home_path, yaml)
                        .map_err(|e| anyhow::anyhow!("failed to write config: {}", e))?;
                    log::info!("config upgraded successfully");
                } else {
                    log::info!("no existing config to upgrade, writing default");
                    let path = config::write_default_home_config(true)?;
                    log::info!("default config written to: {}", path.display());
                }
            } else {
                let path = config::write_default_home_config(force)?;
                log::info!("default config written to: {}", path.display());
            }
        }

        Command::Attach { service } => {
            let cfg = config::load_config(&starting_dir)?;
            log::info!("config loaded: dnsmasq.enabled={}, nginx_proxy.enabled={}",
                cfg.aka.dnsmasq.enabled, cfg.aka.nginx_proxy.enabled);

            if !docker.is_installed() {
                log::error!("docker is not installed or not in PATH");
                return Err(AkaError::DockerNotFound.into());
            }

            let container_name = match service.as_deref() {
                Some("proxy") => cfg.aka.nginx_proxy.container_name.clone(),
                Some("dns") => cfg.aka.dnsmasq.container_name.clone(),
                Some(s) => {
                    return Err(AkaError::InvalidService(s.to_string(), "proxy, dns".to_string()).into());
                }
                None => {
                    log::info!("no service specified, attaching to proxy");
                    cfg.aka.nginx_proxy.container_name.clone()
                }
            };

            if !docker.container_exists(&container_name) {
                log::error!("container '{}' not found", container_name);
                return Err(AkaError::ContainerNotFound(container_name).into());
            }

            log::info!("attaching to container '{}'", container_name);
            docker.attach(&container_name)?;
        }

        Command::Logs { service } => {
            let cfg = config::load_config(&starting_dir)?;
            log::info!("config loaded: dnsmasq.enabled={}, nginx_proxy.enabled={}",
                cfg.aka.dnsmasq.enabled, cfg.aka.nginx_proxy.enabled);

            if !docker.is_installed() {
                log::error!("docker is not installed or not in PATH");
                return Err(AkaError::DockerNotFound.into());
            }

            let container_name = match service.as_deref() {
                Some("proxy") => cfg.aka.nginx_proxy.container_name.clone(),
                Some("dns") => cfg.aka.dnsmasq.container_name.clone(),
                Some(s) => {
                    return Err(AkaError::InvalidService(s.to_string(), "proxy, dns".to_string()).into());
                }
                None => {
                    log::info!("no service specified, getting proxy logs");
                    cfg.aka.nginx_proxy.container_name.clone()
                }
            };

            if !docker.container_exists(&container_name) {
                log::error!("container '{}' not found", container_name);
                return Err(AkaError::ContainerNotFound(container_name).into());
            }

            match docker.get_logs(&container_name) {
                Ok(logs) => println!("{}", logs),
                Err(e) => eprintln!("failed to get logs for '{}': {}", container_name, e),
            }
        }

        Command::Pull { services } => {
            validate_services(&services, &["dns", "proxy"])?;
            let cfg = config::load_config(&starting_dir)?;
            log::info!("config loaded: dnsmasq.enabled={}, nginx_proxy.enabled={}",
                cfg.aka.dnsmasq.enabled, cfg.aka.nginx_proxy.enabled);

            if !docker.is_installed() {
                log::error!("docker is not installed or not in PATH");
                return Err(AkaError::DockerNotFound.into());
            }

            let images = get_images_for_services(&services, &cfg.aka);

            for (name, image) in &images {
                log::info!("pulling image '{}' for service '{}'", image, name);
                match docker.pull_image(image) {
                    Ok(_) => log::info!("successfully pulled '{}'", name),
                    Err(e) => log::error!("failed to pull '{}': {}", name, e),
                }
            }

            if !services.is_empty() {
                log::info!("pulled images for services: {:?}", images.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>());
            } else {
                log::info!("pulled all service images");
            }
        }

        Command::Ip { service } => {
            let cfg = config::load_config(&starting_dir)?;
            log::info!("config loaded: dnsmasq.enabled={}, nginx_proxy.enabled={}",
                cfg.aka.dnsmasq.enabled, cfg.aka.nginx_proxy.enabled);

            let container_name = match service.as_deref() {
                Some("proxy") => cfg.aka.nginx_proxy.container_name.clone(),
                Some("dns") => cfg.aka.dnsmasq.container_name.clone(),
                Some(s) => {
                    return Err(AkaError::InvalidService(s.to_string(), "proxy, dns".to_string()).into());
                }
                None => {
                    log::info!("no service specified, getting proxy IP");
                    cfg.aka.nginx_proxy.container_name.clone()
                }
            };

            match docker.get_container_ip(&container_name) {
                Ok(Some(ip)) => println!("{}", ip),
                Ok(None) => eprintln!("container '{}' has no IP (not running?)", container_name),
                Err(e) => eprintln!("failed to get IP for '{}': {}", container_name, e),
            }
        }

        Command::Upgrade => {
            let current_version = env!("CARGO_PKG_VERSION");
            println!("current version: {}", current_version);

            let url = "https://api.github.com/repos/anomalyco/aka/releases/latest";
            match reqwest::blocking::Client::new()
                .get(url)
                .header("User-Agent", "aka-upgrade-check")
                .send()
            {
                Ok(response) if response.status().is_success() => {
                    match response.json::<serde_json::Value>() {
                        Ok(json) => {
                            if let Some(tag) = json["tag_name"].as_str() {
                                if tag != current_version {
                                    println!("new version available: {}", tag);
                                    println!("run 'cargo install aka' to update");
                                } else {
                                    println!("already running the latest version");
                                }
                            } else {
                                eprintln!("could not parse version from GitHub response");
                            }
                        }
                        Err(e) => eprintln!("could not parse GitHub response: {}", e),
                    }
                }
                Ok(response) => {
                    eprintln!("could not check for updates (GitHub API returned {})", response.status());
                    println!("run 'cargo install aka' to update manually");
                }
                Err(e) => {
                    eprintln!("could not check for updates: {}", e);
                    println!("run 'cargo install aka' to update manually");
                }
            }
        }
    }

    Ok(())
}
