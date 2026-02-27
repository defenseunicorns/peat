//! Jetson platform utilities and metrics collection
//!
//! This module provides Jetson-specific functionality:
//! - GPU/CPU/memory metrics via tegrastats
//! - Power mode detection and management
//! - Temperature monitoring
//! - Clock frequency queries
//!
//! ## Usage
//!
//! ```rust,ignore
//! use peat_inference::inference::jetson::{JetsonMetrics, JetsonInfo};
//!
//! // Get platform info
//! let info = JetsonInfo::detect()?;
//! println!("Running on: {} ({})", info.model, info.jetpack_version);
//!
//! // Start metrics collection
//! let mut metrics = JetsonMetrics::new()?;
//! metrics.start_collection(Duration::from_millis(100)).await?;
//!
//! // Get current stats
//! let stats = metrics.current();
//! println!("GPU: {}%, Temp: {}°C", stats.gpu_utilization, stats.gpu_temp);
//! ```

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::Path;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::watch;

/// Jetson platform information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JetsonInfo {
    /// Jetson model (e.g., "Xavier NX", "AGX Orin")
    pub model: String,
    /// JetPack version
    pub jetpack_version: String,
    /// L4T version
    pub l4t_version: String,
    /// CUDA version
    pub cuda_version: String,
    /// TensorRT version
    pub tensorrt_version: String,
    /// Total GPU memory in MB
    pub gpu_memory_mb: u64,
    /// Number of CPU cores
    pub cpu_cores: usize,
    /// Number of DLA cores (0, 1, or 2)
    pub dla_cores: usize,
}

impl JetsonInfo {
    /// Detect Jetson platform information
    pub fn detect() -> anyhow::Result<Self> {
        // Read from /etc/nv_tegra_release or similar
        let model = Self::detect_model()?;
        let jetpack = Self::detect_jetpack()?;
        let l4t = Self::detect_l4t()?;
        let cuda = Self::detect_cuda()?;
        let trt = Self::detect_tensorrt()?;

        Ok(Self {
            model,
            jetpack_version: jetpack,
            l4t_version: l4t,
            cuda_version: cuda,
            tensorrt_version: trt,
            gpu_memory_mb: Self::detect_gpu_memory()?,
            cpu_cores: num_cpus::get(),
            dla_cores: Self::detect_dla_cores(),
        })
    }

    fn detect_model() -> anyhow::Result<String> {
        // Try reading from device tree
        let model_path = "/proc/device-tree/model";
        if Path::new(model_path).exists() {
            let content = std::fs::read_to_string(model_path)?;
            return Ok(content.trim_matches('\0').trim().to_string());
        }

        // Fallback: try tegra chip
        let chip_path = "/sys/module/tegra_fuse/parameters/tegra_chip_id";
        if Path::new(chip_path).exists() {
            let chip_id = std::fs::read_to_string(chip_path)?;
            let model = match chip_id.trim() {
                "33" => "Jetson TX1",
                "24" => "Jetson TX2",
                "25" => "Jetson Xavier",
                "35" => "Jetson Orin",
                id => &format!("Unknown Tegra ({})", id),
            };
            return Ok(model.to_string());
        }

        Ok("Unknown Jetson".to_string())
    }

    fn detect_jetpack() -> anyhow::Result<String> {
        // JetPack version is typically in /etc/nv_tegra_release
        let release_path = "/etc/nv_tegra_release";
        if Path::new(release_path).exists() {
            let content = std::fs::read_to_string(release_path)?;
            // Parse "# R35 (release), REVISION: 1.0" -> extract version
            if let Some(line) = content.lines().next() {
                return Ok(line.to_string());
            }
        }
        Ok("Unknown".to_string())
    }

    fn detect_l4t() -> anyhow::Result<String> {
        let version_path = "/etc/nv_tegra_release";
        if Path::new(version_path).exists() {
            let content = std::fs::read_to_string(version_path)?;
            // Extract L4T version
            for line in content.lines() {
                if line.contains("R") && line.contains("REVISION") {
                    return Ok(line.to_string());
                }
            }
        }
        Ok("Unknown".to_string())
    }

    fn detect_cuda() -> anyhow::Result<String> {
        // Try nvcc --version
        let output = std::process::Command::new("nvcc").arg("--version").output();

        if let Ok(output) = output {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.contains("release") {
                    return Ok(line.to_string());
                }
            }
        }

        // Try reading from /usr/local/cuda/version.txt
        let version_path = "/usr/local/cuda/version.txt";
        if Path::new(version_path).exists() {
            return Ok(std::fs::read_to_string(version_path)?.trim().to_string());
        }

