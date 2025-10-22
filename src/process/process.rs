// use crate::error::{ContainerError, ContainerResult, Context};
// use nix::libc;
// use nix::mount::{MsFlags, mount};
// use nix::pty::openpty;
// use nix::sys::signal::{SigHandler, Signal, kill, signal};
// use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};
// use nix::unistd::{ForkResult, Pid, dup2, execve, fork, pipe, setsid};
// use std::ffi::CString;
// use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
// use std::path::Path;
// use std::sync::atomic::{AtomicI32, Ordering};

// static CHILD_PID: AtomicI32 = AtomicI32::new(0);

// extern "C" fn handle_signal(sig: i32) {
//     let child = CHILD_PID.load(Ordering::SeqCst);
//     if child > 0 {
//         if let Ok(signal) = Signal::try_from(sig) {
//             let _ = kill(Pid::from_raw(child), signal);
//         }
//     }
// }

// #[derive(Debug)]
// pub struct ProcessManager;

// impl ProcessManager {
//     pub fn execute_container_command(command: &str, args: &[String]) -> ContainerResult<()> {
//         log::info!("Executing container command: {command} with args: {args:?}");
//         Self::ensure_devpts_mounted()?;
//         // Find executable path
//         let command_path = if command.starts_with("/") {
//             command.to_string()
//         } else {
//             ["/bin", "/usr/bin", "/sbin", "/usr/sbin"]
//                 .iter()
//                 .map(|prefix| format!("{}/{}", prefix, command))
//                 .find(|p| Path::new(p).exists())
//                 .unwrap_or_else(|| format!("/bin/{}", command))
//         };

//         if !Path::new(&command_path).exists() {
//             return Err(ContainerError::process_execution(format!(
//                 "Command not found in container: {}",
//                 command_path
//             )));
//         }

//         let argv = Self::build_argv(&command_path, args)?;
//         let envp = Self::build_environment()?;

//         // Try to create pseudo-terminal, fall back to direct execution if not available
//         let use_pty = openpty(None, None).is_ok();

//         if use_pty {
//             Self::execute_with_pty(command, &argv, &envp)
//         } else {
//             log::warn!("PTY not available (ENODEV), running without PTY support");
//             Self::execute_without_pty(command, &argv, &envp)
//         }
//     }
//     // fn ensure_devpts_mounted() -> ContainerResult<()> {
//     //     // Check if /dev/pts exists
//     //     let dev_pts = Path::new("/dev/pts");
//     //     if !dev_pts.exists() {
//     //         log::info!("Creating /dev/pts directory");
//     //         std::fs::create_dir_all(dev_pts).ok();
//     //     }

//     //     // Try to mount devpts if not already mounted
//     //     // We ignore errors here since it might already be mounted
//     //     let result = mount(
//     //         Some("devpts"),
//     //         "/dev/pts",
//     //         Some("devpts"),
//     //         MsFlags::empty(),
//     //         Some("newinstance,ptmxmode=0666,mode=0620"),
//     //     );

//     //     match result {
//     //         Ok(_) => {
//     //             log::info!("devpts filesystem mounted at /dev/pts");
//     //         }
//     //         Err(e) => {
//     //             // Check if it's already mounted (EBUSY is normal)
//     //             if e != nix::errno::Errno::EBUSY {
//     //                 log::warn!("Could not mount devpts: {e} (may already be mounted)");
//     //             }
//     //         }
//     //     }

//     //     // Ensure /dev/ptmx exists and links to /dev/pts/ptmx
//     //     let dev_ptmx = Path::new("/dev/ptmx");
//     //     if !dev_ptmx.exists() {
//     //         log::info!("Creating /dev/ptmx symlink");
//     //         std::os::unix::fs::symlink("/dev/pts/ptmx", "/dev/ptmx").ok();
//     //     }

//     //     Ok(())
//     // }

