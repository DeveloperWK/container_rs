mod cli;
mod error;
mod filesystem;
mod namespace;
mod process;

use cli::{parse_args, ContainerConfig};
use error::{ContainerError, ContainerResult};
use filesystem::FilesystemManager;
use log::{debug, error, info};
use namespace::{NamespaceConfig, NamespaceManager};
use nix::unistd::{getpid, Uid};
use process::ProcessManager;

fn main() {
    env_logger::Builder::from_default_env()
        .format_timestamp_micros()
        .format_module_path(false)
        .filter_level(log::LevelFilter::Info)
        .init();
    if let Err(e) = run() {
        error!("Container runtime error: {e}");
        std::process::exit(1)
    }
}

fn run() -> ContainerResult<()> {
    let config = parse_args();
    info!("Starting container runtime (PID: {})", getpid());
    debug!("Configuration: {config:?}");
    if !Uid::current().is_root() {
        error!("Root privileges required for container operations");
        return Err(ContainerError::RootRequired);
    }
    let ns_config = NamespaceConfig {
        isolate_pid: true,
        isolate_net: true,
        isolate_mount: true,
        isolate_uts: true,
        isolate_ipc: true,
        isolate_user: false,
    };
    NamespaceManager::unshare_namespaces(ns_config)?;
    let hostname = config.hostname.as_deref().unwrap_or("rust-container");
    NamespaceManager::set_hostname(&hostname);
    let rootfs_path = std::path::Path::new(&config.rootfs);
    FilesystemManager::setup_container_filesystem(&rootfs_path)?;
    info!("Container environment setup complete, executing command...");
    ProcessManager::execute_container_command(&config.command, &config.args)?;
    Ok(())
}
