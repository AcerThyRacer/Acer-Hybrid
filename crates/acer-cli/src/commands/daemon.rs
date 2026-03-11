//! Daemon command - manage the background daemon

use anyhow::Result;
use std::path::PathBuf;

use crate::DaemonCommands;

pub async fn execute(command: DaemonCommands) -> Result<()> {
    let pid_file = get_pid_file();
    
    match command {
        DaemonCommands::Start => {
            if pid_file.exists() {
                let pid = std::fs::read_to_string(&pid_file)?;
                eprintln!("Daemon already running (PID: {})", pid.trim());
                eprintln!("Use 'acer daemon stop' to stop it first.");
                std::process::exit(1);
            }
            
            println!("Starting acer daemon...");
            
            // Start daemon process
            let daemon_path = get_daemon_path();
            
            let mut cmd = std::process::Command::new(&daemon_path);
            cmd.spawn()
                .map_err(|e| anyhow::anyhow!("Failed to start daemon: {}", e))?;
            
            println!("Daemon started.");
        }
        
        DaemonCommands::Stop => {
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
                let _ = Command::new("kill")
                    .arg(pid.to_string())
                    .output();
            }
            
            let _ = std::fs::remove_file(&pid_file);
            println!("Daemon stopped.");
        }
        
        DaemonCommands::Status => {
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
            execute(DaemonCommands::Stop).await?;
            std::thread::sleep(std::time::Duration::from_secs(1));
            execute(DaemonCommands::Start).await?;
        }
    }
    
    Ok(())
}

fn get_pid_file() -> PathBuf {
    acer_core::AcerConfig::data_dir().join("acerd.pid")
}

fn get_daemon_path() -> PathBuf {
    // Look for acerd in the same directory as acer
    let current_exe = std::env::current_exe()
        .unwrap_or_else(|_| PathBuf::from("acerd"));
    
    current_exe
        .parent()
        .map(|p| p.join("acerd"))
        .unwrap_or_else(|| PathBuf::from("acerd"))
}