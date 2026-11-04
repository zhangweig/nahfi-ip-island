//! Low-battery reminders latch per device until a valid recovery is observed.
use crate::bt::{Device, DeviceKind};
use std::collections::HashSet;
#[derive(Default)]
pub struct Monitor {
    warned: HashSet<String>,
}
impl Monitor {
    pub fn update(
        &mut self,
        devices: &[Device],
        enabled: bool,
        threshold: u8,
    ) -> Vec<(String, u8)> {
        if !enabled {
            self.warned.clear();
            return vec![];
        }
        let threshold = threshold.clamp(5, 50);
        let mut alerts = vec![];
        for device in devices
            .iter()
            .filter(|d| matches!(d.kind, DeviceKind::Keyboard | DeviceKind::Mouse))
        {
            let Some(level) = device.battery.filter(|v| *v <= 100) else {
                continue;
            };
            if level > threshold.saturating_add(5) {
                self.warned.remove(&device.id);
            } else if level <= threshold && self.warned.insert(device.id.clone()) {
                alerts.push((device.name.clone(), level));
            }
        }
        // Missing/unknown samples do not reset the latch (sleep would otherwise spam).
        alerts
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn device(level: Option<u8>) -> Device {
        Device {
            id: "mouse".into(),
            name: "Mouse".into(),
            battery: level,
            kind: DeviceKind::Mouse,
        }
    }
    #[test]
    fn low_battery_is_once_until_recovery_and_unknown_never_means_zero() {
        let mut m = Monitor::default();
        assert_eq!(m.update(&[device(Some(19))], true, 20).len(), 1);
        assert!(m.update(&[device(Some(18))], true, 20).is_empty());
        m.update(&[], true, 20);
        m.update(&[device(None)], true, 20);
        assert!(m.update(&[device(Some(17))], true, 20).is_empty());
        m.update(&[device(Some(30))], true, 20);
        assert_eq!(m.update(&[device(Some(15))], true, 20).len(), 1);
        assert!(m.update(&[device(None)], true, 20).is_empty());
        assert!(m.update(&[device(Some(10))], false, 20).is_empty());
    }
}
