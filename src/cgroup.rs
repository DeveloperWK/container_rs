use crate::error::{ContainerError, ContainerResult, Context};
use std::fs::{self, File, OpenOptions};
use std::io::ErrorKind;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

const CGROUP_ROOT: &str = "/sys/fs/cgroup";

#[derive(Debug, Clone)]

pub struct CgroupConfig {
    pub name: String,
    pub memory_limit: Option<u64>,
    pub memory_swap_limit: Option<u64>,
    pub cpu_weight: Option<u64>,
    pub cpu_quota: Option<u64>,
    pub cpu_period: Option<u64>,
    pub pids_limit: Option<u64>,
}
impl Default for CgroupConfig {
    fn default() -> Self {
        Self {
            name: format!("container-{}", std::process::id()),
            memory_limit: None,
            memory_swap_limit: None,
            cpu_weight: None,
            cpu_quota: None,
            cpu_period: Some(100000),
            pids_limit: None,
        }
    }
}
impl CgroupConfig {
    pub fn new(name: String) -> Self {
        Self {
            name,
            ..Default::default()
        }
    }
    pub fn with_memory_mb(mut self, mb: u64) -> Self {
        self.memory_limit = Some(mb * 1024 * 1024);
        self
    }
    pub fn with_cpu_percent(mut self, cpu_percent: u64) -> Self {
        let period = self.cpu_period.unwrap_or(100000);
        self.cpu_quota = Some((period * cpu_percent / 100) as u64);
        self
    }
    pub fn with_pids_limit(mut self, limit: u64) -> Self {
        self.pids_limit = Some(limit);
        self
    }
    pub fn with_cpu_weight(mut self, weight: u64) -> Self {
        self.cpu_weight = Some(weight);
        self
    }
}
#[derive(Debug)]
pub struct CgroupManager {
    cgroup_path: PathBuf,
    config: CgroupConfig,
    cgroup_version: CgroupVersion,
}
#[derive(Debug, Clone, Copy, PartialEq)]
enum CgroupVersion {
    V1,
    V2,
}