//     fn ensure_devpts_mounted() -> ContainerResult<()> {
//         let dev_pts = Path::new("/dev/pts");
//         if !dev_pts.exists() {
//             log::info!("Creating /dev/pts directory");
//             std::fs::create_dir_all(dev_pts).ok();
//         }
//         let result = mount(
//             Some("devpts"),
//             "/dev/pts",
//             Some("devpts"),
//             MsFlags::empty(),
//             Some("newinstance,ptmxmode=0666,mode=0620"),
//         );
//         match result {
//             Ok(_) => {
//                 log::info!("devpts filesystem mounted at /dev/pts");
//             }
//             Err(e) => {
//                 if e != nix::errno::Errno::EBUSY {
//                     log::warn!("Could not mount devpts: {e} (may already be mounted)");
//                 }
//             }
//         }
//         let dev_ptmx = Path::new("/dev/ptmx");
//         let needs_symlink = match std::fs::read_link(dev_ptmx) {
//             Ok(target) => target != Path::new("/dev/pts/ptmx"),
//             Err(_) => {
//                 if dev_ptmx.exists() {
//                     log::info!("Removing existing /dev/ptmx file");
//                     std::fs::remove_file(dev_ptmx).ok();
//                 }
//                 true
//             }
//         };

//         if needs_symlink {
//             log::info!("Creating /dev/ptmx symlink");
//             std::os::unix::fs::symlink("/dev/pts/ptmx", "/dev/ptmx").map_err(|e| {
//                 ContainerError::Cgroup {
//                     message: format!("Failed to create /dev/ptmx symlink error: {}", e),
//                 }
//             });
//         }
//         Ok(())
//     }

//     fn execute_with_pty(command: &str, argv: &[CString], envp: &[CString]) -> ContainerResult<()> {
//         let pty = openpty(None, None)
//             .map_err(|e| ContainerError::process_execution(format!("openpty failed: {e}")))?;

//         unsafe {
//             signal(Signal::SIGINT, SigHandler::Handler(handle_signal)).ok();
//             signal(Signal::SIGTERM, SigHandler::Handler(handle_signal)).ok();
//             signal(Signal::SIGQUIT, SigHandler::Handler(handle_signal)).ok();
//         }

//         match unsafe { fork()? } {
//             ForkResult::Child => {
//                 let _ = setsid();

//                 let mut stdin_fd = unsafe { OwnedFd::from_raw_fd(0) };
//                 let mut stdout_fd = unsafe { OwnedFd::from_raw_fd(1) };
//                 let mut stderr_fd = unsafe { OwnedFd::from_raw_fd(2) };

//                 dup2(&pty.slave, &mut stdin_fd).unwrap();
//                 dup2(&pty.slave, &mut stdout_fd).unwrap();
//                 dup2(&pty.slave, &mut stderr_fd).unwrap();

//                 std::mem::forget(stdin_fd);
//                 std::mem::forget(stdout_fd);
//                 std::mem::forget(stderr_fd);

//                 drop(pty.master);
//                 drop(pty.slave);

//                 unsafe {
//                     signal(Signal::SIGINT, SigHandler::SigDfl).ok();
//                     signal(Signal::SIGTERM, SigHandler::SigDfl).ok();
//                     signal(Signal::SIGQUIT, SigHandler::SigDfl).ok();
//                 }

//                 execve(&argv[0], argv, envp).map_err(|e| {
//                     ContainerError::process_execution(format!("execve failed for {command}: {e}"))
//                 })?;
//                 unreachable!()
//             }
//             ForkResult::Parent { child } => {
//                 CHILD_PID.store(child.as_raw(), Ordering::SeqCst);
//                 drop(pty.slave);

//                 log::info!("(Parent) Container process PID: {child}");

//                 let master_fd = pty.master.as_raw_fd();
//                 let master_fd_copy = unsafe { libc::dup(master_fd) };