        Ok("Unknown".to_string())
    }

    fn detect_tensorrt() -> anyhow::Result<String> {
        // Try dpkg query
        let output = std::process::Command::new("dpkg")
            .args(["-l", "tensorrt"])
            .output();

        if let Ok(output) = output {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.contains("tensorrt") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 3 {
                        return Ok(parts[2].to_string());
                    }
                }
            }
        }

        Ok("Unknown".to_string())
    }

    fn detect_gpu_memory() -> anyhow::Result<u64> {
        // On Jetson, GPU shares system memory
        // Read from /proc/meminfo
        let meminfo = std::fs::read_to_string("/proc/meminfo")?;
        for line in meminfo.lines() {
            if line.starts_with("MemTotal:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let kb: u64 = parts[1].parse()?;
                    return Ok(kb / 1024); // Convert to MB
                }
            }
        }
        Ok(0)
    }

    fn detect_dla_cores() -> usize {
        // Xavier has 2 DLA cores, Orin has 2, TX2/Nano have 0
        // Check for DLA in device tree
        let dla_path = "/sys/class/dla";
        if Path::new(dla_path).exists() {
            if let Ok(entries) = std::fs::read_dir(dla_path) {
                return entries.count();
            }
        }
        0
    }

    /// Check if running on actual Jetson hardware
    pub fn is_jetson() -> bool {
        Path::new("/etc/nv_tegra_release").exists()
            || Path::new("/proc/device-tree/model")
                .exists()
                .then(|| {
                    std::fs::read_to_string("/proc/device-tree/model")
                        .map(|s| {
                            s.to_lowercase().contains("jetson")
                                || s.to_lowercase().contains("tegra")
                        })
                        .unwrap_or(false)
                })
                .unwrap_or(false)
    }
}

/// Current Jetson system statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JetsonStats {
    /// GPU utilization (0-100)
    pub gpu_utilization: f32,
    /// GPU frequency in MHz
    pub gpu_freq_mhz: u32,
    /// GPU temperature in Celsius
    pub gpu_temp: f32,
    /// CPU utilization per core (0-100)
    pub cpu_utilization: Vec<f32>,
    /// CPU frequency in MHz (per core)
    pub cpu_freq_mhz: Vec<u32>,
    /// CPU temperature in Celsius
    pub cpu_temp: f32,
    /// Memory used in MB
    pub memory_used_mb: u64,
    /// Memory total in MB
    pub memory_total_mb: u64,
    /// Power consumption in milliwatts
    pub power_mw: u32,
    /// Current power mode name
    pub power_mode: String,
    /// EMC (memory controller) utilization
    pub emc_utilization: f32,
    /// Timestamp
    pub timestamp: std::time::SystemTime,
}

impl Default for JetsonStats {
    fn default() -> Self {
        Self {
            gpu_utilization: 0.0,
            gpu_freq_mhz: 0,
            gpu_temp: 0.0,
            cpu_utilization: Vec::new(),
            cpu_freq_mhz: Vec::new(),
            cpu_temp: 0.0,
            memory_used_mb: 0,
            memory_total_mb: 0,
            power_mw: 0,
            power_mode: String::new(),
            emc_utilization: 0.0,
            timestamp: std::time::SystemTime::now(),
        }
    }
}

impl JetsonStats {
    /// Memory utilization as percentage
    pub fn memory_utilization(&self) -> f32 {
        if self.memory_total_mb > 0 {
            (self.memory_used_mb as f32 / self.memory_total_mb as f32) * 100.0
        } else {
            0.0
        }
    }

    /// Average CPU utilization
    pub fn avg_cpu_utilization(&self) -> f32 {
        if self.cpu_utilization.is_empty() {
            0.0
        } else {
            self.cpu_utilization.iter().sum::<f32>() / self.cpu_utilization.len() as f32
        }
    }
}

/// Jetson metrics collector
///
/// Collects system metrics by parsing tegrastats output.
pub struct JetsonMetrics {
    /// Current stats
    current: JetsonStats,
    /// Historical stats
    history: VecDeque<JetsonStats>,
    /// Max history size
    max_history: usize,
    /// Collection interval
    interval: Duration,
    /// Is collection running
    running: bool,
    /// Shutdown signal
    shutdown_tx: Option<watch::Sender<bool>>,
}

