use crate::error::{ContainerError, ContainerResult};
use nix::unistd::Pid;
use std::fs::{self, File, OpenOptions};
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
    pub pids_limit: Option<i64>,
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
    pub fn with_pids_limit(mut self, limit: i64) -> Self {
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
            CgroupVersion::V1 => self.setup_v1()?,
            CgroupVersion::V2 => self.setup_v2()?,
        };
        Ok(())
    }
    pub fn add_process(&self, pid: i32) -> ContainerResult<()> {
        log::info!("Adding process {} to cgroup", pid);
        match self.cgroup_version {
            CgroupVersion::V1 => self.add_process_v1(pid)?,
            CgroupVersion::V2 => self.add_process_v2(pid)?,
        };
        Ok(())
    }

    pub fn cleanup(&self) -> ContainerResult<()> {
        let path = &self.cgroup_path;
        if path.exists() {
            log::info!("remove cgroup {:?}", path);
            let kill_file = path.join("cgroup.kill");
            if kill_file.exists() {
                self.write_file(&kill_file, "1")?;
            } else {
                let procs_path = path.join("cgroup.procs");
                let procs = self.read_file(&procs_path)?;
                for line in procs.lines() {
                    let pid: i32 = line
                        .parse()
                        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
                    let _ = nix::sys::signal::kill(Pid::from_raw(pid), nix::sys::signal::SIGKILL)?;
                }
            }
            self.delete_with_retry(path, 5, Duration::from_millis(100))?;
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
            self.set_memory_limit_v2(memory_limit)?;
        };
        if let Some(swap_limit) = self.config.memory_swap_limit {
            self.set_memory_swap_v2(swap_limit)?;
        };
        if let Some(cpu_weight) = self.config.cpu_weight {
            self.set_cpu_weight_v2(cpu_weight)?;
        };
        if let Some(cpu_quota) = self.config.cpu_quota {
            if let Some(cpu_period) = self.config.cpu_period {
                self.set_cpu_max_v2(cpu_quota, cpu_period)?;
            }
        };
        if let Some(pids_limit) = self.config.pids_limit {
            self.set_pids_limit_v2(pids_limit)?;
        };

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
    fn delete_with_retry<P: AsRef<Path>, L: Into<Option<Duration>>>(
        &self,
        path: P,
        retries: u32,
        limit_backoff: L,
    ) -> ContainerResult<()> {
        let mut attemps = 0;
        let mut delay = Duration::from_millis(10);
        let path = path.as_ref();
        let limit = limit_backoff.into().unwrap_or(Duration::MAX);
        while attemps < retries {
          if fs::remove_dir(path).is_ok() {
            return Ok(());
        }

            thread::sleep(delay);
            attemps += 1;
            delay *= attemps;
            if delay > limit {
                delay = limit;
            }
        }

let err = std::io::Error::new(std::io::ErrorKind::TimedOut, "could not delete".to_string());
log::error!("Failed to delete {:?}: {:?}", path, err);
return Err(err.into());
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
    fn set_memory_limit_v2(&self, limit: u64) -> ContainerResult<()> {
        let memory_max = self.cgroup_path.join("memory.max");
        let memory_swap_limit = self.cgroup_path.join("memory.swap.max");
        self.write_file(&memory_max, &limit.to_string())?;
        self.write_file(&memory_swap_limit, "0")?;
        log::info!(
            "Set memory limit: {} bytes ({} MB)",
            limit,
            limit / 1024 / 1024
        );
           log::info!(
            "Set memory_swap limit path: {:?}",
            memory_swap_limit
        );
        Ok(())
    }
    fn set_memory_swap_v2(&self, limit: u64) -> ContainerResult<()> {
        let swap_max = self.cgroup_path.join("memory.swap.max");
        let _ = self.write_file(&swap_max, &limit.to_string())?;
        log::info!("Set swap limit: {} bytes", limit);
        Ok(())
    }
    fn set_cpu_weight_v2(&self, weight: u64) -> ContainerResult<()> {
        let cpu_weight = self.cgroup_path.join("cpu.weight");
        let _ = self.write_file(&cpu_weight, &weight.to_string())?;
        log::info!("Set CPU weight: {}", weight);
        Ok(())
    }
    fn set_cpu_max_v2(&self, quota: u64, period: u64) -> ContainerResult<()> {
        let cpu_max = self.cgroup_path.join("cpu.max");
        let value = if quota == u64::MAX {
            "max".to_string()
        } else {
            format!("{} {}", quota, period)
        };
        let _ = self.write_file(&cpu_max, &value)?;
        log::info!(
            "Set CPU quota: {} us / {} us ({:.1}%)",
            quota,
            period,
            (quota as f64 / period as f64) * 100.0
        );
        Ok(())
    }
    fn set_pids_limit_v2(&self, limit: i64) -> ContainerResult<()> {
        let pids_max = self.cgroup_path.join("pids.max");
        let value = if limit == i64::MAX {
            "max".to_string()
        } else {
            limit.to_string()
        };
        let _ = self.write_file(&pids_max, &value);
        log::info!("Set PIDs limit: {}", value);
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
      todo!()
    }
    fn setup_memory_v1(&self) -> ContainerResult<()> {
        todo!()
    }
    fn add_process_v1(&self, pid: i32) -> ContainerResult<()> {
        todo!()
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
        println!("Dropping cgroup {:?}", self.cgroup_path);
        if let Err(e) = self.cleanup() {
            log::warn!("Cgroup cleanup failed in Drop: {:?}", e);
        }
    }
}
