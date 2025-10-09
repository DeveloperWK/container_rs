use nix::mount::{mount, umount2, MntFlags, MsFlags};
use nix::unistd::{chdir, chroot, pivot_root};
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use crate::error::{ContainerError, ContainerResult, Context};

#[derive(Debug)]
pub struct FilesystemManager;
impl FilesystemManager {
    pub fn validate_rootfs(rootfs_path: &Path) -> ContainerResult<()> {
        log::info!("Validating rootfs at: {rootfs_path:?}");
        if !rootfs_path.exists() {
            return Err(ContainerError::Filesystem {
                message: format!("Rootfs path does not exist: {rootfs_path:?}"),
            });
        }
        if !rootfs_path.is_dir() {
            return Err(ContainerError::Filesystem {
                message: format!("Rootfs path is not a directory: {rootfs_path:?}"),
            });
        }
        let essential_dir = ["bin", "lib", "etc"];
        for dir in essential_dir {
            let dir_path = rootfs_path.join(dir);
            if !dir_path.exists() {
                log::warn!("Essential directory missing in rootfs: {dir}")
            }
        }
        log::debug!("Rootfs validation passed");
        Ok(())
    }
}
