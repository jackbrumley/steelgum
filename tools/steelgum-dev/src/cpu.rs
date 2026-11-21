use std::error::Error;
use std::process::Command;

pub(super) fn build_command() -> Result<Command, Box<dyn Error>> {
    let available = std::thread::available_parallelism()?.get();
    let maximum = (available / 2).max(1);
    let jobs = match std::env::var("STEELGUM_BUILD_CPUS") {
        Ok(value) => value
            .parse::<std::num::NonZeroUsize>()
            .map_err(|_| "STEELGUM_BUILD_CPUS must be a positive whole number")?
            .get()
            .min(maximum),
        Err(std::env::VarError::NotPresent) => maximum,
        Err(error) => return Err(error.into()),
    };
    let mut command = restricted_cargo(jobs, available)?;
    command.args(["build", "--jobs", &jobs.to_string()]);
    Ok(command)
}

#[cfg(target_os = "linux")]
fn restricted_cargo(jobs: usize, available: usize) -> Result<Command, Box<dyn Error>> {
    let status = std::fs::read_to_string("/proc/self/status")?;
    let allowed = status
        .lines()
        .find_map(|line| line.strip_prefix("Cpus_allowed_list:"))
        .ok_or("Linux did not report the build process's allowed CPUs")?;
    let cpus = cpu_list(allowed.trim(), jobs)?;
    let mut command = Command::new("taskset");
    command.args(["--cpu-list", &cpus, "nice", "-n", "10", "cargo"]);
    if available == 1 {
        println!("Desktop build: one logical CPU available, using one job at lower priority.");
    } else {
        println!("Desktop build: at most {jobs} of {available} logical CPUs, at lower priority.");
    }
    Ok(command)
}

#[cfg(not(target_os = "linux"))]
fn restricted_cargo(jobs: usize, available: usize) -> Result<Command, Box<dyn Error>> {
    println!("Desktop build: {jobs} Cargo jobs for {available} logical CPUs.");
    Ok(Command::new("cargo"))
}

#[cfg(target_os = "linux")]
fn cpu_list(allowed: &str, limit: usize) -> Result<String, Box<dyn Error>> {
    let mut selected = Vec::new();
    for entry in allowed.split(',') {
        let (start, end) = entry.split_once('-').unwrap_or((entry, entry));
        let (start, end) = (start.parse::<usize>()?, end.parse::<usize>()?);
        if start > end {
            return Err("Linux reported an invalid CPU range".into());
        }
        selected.extend(
            (start..=end)
                .take(limit - selected.len())
                .map(|cpu| cpu.to_string()),
        );
        if selected.len() == limit {
            return Ok(selected.join(","));
        }
    }
    Err("Linux reported fewer allowed CPUs than the build budget".into())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::cpu_list;

    #[test]
    fn uses_only_allowed_cpus_and_handles_sparse_affinity() {
        assert_eq!(cpu_list("2,5-8,12", 3).unwrap(), "2,5,6");
        assert_eq!(cpu_list("3-6", 2).unwrap(), "3,4");
        assert_eq!(cpu_list("7", 1).unwrap(), "7");
        assert!(cpu_list("7", 2).is_err());
    }
}
