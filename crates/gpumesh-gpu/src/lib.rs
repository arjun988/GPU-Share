//! NVIDIA GPU discovery and monitoring (NVML). Falls back to nvidia-smi when unavailable.

use gpumesh_common::{GpuMeshError, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    pub index: u32,
    pub name: String,
    pub uuid: Option<String>,
    pub vram_total_mb: u64,
    pub vram_used_mb: u64,
    pub vram_free_mb: u64,
    pub utilization_gpu: Option<u32>,
    pub utilization_mem: Option<u32>,
    pub temperature_c: Option<u32>,
    pub power_watts: Option<u32>,
    pub driver_version: Option<String>,
    pub cuda_version: Option<String>,
    pub compute_capability: Option<String>,
}

impl GpuInfo {
    pub fn primary_summary(gpus: &[GpuInfo]) -> Option<String> {
        gpus.first().map(|g| {
            format!(
                "{} | VRAM {}/{} MB | util {}%",
                g.name,
                g.vram_used_mb,
                g.vram_total_mb,
                g.utilization_gpu.unwrap_or(0)
            )
        })
    }
}

pub struct GpuMonitor;

impl GpuMonitor {
    pub fn detect() -> Result<Vec<GpuInfo>> {
        detect_inner()
    }

    pub fn refresh() -> Result<Vec<GpuInfo>> {
        detect_inner()
    }
}

#[cfg(not(target_os = "macos"))]
fn detect_inner() -> Result<Vec<GpuInfo>> {
    match try_nvml() {
        Ok(gpus) if !gpus.is_empty() => Ok(gpus),
        Ok(_) => Ok(try_nvidia_smi().unwrap_or_default()),
        Err(e) => {
            tracing::debug!("NVML unavailable: {e}");
            Ok(try_nvidia_smi().unwrap_or_default())
        }
    }
}

#[cfg(target_os = "macos")]
fn detect_inner() -> Result<Vec<GpuInfo>> {
    Ok(Vec::new())
}

#[cfg(not(target_os = "macos"))]
fn try_nvml() -> Result<Vec<GpuInfo>> {
    use nvml_wrapper::enum_wrappers::device::TemperatureSensor;
    use nvml_wrapper::Nvml;

    let nvml = Nvml::init().map_err(|e| GpuMeshError::Gpu(e.to_string()))?;
    let count = nvml
        .device_count()
        .map_err(|e| GpuMeshError::Gpu(e.to_string()))?;
    let driver = nvml.sys_driver_version().ok();
    let cuda = nvml.sys_cuda_driver_version().ok().map(|v| {
        let major = v / 1000;
        let minor = (v % 1000) / 10;
        format!("{major}.{minor}")
    });

    let mut out = Vec::new();
    for i in 0..count {
        let device = nvml
            .device_by_index(i)
            .map_err(|e| GpuMeshError::Gpu(e.to_string()))?;
        let name = device.name().unwrap_or_else(|_| format!("GPU-{i}"));
        let uuid = device.uuid().ok();
        let mem = device
            .memory_info()
            .map_err(|e| GpuMeshError::Gpu(e.to_string()))?;
        let util = device.utilization_rates().ok();
        let temp = device.temperature(TemperatureSensor::Gpu).ok();
        let power = device.power_usage().ok().map(|mw| mw / 1000);
        let cc = device.cuda_compute_capability().ok().map(|c| {
            format!("{}.{}", c.major, c.minor)
        });

        out.push(GpuInfo {
            index: i,
            name,
            uuid,
            vram_total_mb: mem.total / (1024 * 1024),
            vram_used_mb: mem.used / (1024 * 1024),
            vram_free_mb: mem.free / (1024 * 1024),
            utilization_gpu: util.as_ref().map(|u| u.gpu),
            utilization_mem: util.as_ref().map(|u| u.memory),
            temperature_c: temp,
            power_watts: power,
            driver_version: driver.clone(),
            cuda_version: cuda.clone(),
            compute_capability: cc,
        });
    }
    Ok(out)
}

fn try_nvidia_smi() -> Result<Vec<GpuInfo>> {
    let output = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=index,name,uuid,memory.total,memory.used,memory.free,utilization.gpu,temperature.gpu,power.draw,driver_version",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .map_err(|e| GpuMeshError::Gpu(e.to_string()))?;
    if !output.status.success() {
        return Err(GpuMeshError::Gpu("nvidia-smi failed".into()));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut gpus = Vec::new();
    for line in text.lines() {
        let cols: Vec<_> = line.split(',').map(|s| s.trim()).collect();
        if cols.len() < 10 {
            continue;
        }
        let index: u32 = cols[0].parse().unwrap_or(0);
        let total: u64 = cols[3].parse().unwrap_or(0);
        let used: u64 = cols[4].parse().unwrap_or(0);
        let free: u64 = cols[5].parse().unwrap_or(0);
        gpus.push(GpuInfo {
            index,
            name: cols[1].to_string(),
            uuid: Some(cols[2].to_string()),
            vram_total_mb: total,
            vram_used_mb: used,
            vram_free_mb: free,
            utilization_gpu: cols[6].parse().ok(),
            utilization_mem: None,
            temperature_c: cols[7].parse().ok(),
            power_watts: cols[8].parse::<f64>().ok().map(|p| p as u32),
            driver_version: Some(cols[9].to_string()),
            cuda_version: None,
            compute_capability: None,
        });
    }
    Ok(gpus)
}
