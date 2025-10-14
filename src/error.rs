use std::ffi::NulError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ContainerError {
    #[error("IO error: {source}")]
    Io {
        #[from]
        source: std::io::Error,
    },
    #[error("Nix error: {source}")]
    Nix {
        #[from]
        source: nix::Error,
    },
    #[error("Namespace setup failed: {message}")]
    NamespaceSetup { message: String },
    #[error("Filesystem setup failed : {message}")]
    Filesystem { message: String },
    #[error("Process execution failed: {message}")]
    ProcessExecution { message: String },
    #[error("Root privileges required")]
    RootRequired,
    #[error("Invalid configuration: {message}")]
    InvalidConfiguration { message: String },
    #[error("Invalid string format: {source}")]
    InvalidString {
        #[from]
        source: NulError,
    },
    #[error("Container initialization failed: {message}")]
    Initialization { message: String },
    #[error("Cgroup(V2) setup failed: {message}")]
    Cgroup { message: String },
}
pub type ContainerResult<T> = Result<T, ContainerError>;

pub trait Context<T> {
    fn context<C>(self, context: C) -> ContainerResult<T>
    where
        C: Into<String>;
}
impl<T> Context<T> for ContainerResult<T> {
    fn context<C>(self, context: C) -> ContainerResult<T>
    where
        C: Into<String>,
    {
        self.map_err(|err| {
            let context_msg = context.into();
            match err {
                ContainerError::NamespaceSetup { message } => ContainerError::NamespaceSetup {
                    message: format!("{context_msg}:{message}"),
                },
                ContainerError::Filesystem { message } => ContainerError::Filesystem {
                    message: format!("{context_msg}:{message}"),
                },
                ContainerError::Initialization { message } => ContainerError::Initialization {
                    message: format!("{context_msg}:{message}"),
                },
                ContainerError::ProcessExecution { message } => ContainerError::ProcessExecution {
                    message: format!("{context_msg}:{message}"),
                },
                ContainerError::InvalidConfiguration { message } => {
                    ContainerError::InvalidConfiguration {
                        message: format!("{context_msg}:{message}"),
                    }
                }
                ContainerError::Cgroup { message } => ContainerError::Cgroup {
                    message: format!("{context_msg}:{message}"),
                },
                _ => err,
            }
        })
    }
}

impl ContainerError {
    pub fn name_space(message: impl Into<String>) -> Self {
        ContainerError::NamespaceSetup {
            message: message.into(),
        }
    }
    pub fn filesystem_setup(message: impl Into<String>) -> Self {
        ContainerError::Filesystem {
            message: message.into(),
        }
    }
    pub fn initialization(message: impl Into<String>) -> Self {
        ContainerError::Initialization {
            message: message.into(),
        }
    }
    pub fn process_execution(message: impl Into<String>) -> Self {
        ContainerError::ProcessExecution {
            message: message.into(),
        }
    }
    pub fn invalid_configuration(message: impl Into<String>) -> Self {
        ContainerError::InvalidConfiguration {
            message: message.into(),
        }
    }
    pub fn cgroup_setup(message: impl Into<String>) -> Self {
        ContainerError::Cgroup {
            message: message.into(),
        }
    }
}
