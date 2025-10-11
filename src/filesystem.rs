use core::str;
use nix::mount::{MntFlags, MsFlags, mount, umount2};
use nix::unistd::{chdir, chroot, pivot_root};
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use crate::error::{ContainerError, ContainerResult, Context};
use crate::filesystem;

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
    pub fn setup_container_filesystem(rootfs_path: &Path) -> ContainerResult<()> {
        log::info!("Setting up container filesystem");
        Self::validate_rootfs(&rootfs_path)?;
        let absPath = fs::canonicalize(rootfs_path).map_err(|e| {
            ContainerError::filesystem_setup(format!("Failed to canonicalize path: {e}"))
        })?;
        log::debug!("Using absolute path: {absPath:?}");
        Self::pivot_root(&absPath)?; // Use absolute path
        Self::mount_proc(Path::new("/"))?;
        Self::mount_sysfs(Path::new("/"))?;
        Self::mount_devtmpfs(Path::new("/"))?;
        log::info!("Container filesystem setup completed");
        Ok(())
    }
    fn mount_proc(rootfs_path: &Path) -> ContainerResult<()> {
        let proc_path = rootfs_path.join("proc");
        if !proc_path.exists() {
            fs::create_dir_all(&proc_path).map_err(|e| ContainerError::Filesystem {
                message: format!("Failed to create /proc directory: {e}"),
            })?;
        }
        mount(
            Some("proc"),
            &proc_path,
            Some("proc"),
            MsFlags::empty(),
            None::<&str>,
        )
        .map_err(|e| ContainerError::Filesystem {
            message: format!("Failed to mount proc: {e}"),
        })
        .context("mounting proc filesystem")?;
        log::info!("Mounted proc filesystem");
        Ok(())
    }
    fn mount_sysfs(rootfs_path: &Path) -> ContainerResult<()> {
        let sys_path = rootfs_path.join("sys");
        if sys_path.exists() {
            if let Err(e) = mount(
                Some("sysfs"),
                &sys_path,
                Some("sysfs"),
                MsFlags::empty(),
                None::<&str>,
            ) {
                log::warn!("Failed to mount sysfs: {e}, continuing anyway")
            }
        }
        log::debug!("Mounted sysfs filesystem");
        Ok(())
    }
    fn mount_devtmpfs(rootfs_path: &Path) -> ContainerResult<()> {
        let dev_path = rootfs_path.join("dev");
        if !dev_path.exists() {
            return Ok(());
        }
        if let Err(e) = mount(
            Some("devtmpfs"),
            &dev_path,
            Some("devtmpfs"),
            MsFlags::empty(),
            None::<&str>,
        ) {
            log::warn!("Failed to mount devtmpfs: {e}, continuing anyway");
        }
        log::debug!("Mounted devtmpfs filesystem");
        Ok(())
    }
    // fn pivot_root(rootfs_path: &Path) -> ContainerResult<()> {
    //     log::info!("Pivoting root to: {rootfs_path:?}");
    //     mount(
    //         Some(rootfs_path),
    //         rootfs_path,
    //         None::<&str>,
    //         MsFlags::MS_BIND | MsFlags::MS_REC,
    //         None::<&str>,
    //     )
    //     .map_err(|e| ContainerError::filesystem_setup(format!("Failed to bind mount rootfs: {e}")))
    //     .context("bind mounting rootfs to itself")?;

    //     chdir(rootfs_path)
    //         .map_err(|e| ContainerError::Filesystem {
    //             message: format!("chdir to rootfs failed: {e}"),
    //         })
    //         .context("changing to rootfs directory")?;
    //     let put_old_name = "oldroot";
    //     fs::create_dir_all(put_old_name)
    //         .map_err(|e| ContainerError::filesystem_setup(format!("Failed to create put_old: {e}")))
    //         .context("pivoting root filesystem")?;
    //     pivot_root(".", put_old_name)
    //         .map_err(|e| ContainerError::Filesystem {
    //             message: format!("pivot_root failed: {e}"),
    //         })
    //         .context("pivoting root filesystem")?;
    //     chdir("/")
    //         .map_err(|e| ContainerError::filesystem_setup(format!("chdir to new root failed: {e}")))
    //         .context("changing to new root directory")?;
    //     Self::cleanup_old_root(Path::new("/oldroot"))?;
    //     log::debug!("Root pivot completed successfully");
    //     Ok(())
    // }
    fn pivot_root(rootfs_path: &Path) -> ContainerResult<()> {
        log::info!("Pivoting root to: {rootfs_path:?}");

        // Alternative: Remount with MS_SLAVE first, then MS_PRIVATE
        mount(
            None::<&str>,
            "/",
            None::<&str>,
            MsFlags::MS_SLAVE | MsFlags::MS_REC,
            None::<&str>,
        )
        .ok(); // Ignore errors, best effort

        mount(
            Some(rootfs_path),
            rootfs_path,
            None::<&str>,
            MsFlags::MS_BIND | MsFlags::MS_REC,
            None::<&str>,
        )
        .map_err(|e| {
            ContainerError::filesystem_setup(format!("Failed to bind mount rootfs: {e}"))
        })?;

        mount(
            None::<&str>,
            rootfs_path,
            None::<&str>,
            MsFlags::MS_PRIVATE | MsFlags::MS_REC,
            None::<&str>,
        )
        .map_err(|e| {
            ContainerError::filesystem_setup(format!("Failed to make mount private: {e}"))
        })?;

        // Change to the new root
        chdir(rootfs_path)
            .map_err(|e| ContainerError::Filesystem {
                message: format!("chdir to rootfs failed: {e}"),
            })
            .context("changing to rootfs directory")?;

        // Create the directory for the old root inside the new root
        let put_old_name = "oldroot";
        if !Path::new(put_old_name).exists() {
            fs::create_dir_all(put_old_name)
                .map_err(|e| {
                    ContainerError::filesystem_setup(format!("Failed to create put_old: {e}"))
                })
                .context("creating oldroot directory")?;
        }

        // Pivot root using "." for new_root since we're already in it
        pivot_root(".", put_old_name)
            .map_err(|e| ContainerError::Filesystem {
                message: format!("pivot_root failed: {e}"),
            })
            .context("pivoting root filesystem")?;

        // Change to the new root directory
        chdir("/")
            .map_err(|e| ContainerError::filesystem_setup(format!("chdir to new root failed: {e}")))
            .context("changing to new root directory")?;

        // Cleanup
        Self::cleanup_old_root(Path::new("/oldroot"))?;

        log::debug!("Root pivot completed successfully");
        Ok(())
    }
    fn cleanup_old_root(put_old: &Path) -> ContainerResult<()> {
        if let Err(e) = umount2("/oldroot", MntFlags::MNT_DETACH) {
            log::warn!("Failed to unmount old root: {e}, but continuing")
        }
        if let Err(e) = fs::remove_dir_all("/oldroot") {
            log::warn!("Failed to remove old root directory: {e}")
        }
        log::debug!("Old root cleanup completed");
        Ok(())
    }
}
