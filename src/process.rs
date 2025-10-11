use crate::error::{ContainerError, ContainerResult, Context};
use nix::unistd::{execve, Pid};
use std::{env::args, ffi::CString, path::Path};
#[derive(Debug)]
pub struct ProcessManager;
impl ProcessManager {
    pub fn execute_container_command(command: &str, args: &[String]) -> ContainerResult<()> {
        log::info!("Executing container command: {command} with args: {args:?}");
        let command_path = if command.starts_with("/") {
            command.to_string()
        } else {
            let possiable_path = vec![
                format!("/bin/{}", command),
                format!("/usr/bin/{}", command),
                format!("/sbin/{}", command),
                format!("/usr/sbin/{}", command),
            ];
            possiable_path
                .into_iter()
                .find(|p| Path::new(p).exists())
                .unwrap_or_else(|| format!("/bin/{}", command))
        };
        if !Path::new(&command_path).exists() {
            return Err(ContainerError::process_execution(format!(
                "Command not found in container: {} (tried: {})",
                command, command_path
            )));
        }
        let argv = Self::build_argv(&command_path, args)?;
        let envp = Self::build_environment()?;
        log::debug!("Executing: {command_path} with argv: {argv:?}");
        execve(&argv[0], &argv, &envp)
            .map_err(|e| {
                ContainerError::process_execution(format!("execve failed for {command}: {e}"))
            })
            .context("executing container command")?;
        unreachable!("execve should not return")
    }
    pub fn build_argv(command_path: &str, args: &[String]) -> ContainerResult<Vec<CString>> {
        let mut argv = Vec::with_capacity(args.len() + 1);
        argv.push(CString::new(command_path).map_err(|e| {
            ContainerError::process_execution(format!(
                "Invalid command path (contains null byte): {}",
                e
            ))
        })?);
        for arg in args {
            argv.push(CString::new(arg.as_str()).map_err(|e| {
                ContainerError::process_execution(format!(
                    "Invalid argument (contains null byte): {}",
                    e
                ))
            })?)
        }
        Ok(argv)
    }
    pub fn build_environment() -> ContainerResult<Vec<CString>> {
        let env_vers = vec![
            "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
            "TERM=xterm",
            "HOME=/root",
            "HOSTNAME=rust-container",
            "container=rust-container-runtime",
        ];
        let mut env = Vec::with_capacity(env_vers.len());
        for var in env_vers {
            env.push(CString::new(var).map_err(|e| {
                ContainerError::process_execution(format!("Invalid environment variable: {}", e))
            })?);
        }

        Ok(env)
    }
    pub fn get_current_pid() -> Pid {
        nix::unistd::getpid()
    }
}
