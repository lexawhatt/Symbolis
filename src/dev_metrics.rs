use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use crate::media_library::media_root;

const STORAGE_SCAN_LIMIT: usize = 20_000;

#[derive(Clone, Debug, Default)]
pub(crate) struct DevMetricsSnapshot {
    pub(crate) system_cpu_percent: Option<f32>,
    pub(crate) process_cpu_percent: Option<f32>,
    pub(crate) system_memory: Option<SystemMemory>,
    pub(crate) process_memory: Option<ProcessMemory>,
    pub(crate) gpu: Vec<GpuMetric>,
    pub(crate) media_storage: Option<StorageMetric>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SystemMemory {
    pub(crate) total_bytes: u64,
    pub(crate) available_bytes: u64,
}

impl SystemMemory {
    pub(crate) fn used_bytes(self) -> u64 {
        self.total_bytes.saturating_sub(self.available_bytes)
    }

    pub(crate) fn used_percent(self) -> f32 {
        percent(self.used_bytes(), self.total_bytes)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ProcessMemory {
    pub(crate) resident_bytes: u64,
    pub(crate) virtual_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GpuMetric {
    pub(crate) label: String,
    pub(crate) usage_percent: Option<u64>,
    pub(crate) vram_used_bytes: Option<u64>,
    pub(crate) vram_total_bytes: Option<u64>,
}

impl GpuMetric {
    pub(crate) fn vram_used_percent(&self) -> Option<f32> {
        Some(percent(self.vram_used_bytes?, self.vram_total_bytes?))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct StorageMetric {
    pub(crate) bytes: u64,
    pub(crate) truncated: bool,
}

#[derive(Default)]
pub(crate) struct DevMetricsSampler {
    previous_cpu: Option<CpuTimes>,
    previous_process_jiffies: Option<u64>,
    last_snapshot: DevMetricsSnapshot,
    last_refresh: Option<Instant>,
}

impl DevMetricsSampler {
    pub(crate) fn snapshot(&mut self) -> &DevMetricsSnapshot {
        self.refresh_if_due(Duration::from_secs(1))
    }

    fn refresh_if_due(&mut self, interval: Duration) -> &DevMetricsSnapshot {
        let now = Instant::now();
        if self
            .last_refresh
            .is_none_or(|last_refresh| now.duration_since(last_refresh) >= interval)
        {
            self.last_snapshot = self.sample();
            self.last_refresh = Some(now);
        }
        &self.last_snapshot
    }

    fn sample(&mut self) -> DevMetricsSnapshot {
        let cpu = read_cpu_times();
        let process_jiffies = read_process_jiffies();
        let cpu_count = read_cpu_count().max(1);
        let system_cpu_percent = self
            .previous_cpu
            .zip(cpu)
            .and_then(|(previous, current)| system_cpu_percent(previous, current));
        let process_cpu_percent = self
            .previous_cpu
            .zip(cpu)
            .zip(self.previous_process_jiffies.zip(process_jiffies))
            .and_then(
                |((previous_cpu, current_cpu), (previous_process, current_process))| {
                    process_cpu_percent(
                        previous_cpu,
                        current_cpu,
                        previous_process,
                        current_process,
                        cpu_count,
                    )
                },
            );

        self.previous_cpu = cpu;
        self.previous_process_jiffies = process_jiffies;

        DevMetricsSnapshot {
            system_cpu_percent,
            process_cpu_percent,
            system_memory: read_system_memory(),
            process_memory: read_process_memory(),
            gpu: read_gpu_metrics(),
            media_storage: media_root().map(|root| directory_size_limited(&root)),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CpuTimes {
    total: u64,
    idle: u64,
}

fn read_cpu_times() -> Option<CpuTimes> {
    parse_cpu_times(&fs::read_to_string("/proc/stat").ok()?)
}

fn parse_cpu_times(content: &str) -> Option<CpuTimes> {
    let line = content.lines().find(|line| line.starts_with("cpu "))?;
    let values = line
        .split_whitespace()
        .skip(1)
        .filter_map(|value| value.parse::<u64>().ok())
        .collect::<Vec<_>>();
    if values.len() < 5 {
        return None;
    }

    let idle =
        values.get(3).copied().unwrap_or_default() + values.get(4).copied().unwrap_or_default();
    let total = values.iter().sum();
    Some(CpuTimes { total, idle })
}

fn read_cpu_count() -> usize {
    fs::read_to_string("/proc/stat")
        .ok()
        .map(|content| {
            content
                .lines()
                .filter(|line| {
                    line.strip_prefix("cpu")
                        .and_then(|rest| rest.chars().next())
                        .is_some_and(|ch| ch.is_ascii_digit())
                })
                .count()
        })
        .unwrap_or(1)
}

fn system_cpu_percent(previous: CpuTimes, current: CpuTimes) -> Option<f32> {
    let total_delta = current.total.checked_sub(previous.total)?;
    if total_delta == 0 {
        return None;
    }
    let idle_delta = current.idle.saturating_sub(previous.idle);
    Some(percent(total_delta.saturating_sub(idle_delta), total_delta))
}

fn process_cpu_percent(
    previous_cpu: CpuTimes,
    current_cpu: CpuTimes,
    previous_process_jiffies: u64,
    current_process_jiffies: u64,
    cpu_count: usize,
) -> Option<f32> {
    let total_delta = current_cpu.total.checked_sub(previous_cpu.total)?;
    if total_delta == 0 {
        return None;
    }
    let process_delta = current_process_jiffies.saturating_sub(previous_process_jiffies);
    Some((process_delta as f32 / total_delta as f32) * cpu_count as f32 * 100.0)
}

fn read_process_jiffies() -> Option<u64> {
    parse_process_jiffies(&fs::read_to_string("/proc/self/stat").ok()?)
}

fn parse_process_jiffies(content: &str) -> Option<u64> {
    let after_command = content.rsplit_once(") ")?.1;
    let fields = after_command.split_whitespace().collect::<Vec<_>>();
    let user = fields.get(11)?.parse::<u64>().ok()?;
    let system = fields.get(12)?.parse::<u64>().ok()?;
    Some(user + system)
}

fn read_system_memory() -> Option<SystemMemory> {
    parse_system_memory(&fs::read_to_string("/proc/meminfo").ok()?)
}

fn parse_system_memory(content: &str) -> Option<SystemMemory> {
    let total_bytes = parse_meminfo_bytes(content, "MemTotal:")?;
    let available_bytes = parse_meminfo_bytes(content, "MemAvailable:")?;
    Some(SystemMemory {
        total_bytes,
        available_bytes,
    })
}

fn read_process_memory() -> Option<ProcessMemory> {
    parse_process_memory(&fs::read_to_string("/proc/self/status").ok()?)
}

fn parse_process_memory(content: &str) -> Option<ProcessMemory> {
    Some(ProcessMemory {
        resident_bytes: parse_meminfo_bytes(content, "VmRSS:")?,
        virtual_bytes: parse_meminfo_bytes(content, "VmSize:")?,
    })
}

fn parse_meminfo_bytes(content: &str, key: &str) -> Option<u64> {
    let value_kib = content
        .lines()
        .find_map(|line| line.strip_prefix(key))
        .and_then(|line| line.split_whitespace().next())
        .and_then(|value| value.parse::<u64>().ok())?;
    Some(value_kib * 1024)
}

fn read_gpu_metrics() -> Vec<GpuMetric> {
    let Ok(entries) = fs::read_dir("/sys/class/drm") else {
        return Vec::new();
    };
    let mut metrics = entries
        .flatten()
        .filter_map(|entry| gpu_metric_from_drm_card(&entry.path()))
        .collect::<Vec<_>>();
    metrics.sort_by(|a, b| a.label.cmp(&b.label));
    metrics
}

fn gpu_metric_from_drm_card(path: &Path) -> Option<GpuMetric> {
    let card = path.file_name()?.to_str()?;
    if !is_primary_drm_card(card) {
        return None;
    }

    let device = path.join("device");
    if !device.is_dir() {
        return None;
    }

    let usage_percent = read_sysfs_u64(&device.join("gpu_busy_percent"));
    let vram_used_bytes = read_sysfs_u64(&device.join("mem_info_vram_used"));
    let vram_total_bytes = read_sysfs_u64(&device.join("mem_info_vram_total"));
    if usage_percent.is_none() && vram_used_bytes.is_none() && vram_total_bytes.is_none() {
        return None;
    }

    Some(GpuMetric {
        label: gpu_label(card, &device),
        usage_percent,
        vram_used_bytes,
        vram_total_bytes,
    })
}

fn is_primary_drm_card(name: &str) -> bool {
    name.strip_prefix("card")
        .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|ch| ch.is_ascii_digit()))
}

fn gpu_label(card: &str, device: &Path) -> String {
    let vendor = fs::read_to_string(device.join("vendor"))
        .ok()
        .map(|value| match value.trim() {
            "0x1002" => "AMD",
            "0x10de" => "NVIDIA",
            "0x8086" => "Intel",
            _ => "GPU",
        })
        .unwrap_or("GPU");
    format!("{vendor} {card}")
}

fn read_sysfs_u64(path: &Path) -> Option<u64> {
    fs::read_to_string(path)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
}

fn directory_size_limited(root: &Path) -> StorageMetric {
    let mut stack = vec![PathBuf::from(root)];
    let mut bytes = 0_u64;
    let mut visited = 0_usize;

    while let Some(path) = stack.pop() {
        if visited >= STORAGE_SCAN_LIMIT {
            return StorageMetric {
                bytes,
                truncated: true,
            };
        }
        visited += 1;

        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.is_file() {
            bytes = bytes.saturating_add(metadata.len());
        } else if metadata.is_dir()
            && let Ok(entries) = fs::read_dir(&path)
        {
            stack.extend(entries.flatten().map(|entry| entry.path()));
        }
    }

    StorageMetric {
        bytes,
        truncated: false,
    }
}

fn percent(used: u64, total: u64) -> f32 {
    if total == 0 {
        0.0
    } else {
        (used as f32 / total as f32 * 100.0).clamp(0.0, 100.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cpu_times_from_proc_stat() {
        let times = parse_cpu_times("cpu  100 2 30 800 20 0 0 0 0 0\ncpu0 1 2 3").unwrap();
        assert_eq!(times.idle, 820);
        assert_eq!(times.total, 952);
    }

    #[test]
    fn parses_process_jiffies_with_spaced_command_name() {
        let stat = "42 (symbolis dev) S 1 2 3 4 5 6 7 8 9 10 99 77 0 0";
        assert_eq!(parse_process_jiffies(stat), Some(176));
    }

    #[test]
    fn parses_meminfo_values_as_bytes() {
        let content = "MemTotal:       1000 kB\nMemAvailable:    250 kB\nVmRSS:            12 kB\nVmSize:           30 kB\n";
        assert_eq!(
            parse_system_memory(content),
            Some(SystemMemory {
                total_bytes: 1_024_000,
                available_bytes: 256_000,
            })
        );
        assert_eq!(
            parse_process_memory(content),
            Some(ProcessMemory {
                resident_bytes: 12_288,
                virtual_bytes: 30_720,
            })
        );
    }
}
