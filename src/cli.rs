use clap::{Arg, Command};

#[derive(Debug, Clone)]
pub struct ContainerConfig {
    pub rootfs: String,
    pub command: String,
    pub args: Vec<String>,
    pub hostname: Option<String>,
}

pub fn parse_args() -> ContainerConfig {
    let matches = Command::new("container-runtime")
        .version("0.1.0")
        .about("A simple container runtime in Rust")
        .arg(
            Arg::new("rootfs")
                .long("rootfs")
                .value_name("PATH")
                .required(true)
                .help("Path to root filesystem")
                .value_parser(clap::value_parser!(String)),
        )
        .arg(
            Arg::new("hostname")
                .long("hostname")
                .value_name("HOSTNAME")
                .help("container hostname")
                .value_parser(clap::value_parser!(String)),
        )
        .arg(
            Arg::new("command")
                .help("Command to execute inside container")
                .required(true)
                .index(1)
                .value_parser(clap::value_parser!(String)),
        )
        .arg(
            Arg::new("args")
                .help("Arguments for the command")
                .num_args(0..)
                .index(2)
                .value_parser(clap::value_parser!(String)),
        )
        .get_matches();
    let rootfs = matches
        .get_one::<String>("rootfs")
        .expect("rootfs is required")
        .clone();
    let command = matches
        .get_one::<String>("command")
        .expect("command is required")
        .clone();
    let args: Vec<String> = matches
        .get_many::<String>("args")
        .map(|vals| vals.cloned().collect())
        .unwrap_or_default();
    let hostname = matches.get_one::<String>("hostname").cloned();
    ContainerConfig {
        rootfs,
        command,
        args,
        hostname,
    }
}