//                 std::thread::spawn(move || {
//                     let mut master = unsafe { std::fs::File::from_raw_fd(master_fd_copy) };
//                     let mut buffer = [0u8; 1024];
//                     use std::io::{Read, Write};
//                     loop {
//                         match master.read(&mut buffer) {
//                             Ok(0) => break,
//                             Ok(n) => {
//                                 let _ = std::io::stdout().write_all(&buffer[..n]);
//                                 let _ = std::io::stdout().flush();
//                             }
//                             Err(_) => break,
//                         }
//                     }
//                 });

//                 Self::wait_for_child(child)?;
//                 CHILD_PID.store(0, Ordering::SeqCst);
//                 Ok(())
//             }
//         }
//     }

//     fn execute_without_pty(
//         command: &str,
//         argv: &[CString],
//         envp: &[CString],
//     ) -> ContainerResult<()> {
//         unsafe {
//             signal(Signal::SIGINT, SigHandler::Handler(handle_signal)).ok();
//             signal(Signal::SIGTERM, SigHandler::Handler(handle_signal)).ok();
//             signal(Signal::SIGQUIT, SigHandler::Handler(handle_signal)).ok();
//         }

//         match unsafe { fork()? } {
//             ForkResult::Child => {
//                 let _ = setsid();

//                 unsafe {
//                     signal(Signal::SIGINT, SigHandler::SigDfl).ok();
//                     signal(Signal::SIGTERM, SigHandler::SigDfl).ok();
//                     signal(Signal::SIGQUIT, SigHandler::SigDfl).ok();
//                 }

//                 execve(&argv[0], argv, envp).map_err(|e| {
//                     ContainerError::process_execution(format!("execve failed for {command}: {e}"))
//                 })?;
//                 unreachable!()
//             }
//             ForkResult::Parent { child } => {
//                 CHILD_PID.store(child.as_raw(), Ordering::SeqCst);
//                 log::info!("(Parent) Container process PID: {child}");

//                 Self::wait_for_child(child)?;
//                 CHILD_PID.store(0, Ordering::SeqCst);
//                 Ok(())
//             }
//         }
//     }

//     fn wait_for_child(child: Pid) -> ContainerResult<()> {
//         loop {
//             match waitpid(child, Some(WaitPidFlag::empty())) {
//                 Ok(WaitStatus::Exited(_, status)) => {
//                     log::info!("Container exited with status: {status}");
//                     if status != 0 {
//                         return Err(ContainerError::process_execution(format!(
//                             "Container process exited with non-zero status: {status}"
//                         )));
//                     }
//                     break;
//                 }
//                 Ok(WaitStatus::Signaled(_, sig, _)) => {
//                     log::warn!("Container killed by signal: {sig}");
//                     return Err(ContainerError::process_execution(format!(
//                         "Container process killed by signal: {sig}"
//                     )));
//                 }
//                 Ok(_) => continue,
//                 Err(nix::errno::Errno::EINTR) => continue,
//                 Err(e) => {
//                     return Err(ContainerError::process_execution(format!(
//                         "waitpid failed: {e}"
//                     )));
//                 }
//             }
//         }
//         Ok(())
//     }

//     pub fn build_argv(command_path: &str, args: &[String]) -> ContainerResult<Vec<CString>> {
//         let mut argv = vec![CString::new(command_path).unwrap()];
//         for arg in args {
//             argv.push(CString::new(arg.as_str()).unwrap());
//         }
//         Ok(argv)
//     }

//     pub fn build_environment() -> ContainerResult<Vec<CString>> {
//         let envs = vec![
//             "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
//             "TERM=xterm",
//             "HOME=/root",
//             "HOSTNAME=rust-container",
//             "container=rust-container-runtime",
//         ];
//         Ok(envs.iter().map(|s| CString::new(*s).unwrap()).collect())
//     }
// }
use crate::error::{ContainerError, ContainerResult, Context};
use nix::libc;
use nix::mount::{MsFlags, mount};
use nix::pty::openpty;
use nix::sys::signal::{SigHandler, Signal, kill, signal};
use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};
use nix::unistd::{ForkResult, Pid, dup2, execve, fork, setsid};
use std::ffi::CString;
use std::fmt::format;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::Path;
use std::sync::atomic::{AtomicI32, Ordering};