impl JetsonMetrics {
    /// Create a new metrics collector
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            current: JetsonStats::default(),
            history: VecDeque::new(),
            max_history: 1000,
            interval: Duration::from_millis(100),
            running: false,
            shutdown_tx: None,
        })
    }

    /// Start background metrics collection
    pub async fn start_collection(&mut self, interval: Duration) -> anyhow::Result<()> {
        if self.running {
            return Ok(());
        }

        self.interval = interval;

        // Check if tegrastats is available
        if !JetsonInfo::is_jetson() {
            tracing::warn!("Not running on Jetson - metrics collection will use simulated data");
            self.start_simulated_collection().await?;
            return Ok(());
        }

        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        self.shutdown_tx = Some(shutdown_tx);
        self.running = true;

        // Spawn tegrastats process
        let interval_ms = interval.as_millis() as u32;
        let mut cmd = Command::new("tegrastats")
            .arg("--interval")
            .arg(interval_ms.to_string())
            .stdout(std::process::Stdio::piped())
            .spawn()?;

        let stdout = cmd.stdout.take().expect("Failed to get stdout");
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();

        tokio::spawn(async move {
            let mut shutdown_rx = shutdown_rx;

            loop {
                tokio::select! {
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            let _ = cmd.kill().await;
                            break;
                        }
                    }
                    line = lines.next_line() => {
                        match line {
                            Ok(Some(line)) => {
                                if let Ok(stats) = Self::parse_tegrastats(&line) {
                                    // TODO: Send stats to main struct
                                    tracing::trace!("GPU: {}%, Mem: {}MB", stats.gpu_utilization, stats.memory_used_mb);
                                }
                            }
                            Ok(None) => break,
                            Err(e) => {
                                tracing::error!("Error reading tegrastats: {}", e);
                                break;
                            }
                        }
                    }
                }
            }
        });

        Ok(())
    }

    /// Start simulated collection for non-Jetson platforms
    async fn start_simulated_collection(&mut self) -> anyhow::Result<()> {
        self.running = true;

        // Just set some default values
        self.current = JetsonStats {
            gpu_utilization: 50.0,
            gpu_freq_mhz: 1000,
            gpu_temp: 45.0,
            cpu_utilization: vec![30.0; 6],
            cpu_freq_mhz: vec![1500; 6],
            cpu_temp: 40.0,
            memory_used_mb: 4096,
            memory_total_mb: 8192,
            power_mw: 15000,
            power_mode: "SIMULATED".to_string(),
            emc_utilization: 25.0,
            timestamp: std::time::SystemTime::now(),
        };

        Ok(())
    }

    /// Stop metrics collection
    pub fn stop_collection(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(true);
        }
        self.running = false;
    }

    /// Get current stats
    pub fn current(&self) -> &JetsonStats {
        &self.current
    }

    /// Get stats history
    pub fn history(&self) -> &VecDeque<JetsonStats> {
        &self.history
    }

    /// Parse a tegrastats output line
    ///
    /// Example line:
    /// "RAM 2432/7772MB (lfb 1x4MB) CPU [20%@1190,15%@1190,16%@1190,18%@1190,off,off] EMC_FREQ 0% GR3D_FREQ 0% PLL@41C CPU@43.5C PMIC@100C GPU@42C AO@49C thermal@42.6C POM_5V_IN 3977/3977 POM_5V_GPU 0/0 POM_5V_CPU 623/623"
    fn parse_tegrastats(line: &str) -> anyhow::Result<JetsonStats> {
        let mut stats = JetsonStats {
            timestamp: std::time::SystemTime::now(),
            ..Default::default()
        };

        // Parse RAM
        if let Some(ram_match) = line.find("RAM ") {
            let ram_str = &line[ram_match + 4..];
            if let Some(slash) = ram_str.find('/') {
                let used_str = &ram_str[..slash];
                if let Ok(used) = used_str.parse::<u64>() {
                    stats.memory_used_mb = used;
                }
                if let Some(mb_pos) = ram_str.find("MB") {
                    let total_str = &ram_str[slash + 1..mb_pos];
                    if let Ok(total) = total_str.parse::<u64>() {
                        stats.memory_total_mb = total;
                    }
                }
            }
        }

        // Parse CPU utilization
        if let Some(cpu_start) = line.find("CPU [") {
            if let Some(cpu_end) = line[cpu_start..].find(']') {
                let cpu_str = &line[cpu_start + 5..cpu_start + cpu_end];
                for core in cpu_str.split(',') {
                    if core.contains('%') {
                        if let Some(pct_pos) = core.find('%') {
                            if let Ok(pct) = core[..pct_pos].parse::<f32>() {
                                stats.cpu_utilization.push(pct);
                            }
                        }
                    }
                }
            }
        }

        // Parse GPU frequency
        if let Some(gr3d_match) = line.find("GR3D_FREQ ") {
            let gr3d_str = &line[gr3d_match + 10..];
            if let Some(pct_pos) = gr3d_str.find('%') {
                if let Ok(pct) = gr3d_str[..pct_pos].parse::<f32>() {
                    stats.gpu_utilization = pct;
                }
            }
        }

        // Parse temperatures
        for part in line.split_whitespace() {
            if part.starts_with("GPU@") && part.ends_with('C') {
                if let Ok(temp) = part[4..part.len() - 1].parse::<f32>() {
                    stats.gpu_temp = temp;
                }
            }
            if part.starts_with("CPU@") && part.ends_with('C') {
                if let Ok(temp) = part[4..part.len() - 1].parse::<f32>() {
                    stats.cpu_temp = temp;
                }
            }
        }

        // Parse power (POM_5V_IN)
        if let Some(pom_match) = line.find("POM_5V_IN ") {
            let pom_str = &line[pom_match + 10..];
            if let Some(slash) = pom_str.find('/') {
                if let Ok(power) = pom_str[..slash].parse::<u32>() {
                    stats.power_mw = power;
                }
            }
        }

        // Parse EMC utilization
        if let Some(emc_match) = line.find("EMC_FREQ ") {
            let emc_str = &line[emc_match + 9..];
            if let Some(pct_pos) = emc_str.find('%') {
                if let Ok(pct) = emc_str[..pct_pos].parse::<f32>() {
                    stats.emc_utilization = pct;
                }
            }
        }

        Ok(stats)
    }

    /// Get power mode
    pub async fn get_power_mode() -> anyhow::Result<String> {
        let output = Command::new("nvpmodel").arg("-q").output().await?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if line.contains("NV Power Mode") {
                return Ok(line.to_string());
            }
        }

        Ok("Unknown".to_string())
    }

    /// Set power mode (requires sudo)
    ///
    /// Common modes:
    /// - 0: MAX performance (MAXN)
    /// - 1: 15W (on Xavier)
    /// - 2: 10W
    pub async fn set_power_mode(mode: u8) -> anyhow::Result<()> {
        let output = Command::new("sudo")
            .args(["nvpmodel", "-m", &mode.to_string()])
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to set power mode: {}", stderr);
        }

        Ok(())
    }

    /// Maximize clocks for best inference performance (requires sudo)
    pub async fn maximize_clocks() -> anyhow::Result<()> {
        let output = Command::new("sudo").arg("jetson_clocks").output().await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to maximize clocks: {}", stderr);
        }

        Ok(())
    }
}