impl CgroupManager {
    pub fn new(config: CgroupConfig) -> ContainerResult<Self> {
        let cgroup_version = Self::detect_cgroup_version()?;
        log::info!("Detected cgroup version: {:?}", cgroup_version);
        let cgroup_path = match cgroup_version {
            CgroupVersion::V1 => PathBuf::from(CGROUP_ROOT),
            CgroupVersion::V2 => PathBuf::from(CGROUP_ROOT).join(&config.name),
        };

        Ok(Self {
            cgroup_path,
            config,
            cgroup_version,
        })
    }
    fn detect_cgroup_version() -> ContainerResult<CgroupVersion> {
        let cgroup_controllers = Path::new(CGROUP_ROOT).join("cgroup.controllers");
        if cgroup_controllers.exists() {
            log::debug!("Detected cgroup v2");
            Ok(CgroupVersion::V2)
        } else {
            log::debug!("Detected cgroup v1");
            Ok(CgroupVersion::V1)
        }
    }
    pub fn setup(&self) -> ContainerResult<()> {
        log::info!("Setting up cgroups for container: {}", self.config.name);
        match self.cgroup_version {
            CgroupVersion::V1 => self.setup_v1(),
            CgroupVersion::V2 => self.setup_v2(),
        };
        Ok(())
    }
    pub fn add_process(&self, pid: i32) -> ContainerResult<()> {
        log::info!("Adding process {} to cgroup", pid);
        match self.cgroup_version {
            CgroupVersion::V1 => self.add_process_v1(pid),
            CgroupVersion::V2 => self.add_process_v2(pid),
        };
        Ok(())
    }
    //pub fn cleanup(&self) -> ContainerResult<()> {
    //    log::info!("Cleaning up cgroup: {}", self.config.name);
    //    if self.cgroup_path.exists() {
    //        fs::remove_dir(&self.cgroup_path).map_err(|e| {
    //            log::warn!("Failed to remove cgroup directory: {}", e);
    //            ContainerError::Cgroup {
    //                message: format!("Failed to cleanup cgroup: {}", e),
    //            }
    //        })?;
    //        log::info!("Successfully cleaned up cgroup");
    //    } else {
    //        log::debug!("Cgroup directory doesn't exist, skipping cleanup");
    //    }
    //    Ok(())
    //}
    // fn cleanup(&self) -> ContainerResult<()> {
    //     if !self.cgroup_path.exists() {
    //         log::info!("Cgroup {:#?} already removed", self.cgroup_path);
    //         return Ok(());
    //     }
    //     let reclaim_path = self.cgroup_path.join("memory.reclaim");
    //     if reclaim_path.exists() {
    //         if let Err(e) = fs::write(&reclaim_path, b"1") {
    //             log::warn!(
    //                 "Failed to write memory.reclaim for {:#?}: {}",
    //                 self.cgroup_path,
    //                 e
    //             )
    //         } else {
    //             log::info!("Triggered memory reclaim for {:#?}", self.cgroup_path);
    //         }
    //     }
    //     if let Ok(entries) = fs::read_dir(&self.cgroup_path) {
    //         for entry in entries.flatten() {
    //             let path = entry.path();
    //             if path.is_dir() {
    //                 let child = CgroupManager {
    //                     cgroup_path: path,
    //                     cgroup_version: self.cgroup_version,
    //                     config: self.config.clone(),
    //                 };
    //                 let _ = child.cleanup();
    //             }
    //         }
    //     }
    //     let timeout = Duration::from_secs(2);
    //     let start = std::time::Instant::now();
    //     loop {
    //         let mem_current = fs::read_to_string(&self.cgroup_path.join("memory.current"))
    //             .ok()
    //             .and_then(|s| s.trim().parse::<u64>().ok())
    //             .unwrap_or(0);
    //         let kmem_usage =
    //             fs::read_to_string(&self.cgroup_path.join("memory.kmem.usage_in_bytes"))
    //                 .ok()
    //                 .and_then(|s| s.trim().parse::<u64>().ok())
    //                 .unwrap_or(0);
    //         if mem_current == 0 && kmem_usage == 0 {
    //             match fs::read_dir(&self.cgroup_path) {
    //                 Ok(_) => {
    //                     log::info!("Successfully removed cgroup: {:#?}", self.cgroup_path);
    //                     break;
    //                 }
    //                 Err(e) => {
    //                     if start.elapsed() > timeout {
    //                         log::error!(
    //                             "Failed to remove cgroup {:#?} after retries: {:#?}",
    //                             self.cgroup_path,
    //                             e
    //                         );
    //                         break;
    //                     }
    //                 }
    //             }
    //         }
    //         if start.elapsed() > timeout {
    //             log::warn!(
    //                 "Timeout reached waiting for memory to be released in {:#?}",
    //                 self.cgroup_path
    //             );
    //             break;
    //         }
    //         thread::sleep(Duration::from_millis(50));
    //     }
    //     Ok(())
    // }
    fn cleanup(&self) -> ContainerResult<()> {
        use std::{fs, thread, time::Duration};

        let path = &self.cgroup_path;

        // 1️⃣ Trigger memory reclaim if possible
        let reclaim_path = path.join("memory.reclaim");
        if reclaim_path.exists() {
            if let Err(e) = fs::write(&reclaim_path, b"1") {
                log::warn!("Failed to write memory.reclaim for {:?}: {}", path, e);
            } else {
                log::info!("Triggered memory reclaim for {:?}", path);
            }
        }

        // 2️⃣ Clean child cgroups first (recursive)
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                let sub = entry.path();
                if sub.is_dir() {
                    let child = CgroupManager {
                        cgroup_path: sub,
                        cgroup_version: self.cgroup_version,
                        config: self.config.clone(),
                    };
                    let _ = child.cleanup();
                }
            }
        }

        // 3️⃣ Wait for memory release before removing
        let timeout = Duration::from_secs(2);
        let start = std::time::Instant::now();

        loop {
            let mem_current = fs::read_to_string(path.join("memory.current"))
                .ok()
                .and_then(|s| s.trim().parse::<u64>().ok())
                .unwrap_or(0);
            let kmem_usage = fs::read_to_string(path.join("memory.kmem.usage_in_bytes"))
                .ok()
                .and_then(|s| s.trim().parse::<u64>().ok())
                .unwrap_or(0);

            if mem_current == 0 && kmem_usage == 0 {
                thread::sleep(Duration::from_secs(1)); // mimic your manual `sleep 1`
            }

            if start.elapsed() > timeout {
                log::warn!(
                    "Timeout waiting for memory release in {:?} (mem={}, kmem={})",
                    path,
                    mem_current,
                    kmem_usage
                );
                break;
            }

            // thread::sleep(Duration::from_millis(50));
        }
        match fs::remove_dir_all(path) {
            Ok(_) => log::info!("Removed cgroup {:?}", path),
            Err(e) if e.kind() == ErrorKind::NotFound => {
                log::info!("Cgroup {:?} already gone (ENOENT)", path)
            }
            Err(e) => log::warn!("Failed to remove cgroup {:?}: {}", path, e),
        }

        Ok(())
    }

    fn setup_v2(&self) -> ContainerResult<()> {
        fs::create_dir_all(&self.cgroup_path).map_err(|e| ContainerError::Cgroup {
            message: format!("Failed to create cgroup directory: {}", e),
        })?;
        log::debug!("Created cgroup directory: {:?}", self.cgroup_path);
        self.enable_controllers_v2()?;
        if let Some(memory_limit) = self.config.memory_limit {
            self.set_memory_limit(memory_limit)?;
        }
        log::info!("Cgroup v2 setup completed successfully");
        Ok(())
    }
    fn enable_controllers_v2(&self) -> ContainerResult<()> {
        let parent_subtree = Path::new(CGROUP_ROOT).join("cgroup.subtree_control");
        let controllers = vec!["cpu", "memory", "pids", "io"];
        for controller in controllers {
            let enable_cmd = format!("+{}", controller);
            if let Err(e) = self.write_file(&parent_subtree, &enable_cmd) {
                log::warn!(
                    "Failed to enable {} controller: {} (may already be enabled)",
                    controller,
                    e
                );
            } else {
                log::debug!("Enabled {} controller", controller);
            }
        }
        Ok(())
    }

    fn write_file(&self, path: &Path, content: &str) -> ContainerResult<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(path)
            .map_err(|e| ContainerError::Cgroup {
                message: format!("Failed to open {:?}: {}", path, e),
            })?;
        file.write_all(content.as_bytes())
            .map_err(|e| ContainerError::Cgroup {
                message: format!("Failed to write to {:?}: {}", path, e),
            })?;
        Ok(())
    }
    fn set_memory_limit(&self, limit: u64) -> ContainerResult<()> {
        let memory_max = self.cgroup_path.join("memory_max");
        self.write_file(&memory_max, &limit.to_string())?;
        log::info!(
            "Set memory limit: {} bytes ({} MB)",
            limit,
            limit / 1024 / 1024
        );
        Ok(())
    }
    fn add_process_v2(&self, pid: i32) -> ContainerResult<()> {
        let cgroup_process = self.cgroup_path.join("cgroup.procs");
        self.write_file(&cgroup_process, &pid.to_string())?;
        log::debug!("Added process {} to cgroup", pid);
        Ok(())
    }

    // ==================== Cgroup V1 Implementation ====================
    fn setup_v1(&self) -> ContainerResult<()> {
        Ok(())
    }
    fn setup_memory_v1(&self) -> ContainerResult<()> {
        Ok(())
    }
    fn add_process_v1(&self, pid: i32) -> ContainerResult<()> {
        Ok(())
    }
    fn read_file(&self, path: &Path) -> ContainerResult<String> {
        let mut file = File::open(path).map_err(|e| ContainerError::Cgroup {
            message: format!("Failed to open {:?}: {}", path, e),
        })?;

        let mut content = String::new();
        file.read_to_string(&mut content)
            .map_err(|e| ContainerError::Cgroup {
                message: format!("Failed to read {:?}: {}", path, e),
            })?;

        Ok(content)
    }
}

impl Drop for CgroupManager {
    fn drop(&mut self) {
        if let Err(e) = self.cleanup() {
            log::warn!(
                "Cgroup cleanup failed in Drop for {:#?}: {:#?}",
                self.cleanup(),
                e
            )
        }
    }
}
