mod cgroup;
mod cli;
mod error;
mod filesystem;
mod namespace;
mod process;


use cli::{ContainerConfig, parse_args};
use error::{ContainerError, ContainerResult};
use filesystem::FilesystemManager;
use log::{debug, error, info};
use namespace::{NamespaceConfig, NamespaceManager};
use nix::sys::signal;
use nix::{
    libc::{self, nice, signal},
    unistd::{Pid, Uid, getpid},
};
use process::ProcessManager;
// use signal_hook::iterator::Signals;

use cgroup::{CgroupConfig, CgroupManager};

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
    let cgroup_manager = if config.memory_limit_mb.is_some() || config.cpu_percent.is_some() || config.pids_limit.is_some()  {
        let mut cgroup_config = CgroupConfig::new(format!("container-{}", getpid()));
        if let Some(mem) = config.memory_limit_mb {
            cgroup_config = cgroup_config.with_memory_mb(mem);
            info!("Setting memory limit: {} MB", mem);
        }
        if let Some(cpu) =config.cpu_percent  {
            cgroup_config = cgroup_config.with_cpu_percent(cpu);
            log::info!("Setting CPU limit: {}%", cpu)
        }
        if let Some(pids) = config.pids_limit   {
            cgroup_config = cgroup_config.with_pids_limit(pids);
            log::info!("Setting PIDs limit: {}", pids)
        }
        let manager = CgroupManager::new(cgroup_config)?;
        manager.setup()?;
        manager.add_process(getpid().as_raw());
        Some(manager)
    } else {
        info!("No resource limits specified, skipping cgroup setup");
        None
    };
    NamespaceManager::unshare_namespaces(ns_config)?;
    NamespaceManager::enter_pid_namespace()?;
    info!("Running as PID 1 in container (host PID: {})", getpid());
    let hostname = config.hostname.as_deref().unwrap_or("rust-container");
    NamespaceManager::set_hostname(&hostname)?;
    let rootfs_path = std::path::Path::new(&config.rootfs);
    FilesystemManager::setup_container_filesystem(&rootfs_path)?;
    info!("Container environment setup complete, executing command...");

    ProcessManager::execute_container_command(&config.command, &config.args)?;
    // if let Some(ref manager) = cgroup_manager {
    //     info!("Cleaning up cgroups before exit...");
    //     // manager.cleanup().ok();
    //     if let Err(e) = manager.cleanup() {
    //         log::warn!("Failed to clean up cgroup: {:?}", e);
    //     }
    // }

    Ok(())
}