static CHILD_PID: AtomicI32 = AtomicI32::new(0);

extern "C" fn handle_signal(sig: i32) {
    let child = CHILD_PID.load(Ordering::SeqCst);
    if child > 0 {
        if let Ok(signal) = Signal::try_from(sig) {
            let _ = kill(Pid::from_raw(child), signal);
        }
    }
}

#[derive(Debug)]
pub struct ProcessManager;

impl ProcessManager {
    pub fn execute_container_command(command: &str, args: &[String]) -> ContainerResult<()> {
        log::info!("Executing container command: {command} with args: {args:?}");
        // Self::ensure_devpts_mounted()?;

        let command_path = if command.starts_with("/") {
            command.to_string()
        } else {
            ["/bin", "/usr/bin", "/sbin", "/usr/sbin"]
                .iter()
                .map(|prefix| format!("{}/{}", prefix, command))
                .find(|p| Path::new(p).exists())
                .unwrap_or_else(|| format!("/bin/{}", command))
        };

        if !Path::new(&command_path).exists() {
            return Err(ContainerError::process_execution(format!(
                "Command not found in container: {}",
                command_path
            )));
        }

        let argv = Self::build_argv(&command_path, args)?;
        let envp = Self::build_environment()?;

        // Try to create pseudo-terminal, fall back to direct execution if not available
        let use_pty = openpty(None, None).is_ok();

        if use_pty {
            Self::execute_with_pty(command, &argv, &envp)
        } else {
            log::warn!("PTY not available (ENODEV), running without PTY support");
            Self::execute_without_pty(command, &argv, &envp)
        }
    }

    fn ensure_devpts_mounted() -> ContainerResult<()> {
        let dev_pts = Path::new("/dev/pts");
        if !dev_pts.exists() {
            log::info!("Creating /dev/pts directory");
            std::fs::create_dir_all(dev_pts).map_err(|_| ContainerError::Cgroup {
                message: format!("failed to create /dev/pts "),
            })?;
        }

        let result = mount(
            Some("devpts"),
            "/dev/pts",
            Some("devpts"),
            MsFlags::empty(),
            Some("newinstance,ptmxmode=0666,mode=0620"),
        );

        match result {
            Ok(_) => {
                log::info!("devpts filesystem mounted at /dev/pts");
            }
            Err(e) => {
                if e != nix::errno::Errno::EBUSY {
                    log::warn!("Could not mount devpts: {e} (may already be mounted)");
                }
            }
        }

        let dev_ptmx = Path::new("/dev/ptmx");
        let needs_symlink = match std::fs::read_link(dev_ptmx) {
            Ok(target) => target != Path::new("/dev/pts/ptmx"),
            Err(_) => {
                if dev_ptmx.exists() {
                    log::info!("Removing existing /dev/ptmx file");
                    std::fs::remove_file(dev_ptmx).map_err(|_| ContainerError::Cgroup {
                        message: format!("Failed to remove /dev/ptmx"),
                    })?
                }
                true
            }
        };

        if needs_symlink {
            log::info!("Creating /dev/ptmx symlink");
            std::os::unix::fs::symlink("/dev/pts/ptmx", "/dev/ptmx").map_err(|_| {
                ContainerError::Cgroup {
                    message: format!("Failed to create /dev/ptmx symlink"),
                }
            })?
        }

        Ok(())
    }

