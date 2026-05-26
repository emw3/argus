use anyhow::Result;
use std::ffi::CString;
use std::time::Instant;

/// System-wide metrics snapshot returned after each poll.
#[derive(Debug, Clone, Default)]
pub struct SystemMetrics {
    pub cpu_percent: f64,
    pub mem_percent: f64,
    pub disk_percent: f64,
    /// Combined rx+tx bytes per second across all non-loopback interfaces.
    pub net_total_bytes_sec: u64,
}

// ---------------------------------------------------------------------------
// Internal snapshots for delta calculations
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
struct CpuSnapshot {
    idle: u64,
    total: u64,
}

#[derive(Debug, Clone, Default)]
struct NetSnapshot {
    bytes: u64,
    at: Option<Instant>,
}

pub struct MetricsCollector {
    prev_cpu: Option<CpuSnapshot>,
    prev_net: Option<NetSnapshot>,
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            prev_cpu: None,
            prev_net: None,
        }
    }

    pub fn collect(&mut self) -> Result<SystemMetrics> {
        let cpu_percent = self.read_cpu()?;
        let mem_percent = read_mem()?;
        let disk_percent = read_disk()?;
        let net_total_bytes_sec = self.read_net()?;

        Ok(SystemMetrics {
            cpu_percent,
            mem_percent,
            disk_percent,
            net_total_bytes_sec,
        })
    }

    // -----------------------------------------------------------------------
    // CPU: /proc/stat
    // -----------------------------------------------------------------------

    fn read_cpu(&mut self) -> Result<f64> {
        let contents = std::fs::read_to_string("/proc/stat")?;
        let cpu_line = contents
            .lines()
            .next()
            .ok_or_else(|| anyhow::anyhow!("/proc/stat empty"))?;

        // Fields: user nice system idle iowait irq softirq steal guest guest_nice
        let fields: Vec<u64> = cpu_line
            .split_whitespace()
            .skip(1) // skip the "cpu" label
            .filter_map(|s| s.parse().ok())
            .collect();

        if fields.len() < 4 {
            return Ok(0.0);
        }

        // idle = idle + iowait (fields 3 and 4, 0-indexed)
        let idle: u64 = fields[3] + fields.get(4).copied().unwrap_or(0);
        let total: u64 = fields.iter().sum();

        let snapshot = CpuSnapshot { idle, total };

        let percent = match &self.prev_cpu {
            None => 0.0,
            Some(prev) => {
                let total_delta = total.saturating_sub(prev.total);
                let idle_delta = idle.saturating_sub(prev.idle);
                if total_delta == 0 {
                    0.0
                } else {
                    (1.0 - idle_delta as f64 / total_delta as f64) * 100.0
                }
            }
        };

        self.prev_cpu = Some(snapshot);
        Ok(percent.clamp(0.0, 100.0))
    }

    // -----------------------------------------------------------------------
    // Network: /proc/net/dev
    // -----------------------------------------------------------------------

    fn read_net(&mut self) -> Result<u64> {
        let contents = std::fs::read_to_string("/proc/net/dev")?;
        let now = Instant::now();

        let mut total_bytes: u64 = 0;
        for line in contents.lines().skip(2) {
            // Format: "  eth0:  rx_bytes  rx_pkts ... tx_bytes ..."
            let line = line.trim();
            let colon = match line.find(':') {
                Some(c) => c,
                None => continue,
            };
            let iface = line[..colon].trim();
            // Skip loopback
            if iface == "lo" {
                continue;
            }
            let fields: Vec<u64> = line[colon + 1..]
                .split_whitespace()
                .filter_map(|s| s.parse().ok())
                .collect();
            if fields.len() < 9 {
                continue;
            }
            // fields[0] = rx_bytes, fields[8] = tx_bytes
            total_bytes += fields[0] + fields[8];
        }

        let bytes_sec = match &self.prev_net {
            None => 0,
            Some(prev) => {
                if let Some(prev_at) = prev.at {
                    let elapsed = now.duration_since(prev_at).as_secs_f64();
                    if elapsed > 0.0 {
                        let delta = total_bytes.saturating_sub(prev.bytes);
                        (delta as f64 / elapsed) as u64
                    } else {
                        0
                    }
                } else {
                    0
                }
            }
        };

        self.prev_net = Some(NetSnapshot {
            bytes: total_bytes,
            at: Some(now),
        });

        Ok(bytes_sec)
    }
}

// ---------------------------------------------------------------------------
// Memory: /proc/meminfo
// ---------------------------------------------------------------------------

fn read_mem() -> Result<f64> {
    let contents = std::fs::read_to_string("/proc/meminfo")?;
    let mut total: Option<u64> = None;
    let mut available: Option<u64> = None;

    for line in contents.lines() {
        if line.starts_with("MemTotal:") {
            total = parse_kb_line(line);
        } else if line.starts_with("MemAvailable:") {
            available = parse_kb_line(line);
        }
        if total.is_some() && available.is_some() {
            break;
        }
    }

    match (total, available) {
        (Some(t), Some(a)) if t > 0 => {
            let used = t.saturating_sub(a);
            Ok((used as f64 / t as f64 * 100.0).clamp(0.0, 100.0))
        }
        _ => Ok(0.0),
    }
}

fn parse_kb_line(line: &str) -> Option<u64> {
    line.split_whitespace().nth(1).and_then(|s| s.parse().ok())
}

// ---------------------------------------------------------------------------
// Disk: libc::statvfs("/")
// ---------------------------------------------------------------------------

fn read_disk() -> Result<f64> {
    use std::mem::MaybeUninit;

    let path = CString::new("/").expect("CString for /");
    let mut stat: MaybeUninit<libc::statvfs> = MaybeUninit::uninit();

    let ret = unsafe { libc::statvfs(path.as_ptr(), stat.as_mut_ptr()) };
    if ret != 0 {
        return Ok(0.0);
    }
    let stat = unsafe { stat.assume_init() };

    let total = stat.f_blocks as f64 * stat.f_frsize as f64;
    let avail = stat.f_bavail as f64 * stat.f_frsize as f64;
    if total == 0.0 {
        return Ok(0.0);
    }
    let used = total - avail;
    Ok((used / total * 100.0).clamp(0.0, 100.0))
}

// ---------------------------------------------------------------------------
// Uptime: /proc/uptime
