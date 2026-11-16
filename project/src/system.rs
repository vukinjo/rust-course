//! `GET /system`: what the running process sees of the operating system, for
//! learning. Call it once on `cargo run` and once in the container, and
//! compare the answers.
//!
//! On Linux, almost all values come from files in `/proc` and `/sys`. The
//! kernel makes their content when a program reads them, so reading them is
//! how a program asks the kernel. On other systems, these fields are `null`.
use std::env;
use std::fs;
use std::path::Path;
use std::thread;
use std::time::{Instant, SystemTime};

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::Serialize;

/// When the service started, for the uptime of the process.
#[derive(Debug, Clone, Copy)]
pub struct Started {
    instant: Instant,
    at: SystemTime,
}

impl Started {
    pub fn now() -> Started {
        Started {
            instant: Instant::now(),
            at: SystemTime::now(),
        }
    }
}

pub fn router(started: Started) -> Router {
    Router::new()
        .route("/system", get(system))
        .with_state(started)
}

async fn system(State(started): State<Started>) -> Json<SystemInfo> {
    // The files in /proc are in memory, but a file read is still a blocking
    // call. Thus, all reads run on the blocking thread pool of Tokio.
    let info = tokio::task::spawn_blocking(move || SystemInfo::collect(started))
        .await
        .expect("collecting the system information does not panic");
    Json(info)
}

#[derive(Debug, Serialize)]
pub struct SystemInfo {
    pub build: Build,
    pub system: System,
    pub process: Process,
    pub resources: Resources,
    pub environment: Environment,
}

/// Fixed when the binary was compiled.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Build {
    pub version: &'static str,
    pub os: &'static str,
    pub arch: &'static str,
    /// The C library: "gnu" (glibc) or "musl"
    pub target_env: &'static str,
    /// True in the dev profile, false in release and small
    pub debug_assertions: bool,
}

/// The computer, or the container, that the process runs on.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct System {
    /// In a container, its own hostname: the UTS namespace
    pub hostname: Option<String>,
    /// From /etc/os-release, which belongs to the image in a container
    pub os_name: Option<String>,
    /// The kernel, which a container shares with its host
    pub kernel: Kernel,
    pub uptime_seconds: Option<u64>,
    /// Runnable processes, averaged over 1, 5 and 15 minutes
    pub load_average: Option<[f64; 3]>,
    pub in_container: bool,
}

#[derive(Debug, Serialize)]
pub struct Kernel {
    pub name: Option<String>,
    pub release: Option<String>,
    pub version: Option<String>,
}

/// This process.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Process {
    /// 1 in a container: the PID namespace
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
    pub executable: Option<String>,
    pub working_directory: Option<String>,
    pub started_at: DateTime<Utc>,
    pub uptime_seconds: u64,
}

/// What the process uses, and its limits.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Resources {
    /// The main thread, the Tokio workers and the blocking threads
    pub threads: Option<u32>,
    /// Sockets, database connections and other files
    pub open_files: Option<usize>,
    pub open_files_limit: Option<u64>,
    pub resident_memory_bytes: Option<u64>,
    /// Follows the CPU limit of the cgroup
    pub available_cpus: Option<usize>,
    /// The memory limit of the cgroup, null if there is none
    pub memory_limit_bytes: Option<u64>,
}

/// Only the names of the settings variables: the values can contain secrets.
#[derive(Debug, Serialize)]
pub struct Environment {
    pub name: Option<String>,
    pub variables: Vec<String>,
}

impl SystemInfo {
    pub fn collect(started: Started) -> SystemInfo {
        let status = read("/proc/self/status").unwrap_or_default();
        let field = |key| status_field(&status, key);
        SystemInfo {
            build: Build {
                version: env!("CARGO_PKG_VERSION"),
                os: env::consts::OS,
                arch: env::consts::ARCH,
                target_env: TARGET_ENV,
                debug_assertions: cfg!(debug_assertions),
            },
            system: System {
                hostname: read("/proc/sys/kernel/hostname"),
                os_name: read("/etc/os-release").and_then(|text| os_release_name(&text)),
                kernel: Kernel {
                    name: read("/proc/sys/kernel/ostype"),
                    release: read("/proc/sys/kernel/osrelease"),
                    version: read("/proc/sys/kernel/version"),
                },
                uptime_seconds: read("/proc/uptime").and_then(|text| first_number(&text)),
                load_average: read("/proc/loadavg").and_then(|text| load_average(&text)),
                in_container: Path::new("/run/.containerenv").exists()
                    || Path::new("/.dockerenv").exists(),
            },
            process: Process {
                pid: std::process::id(),
                parent_pid: field("PPid").and_then(|v| v.parse().ok()),
                uid: field("Uid").and_then(first_number),
                gid: field("Gid").and_then(first_number),
                executable: env::current_exe().ok().map(|p| p.display().to_string()),
                working_directory: env::current_dir().ok().map(|p| p.display().to_string()),
                started_at: DateTime::from(started.at),
                uptime_seconds: started.instant.elapsed().as_secs(),
            },
            resources: Resources {
                threads: field("Threads").and_then(|v| v.parse().ok()),
                open_files: fs::read_dir("/proc/self/fd").ok().map(|dir| dir.count()),
                open_files_limit: read("/proc/self/limits")
                    .and_then(|text| soft_limit(&text, "Max open files")),
                resident_memory_bytes: field("VmRSS")
                    .and_then(first_number::<u64>)
                    .map(|kib| kib * 1024),
                available_cpus: thread::available_parallelism().ok().map(|n| n.get()),
                // memory.max is "max" when there is no limit, which does not parse
                memory_limit_bytes: cgroup_file("memory.max").and_then(|max| max.parse().ok()),
            },
            environment: Environment {
                name: env::var("APP_ENVIRONMENT").ok(),
                variables: settings_variables(),
            },
        }
    }
}

