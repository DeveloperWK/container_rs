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
            format!("/bin/{command}")
        };
        if !Path::new(&command_path).exists() {
            return Err(ContainerError::process_execution(format!(
                "command not found: {command}"
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
        unreachable!()
    }
    pub fn build_argv(command_path: &str, args: &[String]) -> ContainerResult<Vec<CString>> {
        let mut argv = Vec::with_capacity(args.len() + 1);
        argv.push(CString::new(command_path)?);
        for arg in args {
            argv.push(CString::new(arg.as_str())?)
        }
        Ok(argv)
    }
    pub fn build_environment() -> ContainerResult<Vec<CString>> {
        let env = vec![
            CString::new("PATH=/bin:/usr/bin")?,
            CString::new("TERM=xterm")?,
            CString::new("container=rust-container-runtime")?,
        ];
        Ok(env)
    }
    pub fn get_current_pid() -> Pid {
        nix::unistd::getpid()
    }
}
