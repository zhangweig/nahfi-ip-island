//! Lightweight background host metrics; no process list or personal file paths.
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::mpsc,
    time::{Duration, Instant, SystemTime},
};

#[derive(Clone, Default)]
pub struct Disk {
    pub label: String,
    pub total: u64,
    pub available: u64,
}
#[derive(Clone, Default)]
pub struct Snapshot {
    pub cpu: Option<f32>,
    pub used_memory: u64,
    pub total_memory: u64,
    pub down: Option<f64>,
    pub up: Option<f64>,
    pub disks: Vec<Disk>,
    pub sampled_at: Option<SystemTime>,
    pub route_hash: u64,
    pub system_proxy: Option<String>,
}
pub fn rate(before: u64, after: u64, seconds: f64) -> Option<f64> {
    (seconds.is_finite() && seconds > 0.0 && after >= before)
        .then(|| (after - before) as f64 / seconds)
}
pub fn percent(used: u64, total: u64) -> Option<f32> {
    (total > 0).then(|| (used.min(total) as f64 * 100.0 / total as f64) as f32)
}
pub fn bytes(value: f64) -> String {
    if !value.is_finite() || value < 0.0 {
        return "--".into();
    }
    if value >= 1073741824.0 {
        format!("{:.1} GB", value / 1073741824.0)
    } else if value >= 1048576.0 {
        format!("{:.1} MB", value / 1048576.0)
    } else if value >= 1024.0 {
        format!("{:.1} KB", value / 1024.0)
    } else {
        format!("{value:.0} B")
    }
}
struct Sampler {
    system: sysinfo::System,
    networks: sysinfo::Networks,
    disks: sysinfo::Disks,
    last: Option<(u64, u64, Instant, u64)>,
    disk_at: Instant,
}
impl Sampler {
    fn new() -> Self {
        let mut system = sysinfo::System::new();
        system.refresh_cpu_usage();
        Self {
            system,
            networks: sysinfo::Networks::new_with_refreshed_list(),
            disks: sysinfo::Disks::new_with_refreshed_list(),
            last: None,
            disk_at: Instant::now(),
        }
    }
    fn sample(&mut self) -> Snapshot {
        self.system.refresh_cpu_usage();
        self.system.refresh_memory();
        self.networks.refresh(true);
        if self.disk_at.elapsed() >= Duration::from_secs(30) {
            self.disks.refresh(true);
            self.disk_at = Instant::now();
        }
        let mut interfaces: Vec<_> = self
            .networks
            .iter()
            .map(|(name, data)| (name.to_owned(), format!("{:?}", data.ip_networks())))
            .collect();
        interfaces.sort();
        let proxy = crate::ip_monitor::system_proxy();
        let mut hash = DefaultHasher::new();
        interfaces.hash(&mut hash);
        proxy.hash(&mut hash);
        let route_hash = hash.finish();
        let (received, sent) = self.networks.iter().fold((0u64, 0u64), |(rx, tx), (_, d)| {
            (
                rx.saturating_add(d.total_received()),
                tx.saturating_add(d.total_transmitted()),
            )
        });
        let now = Instant::now();
        let (down, up) = self
            .last
            .filter(|old| old.3 == route_hash)
            .map(|old| {
                (
                    rate(old.0, received, now.duration_since(old.2).as_secs_f64()),
                    rate(old.1, sent, now.duration_since(old.2).as_secs_f64()),
                )
            })
            .unwrap_or((None, None));
        let cpu = self
            .last
            .map(|_| self.system.global_cpu_usage().clamp(0.0, 100.0));
        self.last = Some((received, sent, now, route_hash));
        Snapshot {
            cpu,
            used_memory: self.system.used_memory(),
            total_memory: self.system.total_memory(),
            down,
            up,
            disks: self
                .disks
                .iter()
                .filter(|d| d.total_space() > 0)
                .map(|d| Disk {
                    label: d.mount_point().to_string_lossy().into(),
                    total: d.total_space(),
                    available: d.available_space(),
                })
                .collect(),
            sampled_at: Some(SystemTime::now()),
            route_hash,
            system_proxy: proxy,
        }
    }
}
pub fn spawn(ctx: eframe::egui::Context) -> mpsc::Receiver<Snapshot> {
    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut sampler = Sampler::new();
        loop {
            let sample = sampler.sample();
            match tx.try_send(sample) {
                Err(mpsc::TrySendError::Disconnected(_)) => break,
                _ => ctx.request_repaint(),
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    });
    rx
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metric_math_handles_counter_reset_missing_memory_and_units() {
        assert_eq!(rate(100, 300, 2.0), Some(100.0));
        assert_eq!(rate(300, 100, 2.0), None);
        assert_eq!(rate(0, 1, 0.0), None);
        assert_eq!(percent(5, 10), Some(50.0));
        assert_eq!(percent(5, 0), None);
        assert_eq!(bytes(1048576.0), "1.0 MB");
    }
    #[test]
    fn samples_actual_host_memory_and_storage() {
        let mut s = Sampler::new();
        let sample = s.sample();
        assert!(sample.total_memory > 0);
        assert!(sample.used_memory <= sample.total_memory);
        assert!(sample.sampled_at.is_some());
        for d in sample.disks {
            assert!(d.available <= d.total);
        }
        assert_eq!(sample.cpu, None);
    }
}