const TARGET_ENV: &str = if cfg!(target_env = "gnu") {
    "gnu"
} else if cfg!(target_env = "musl") {
    "musl"
} else {
    ""
};

/// A file as text without the newline at the end, or None if it cannot be
/// read (for example, /proc on macOS).
fn read(path: &str) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|text| text.trim_end().to_string())
}

/// The value of `key` in /proc/self/status, where each line is "Key:\tvalue".
fn status_field<'a>(status: &'a str, key: &str) -> Option<&'a str> {
    status.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        (name == key).then(|| value.trim())
    })
}

/// The first number in a text such as "1000\t1000\t1000\t1000" or "8832 kB".
fn first_number<T: std::str::FromStr>(text: &str) -> Option<T> {
    text.split_whitespace()
        .next()?
        .split('.')
        .next()?
        .parse()
        .ok()
}

/// "0.52 0.58 0.59 1/512 4242": the first three numbers.
fn load_average(text: &str) -> Option<[f64; 3]> {
    let mut numbers = text.split_whitespace().map(|n| n.parse().ok());
    Some([numbers.next()??, numbers.next()??, numbers.next()??])
}

/// The soft limit in a line of /proc/self/limits, such as
/// "Max open files            1024                 524288               files".
fn soft_limit(limits: &str, name: &str) -> Option<u64> {
    let line = limits.lines().find(|line| line.starts_with(name))?;
    line[name.len()..].split_whitespace().next()?.parse().ok()
}

/// PRETTY_NAME="Debian GNU/Linux 13 (trixie)" in /etc/os-release.
fn os_release_name(text: &str) -> Option<String> {
    let value = text
        .lines()
        .find_map(|line| line.strip_prefix("PRETTY_NAME="))?;
    Some(value.trim_matches('"').to_string())
}

/// A file of the cgroup of this process. /proc/self/cgroup names the cgroup
/// ("0::/user.slice/..." on your computer, "0::/" in a container), and its
/// files are in /sys/fs/cgroup.
fn cgroup_file(name: &str) -> Option<String> {
    let cgroup = read("/proc/self/cgroup")?;
    let path = cgroup.lines().find_map(|line| line.strip_prefix("0::"))?;
    read(&format!(
        "/sys/fs/cgroup{}/{name}",
        path.trim_end_matches('/')
    ))
}

/// The names of the APP_* variables, sorted. Never their values.
fn settings_variables() -> Vec<String> {
    let mut names: Vec<String> = env::vars_os()
        .filter_map(|(name, _)| name.into_string().ok())
        .filter(|name| name.starts_with("APP_"))
        .collect();
    names.sort();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATUS: &str = "Name:\tshoes\nPPid:\t1\nUid:\t65532\t65532\t65532\t65532\n\
                          Gid:\t65532\t65532\t65532\t65532\nVmRSS:\t   12288 kB\nThreads:\t9\n";

    #[test]
    fn reads_fields_of_proc_status() {
        assert_eq!(status_field(STATUS, "Threads"), Some("9"));
        assert_eq!(
            status_field(STATUS, "Uid").and_then(first_number),
            Some(65532)
        );
        assert_eq!(
            status_field(STATUS, "VmRSS").and_then(first_number),
            Some(12288)
        );
        assert_eq!(status_field(STATUS, "Missing"), None);
    }

    #[test]
    fn reads_limits_and_load() {
        let limits = "Limit                     Soft Limit           Hard Limit           Units\n\
                      Max open files            1024                 524288               files\n";
        assert_eq!(soft_limit(limits, "Max open files"), Some(1024));
        assert_eq!(
            load_average("0.52 0.58 0.59 1/512 4242"),
            Some([0.52, 0.58, 0.59])
        );
        assert_eq!(first_number::<u64>("8123.45 31234.56"), Some(8123));
    }

    #[test]
    fn reads_the_os_name() {
        let text = "PRETTY_NAME=\"Debian GNU/Linux 13 (trixie)\"\nNAME=\"Debian GNU/Linux\"\n";
        assert_eq!(
            os_release_name(text).as_deref(),
            Some("Debian GNU/Linux 13 (trixie)")
        );
    }

    #[test]
    fn lists_only_names() {
        // Under cargo test, .cargo/config.toml sets the APP_* variables
        let names = settings_variables();
        assert!(names.contains(&"APP_DATABASE__URL".to_string()));
        assert!(names.iter().all(|name| name.starts_with("APP_")));
    }
}
