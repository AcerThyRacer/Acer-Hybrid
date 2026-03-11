//! Daemon command - manage the background daemon

use anyhow::Result;
use std::path::PathBuf;

use crate::DaemonCommands;

pub async fn execute(command: DaemonCommands) -> Result<()> {
    match command {
        DaemonCommands::Start => {
            start_daemon()?;
        }

        DaemonCommands::Stop => {
            stop_daemon()?;
        }

        DaemonCommands::Status => {
            let pid_file = get_pid_file();
            if !pid_file.exists() {
                println!("Daemon status: not running");
                return Ok(());
            }

            let pid = std::fs::read_to_string(&pid_file)?;
            println!("Daemon status: running (PID: {})", pid.trim());

            // Check if process is actually running
            #[cfg(unix)]
            {
                let status = std::process::Command::new("kill")
                    .args(["-0", pid.trim()])
                    .status();

                match status {
                    Ok(s) if s.success() => println!("Process is alive."),
                    _ => {
                        println!("Warning: Process not responding (stale PID file?)");
                    }
                }
            }
        }

        DaemonCommands::Restart => {
            stop_daemon()?;
            std::thread::sleep(std::time::Duration::from_secs(1));
            start_daemon()?;
        }
    }

    Ok(())
}

fn start_daemon() -> Result<()> {
    let pid_file = get_pid_file();
    if pid_file.exists() {
        let pid = std::fs::read_to_string(&pid_file)?;
        eprintln!("Daemon already running (PID: {})", pid.trim());
        eprintln!("Use 'acer daemon stop' to stop it first.");
        std::process::exit(1);
    }

    println!("Starting acer daemon...");
    let daemon_path = get_daemon_path();
    std::process::Command::new(&daemon_path)
        .spawn()
        .map_err(|e| anyhow::anyhow!("Failed to start daemon: {}", e))?;
    println!("Daemon started.");
    Ok(())
}

fn stop_daemon() -> Result<()> {
    let pid_file = get_pid_file();
    if !pid_file.exists() {
        println!("Daemon is not running.");
        return Ok(());
    }

    let pid: i32 = std::fs::read_to_string(&pid_file)?
        .trim()
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid PID: {}", e))?;

    println!("Stopping daemon (PID: {})...", pid);

    #[cfg(unix)]
    {
        use std::process::Command;
        let _ = Command::new("kill").arg(pid.to_string()).output();
    }

    let _ = std::fs::remove_file(&pid_file);
    println!("Daemon stopped.");
    Ok(())
}

fn get_pid_file() -> PathBuf {
    acer_core::AcerConfig::data_dir().join("acerd.pid")
}

fn get_daemon_path() -> PathBuf {
    // Look for acerd in the same directory as acer
    let current_exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("acerd"));

    current_exe
        .parent()
        .map(|p| p.join("acerd"))
        .unwrap_or_else(|| PathBuf::from("acerd"))
}