impl Default for JetsonMetrics {
    fn default() -> Self {
        Self::new().unwrap()
    }
}

impl Drop for JetsonMetrics {
    fn drop(&mut self) {
        self.stop_collection();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tegrastats() {
        let line = "RAM 2432/7772MB (lfb 1x4MB) CPU [20%@1190,15%@1190,16%@1190,18%@1190,off,off] EMC_FREQ 25% GR3D_FREQ 50% PLL@41C CPU@43.5C PMIC@100C GPU@42C AO@49C thermal@42.6C POM_5V_IN 3977/3977 POM_5V_GPU 0/0 POM_5V_CPU 623/623";

        let stats = JetsonMetrics::parse_tegrastats(line).unwrap();

        assert_eq!(stats.memory_used_mb, 2432);
        assert_eq!(stats.memory_total_mb, 7772);
        assert_eq!(stats.cpu_utilization.len(), 4);
        assert_eq!(stats.cpu_utilization[0], 20.0);
        assert_eq!(stats.gpu_utilization, 50.0);
        assert_eq!(stats.gpu_temp, 42.0);
        assert_eq!(stats.cpu_temp, 43.5);
        assert_eq!(stats.power_mw, 3977);
        assert_eq!(stats.emc_utilization, 25.0);
    }

    #[test]
    fn test_stats_derived() {
        let stats = JetsonStats {
            memory_used_mb: 4000,
            memory_total_mb: 8000,
            cpu_utilization: vec![20.0, 30.0, 40.0, 50.0],
            ..Default::default()
        };

        assert_eq!(stats.memory_utilization(), 50.0);
        assert_eq!(stats.avg_cpu_utilization(), 35.0);
    }

    #[test]
    fn test_is_jetson_detection() {
        // This will return false on non-Jetson systems
        let is_jetson = JetsonInfo::is_jetson();
        println!("Running on Jetson: {}", is_jetson);
        // Just verify it doesn't crash
    }
}
