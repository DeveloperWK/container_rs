use nix::sched::{unshare, CloneFlags};
use nix::unistd::{getpid, sethostname};

use crate::error::{ContainerError, ContainerResult, Context};
#[derive(Debug, Clone, Copy)]
pub struct NamespaceConfig {
    pub isolate_pid: bool,
    pub isolate_net: bool,
    pub isolate_mount: bool,
    pub isolate_uts: bool,
    pub isolate_ipc: bool,
    pub isolate_user: bool,
}
impl Default for NamespaceConfig {
    fn default() -> Self {
        Self {
            isolate_pid: true,
            isolate_net: true,
            isolate_mount: true,
            isolate_uts: true,
            isolate_ipc: true,
            isolate_user: false,
        }
    }
}
impl NamespaceConfig {
    pub fn to_clone_flags(&self) -> CloneFlags {
        let mut flags = CloneFlags::empty();
        if self.isolate_pid {
            flags |= CloneFlags::CLONE_NEWPID;
        }
        if self.isolate_net {
            flags |= CloneFlags::CLONE_NEWNET;
        }
        if self.isolate_mount {
            flags |= CloneFlags::CLONE_NEWNS;
        }
        if self.isolate_uts {
            flags |= CloneFlags::CLONE_NEWUTS;
        }
        if self.isolate_ipc {
            flags |= CloneFlags::CLONE_NEWIPC;
        }
        if self.isolate_user {
            flags |= CloneFlags::CLONE_NEWUSER;
        }
        flags
    }
}
#[derive(Debug)]
pub struct NamespaceManager;
impl NamespaceManager {
    pub fn unshare_namespaces(config: NamespaceConfig) -> ContainerResult<()> {
        log::info!("Unsharing namespaces with config: {config:?}");
        let flags = config.to_clone_flags();
        if flags.is_empty() {
            log::warn!("No namespaces specified for unshare");
            return Ok(());
        }
        unshare(flags)
            .map_err(|e| ContainerError::NamespaceSetup {
                message: format!("Failed to unshare namespaces: {e} (flags: {flags:?})"),
            })
            .context("unshare system call failed")?;
        log::info!("Successfully unshared namespaces: {flags:?}");
        Ok(())
    }
    pub fn set_hostname(hostname: &str) -> ContainerResult<()> {
        log::info!("Setting hostname to: {hostname}");
        sethostname(hostname)
            .map_err(|e| ContainerError::NamespaceSetup {
                message: format!("Failed to set hostname: {e}"),
            })
            .context("sethostname system call failed")?;
        log::debug!("Hostname set successfully");

        Ok(())
    }
    pub fn get_current_pid() -> i32 {
        getpid().as_raw()
    }
}