    fn execute_with_pty(command: &str, argv: &[CString], envp: &[CString]) -> ContainerResult<()> {
        let pty = openpty(None, None)
            .map_err(|e| ContainerError::process_execution(format!("openpty failed: {e}")))?;

        unsafe {
            signal(Signal::SIGINT, SigHandler::Handler(handle_signal)).ok();
            signal(Signal::SIGTERM, SigHandler::Handler(handle_signal)).ok();
            signal(Signal::SIGQUIT, SigHandler::Handler(handle_signal)).ok();
        }

        match unsafe { fork()? } {
            ForkResult::Child => {
                // Child process
                drop(pty.master); // Close master in child

                let _ = setsid();

                // Redirect stdin, stdout, stderr to slave
                let slave_fd = pty.slave.as_raw_fd();
                unsafe {
                    libc::dup2(slave_fd, 0);
                    libc::dup2(slave_fd, 1);
                    libc::dup2(slave_fd, 2);
                }

                // Close the slave after dup2
                drop(pty.slave);

                // Reset signal handlers
                unsafe {
                    signal(Signal::SIGINT, SigHandler::SigDfl).ok();
                    signal(Signal::SIGTERM, SigHandler::SigDfl).ok();
                    signal(Signal::SIGQUIT, SigHandler::SigDfl).ok();
                }

                execve(&argv[0], argv, envp).map_err(|e| {
                    ContainerError::process_execution(format!("execve failed for {command}: {e}"))
                })?;
                unreachable!()
            }
            ForkResult::Parent { child } => {
                // Parent process
                CHILD_PID.store(child.as_raw(), Ordering::SeqCst);
                drop(pty.slave); // Close slave in parent

                log::info!("Container process PID: {child}");

                // Set master to non-blocking
                let master_fd = pty.master.as_raw_fd();
                unsafe {
                    let flags = libc::fcntl(master_fd, libc::F_GETFL, 0);
                    libc::fcntl(master_fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
                }

                // Spawn thread to copy stdin to master
                let master_fd_in = unsafe { libc::dup(master_fd) };
                std::thread::spawn(move || {
                    let mut stdin = std::io::stdin();
                    let mut master_in = unsafe { std::fs::File::from_raw_fd(master_fd_in) };
                    let mut buffer = [0u8; 1024];

                    loop {
                        match stdin.read(&mut buffer) {
                            Ok(0) => break, // EOF
                            Ok(n) => {
                                if master_in.write_all(&buffer[..n]).is_err() {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                });

                // Main thread: copy master to stdout
                let mut master_out = unsafe { std::fs::File::from_raw_fd(master_fd) };
                let mut stdout = std::io::stdout();
                let mut buffer = [0u8; 4096];

                loop {
                    // Check if child is still alive
                    match waitpid(child, Some(WaitPidFlag::WNOHANG)) {
                        Ok(WaitStatus::Exited(_, status)) => {
                            // Read any remaining output
                            while let Ok(n) = master_out.read(&mut buffer) {
                                if n == 0 {
                                    break;
                                }
                                let _ = stdout.write_all(&buffer[..n]);
                            }
                            let _ = stdout.flush();

                            log::info!("Container exited with status: {status}");
                            CHILD_PID.store(0, Ordering::SeqCst);

                            if status != 0 {
                                return Err(ContainerError::process_execution(format!(
                                    "Container process exited with non-zero status: {status}"
                                )));
                            }
                            return Ok(());
                        }
                        Ok(WaitStatus::Signaled(_, sig, _)) => {
                            log::warn!("Container killed by signal: {sig}");
                            CHILD_PID.store(0, Ordering::SeqCst);
                            return Err(ContainerError::process_execution(format!(
                                "Container process killed by signal: {sig}"
                            )));
                        }
                        Ok(WaitStatus::StillAlive) => {
                            // Child still running, continue reading output
                        }
                        Ok(_) => continue,
                        Err(nix::errno::Errno::ECHILD) => {
                            // Child already exited
                            break;
                        }
                        Err(_) => continue,
                    }

                    // Read from master and write to stdout
                    match master_out.read(&mut buffer) {
                        Ok(0) => {
                            // EOF - wait for child to exit
                            std::thread::sleep(std::time::Duration::from_millis(10));
                        }
                        Ok(n) => {
                            let _ = stdout.write_all(&buffer[..n]);
                            let _ = stdout.flush();
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            // No data available, sleep briefly
                            std::thread::sleep(std::time::Duration::from_millis(10));
                        }
                        Err(_) => {
                            // Error reading, child probably exited
                            std::thread::sleep(std::time::Duration::from_millis(10));
                        }
                    }
                }

                // Final wait for child
                match waitpid(child, Some(WaitPidFlag::empty())) {
                    Ok(WaitStatus::Exited(_, status)) => {
                        log::info!("Container exited with status: {status}");
                        CHILD_PID.store(0, Ordering::SeqCst);
                        if status != 0 {
                            return Err(ContainerError::process_execution(format!(
                                "Container process exited with non-zero status: {status}"
                            )));
                        }
                    }
                    Ok(WaitStatus::Signaled(_, sig, _)) => {
                        log::warn!("Container killed by signal: {sig}");
                        CHILD_PID.store(0, Ordering::SeqCst);
                        return Err(ContainerError::process_execution(format!(
                            "Container process killed by signal: {sig}"
                        )));
                    }
                    _ => {}
                }

                Ok(())
            }
        }
    }

    fn execute_without_pty(
        command: &str,
        argv: &[CString],
        envp: &[CString],
    ) -> ContainerResult<()> {
        unsafe {
            signal(Signal::SIGINT, SigHandler::Handler(handle_signal)).ok();
            signal(Signal::SIGTERM, SigHandler::Handler(handle_signal)).ok();
            signal(Signal::SIGQUIT, SigHandler::Handler(handle_signal)).ok();
        }

        match unsafe { fork()? } {
            ForkResult::Child => {
                let _ = setsid();

                unsafe {
                    signal(Signal::SIGINT, SigHandler::SigDfl).ok();
                    signal(Signal::SIGTERM, SigHandler::SigDfl).ok();
                    signal(Signal::SIGQUIT, SigHandler::SigDfl).ok();
                }

                execve(&argv[0], argv, envp).map_err(|e| {
                    ContainerError::process_execution(format!("execve failed for {command}: {e}"))
                })?;
                unreachable!()
            }
            ForkResult::Parent { child } => {
                CHILD_PID.store(child.as_raw(), Ordering::SeqCst);
                log::info!("Container process PID: {child}");

                Self::wait_for_child(child)?;
                CHILD_PID.store(0, Ordering::SeqCst);
                Ok(())
            }
        }
    }

    fn wait_for_child(child: Pid) -> ContainerResult<()> {
        loop {
            match waitpid(child, Some(WaitPidFlag::empty())) {
                Ok(WaitStatus::Exited(_, status)) => {
                    log::info!("Container exited with status: {status}");
                    if status != 0 {
                        return Err(ContainerError::process_execution(format!(
                            "Container process exited with non-zero status: {status}"
                        )));
                    }
                    break;
                }
                Ok(WaitStatus::Signaled(_, sig, _)) => {
                    log::warn!("Container killed by signal: {sig}");
                    return Err(ContainerError::process_execution(format!(
                        "Container process killed by signal: {sig}"
                    )));
                }
                Ok(_) => continue,
                Err(nix::errno::Errno::EINTR) => continue,
                Err(e) => {
                    return Err(ContainerError::process_execution(format!(
                        "waitpid failed: {e}"
                    )));
                }
            }
        }
        Ok(())
    }

    pub fn build_argv(command_path: &str, args: &[String]) -> ContainerResult<Vec<CString>> {
        let mut argv = vec![CString::new(command_path).unwrap()];
        for arg in args {
            argv.push(CString::new(arg.as_str()).unwrap());
        }
        Ok(argv)
    }

    pub fn build_environment() -> ContainerResult<Vec<CString>> {
        let envs = vec![
            "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
            "TERM=xterm",
            "HOME=/root",
            "HOSTNAME=rust-container",
            "container=rust-container-runtime",
        ];
        Ok(envs.iter().map(|s| CString::new(*s).unwrap()).collect())
    }
}
