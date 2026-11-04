//! Receiver-specific, read-only battery queries on vendor HID collections.
//! Protocol references and MIT notices: docs/USB_BATTERY.md.
use crate::bt::{Device, DeviceKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Model {
    K75Lite,
    VxeR1,
}
fn model(vid: u16, pid: u16, interface: i32, page: u16, usage: u16) -> Option<Model> {
    if interface != 1 || page != 0xff02 || usage != 2 {
        return None;
    }
    match (vid, pid) {
        (0x3554, 0xfa09) => Some(Model::K75Lite),
        (0x3554, 0xf58a) | (0x373b, 0x1085) => Some(Model::VxeR1),
        _ => None,
    }
}
fn k75_request(command: u8, subcommand: u8) -> [u8; 20] {
    let mut r = [0u8; 20];
    r[0] = 0x13;
    r[1] = command;
    r[2] = 1;
    r[4] = subcommand;
    r[19] = r[..19].iter().fold(0u8, |a, b| a.wrapping_add(*b));
    r
}
fn vxe_request(command: u8) -> [u8; 17] {
    let mut r = [0u8; 17];
    r[0] = 8;
    r[1] = command;
    if command == 1 {
        r[5] = 8;
        for i in 0..8 {
            r[6 + i] = i as u8 + 1;
        }
    }
    r[16] = vxe_checksum(&r);
    r
}
fn vxe_checksum(r: &[u8]) -> u8 {
    0x55u8.wrapping_sub(r[..16].iter().fold(0u8, |a, b| a.wrapping_add(*b)))
}
fn vxe_response(r: &[u8], command: u8) -> bool {
    r.len() == 17
        && r[0] == 8
        && r[1] == command
        && r[2] == 0
        && r[5] <= 10
        && r[16] == vxe_checksum(r)
}
fn power(r: &[u8]) -> Option<u8> {
    (r.len() >= 2 && r[0] <= 100 && r[1] <= 1).then(|| r[0])
}
// Bound multipart reports and require one consistent, complete response.
#[derive(Default)]
struct K75Response {
    count: Option<usize>,
    packets: [Option<Vec<u8>>; 4],
}
impl K75Response {
    fn push(&mut self, r: &[u8], command: u8, wanted: usize) -> Option<Vec<u8>> {
        if r.len() != 20 || r[0] != 0x13 || r[1] != command {
            return None;
        }
        let count = (r[2] & 0x7f) as usize;
        let index = r[3] as usize;
        let len = (r[4] & 15) as usize;
        if !(1..=4).contains(&count)
            || index >= count
            || len > 14
            || self.count.is_some_and(|c| c != count)
        {
            return None;
        }
        self.count = Some(count);
        self.packets[index] = Some(r[5..5 + len].to_vec());
        let mut payload = Vec::new();
        for p in &self.packets[..count] {
            payload.extend_from_slice(p.as_ref()?);
        }
        (payload.len() >= wanted).then(|| payload[..wanted].to_vec())
    }
}

#[cfg(windows)]
fn k75_command(h: &hidapi::HidDevice, command: u8, sub: u8, wanted: usize) -> Option<Vec<u8>> {
    use std::time::{Duration, Instant};
    let request = k75_request(command, sub);
    if h.write(&request).ok()? != request.len() {
        return None;
    }
    let until = Instant::now() + Duration::from_millis(350);
    let mut reply = K75Response::default();
    while Instant::now() < until {
        let mut bytes = [0u8; 64];
        let n = h.read_timeout(&mut bytes, 50).ok()?;
        if let Some(data) = reply.push(&bytes[..n], command, wanted) {
            return Some(data);
        }
    }
    None
}
#[cfg(windows)]
fn vxe_command(h: &hidapi::HidDevice, command: u8) -> Option<[u8; 17]> {
    use std::time::{Duration, Instant};
    let request = vxe_request(command);
    if h.write(&request).ok()? != request.len() {
        return None;
    }
    let until = Instant::now() + Duration::from_millis(500);
    while Instant::now() < until {
        let mut bytes = [0u8; 64];
        let n = h.read_timeout(&mut bytes, 50).ok()?;
        if vxe_response(&bytes[..n], command) {
            return bytes[..17].try_into().ok();
        }
    }
    None
}
#[cfg(windows)]
fn read(h: &hidapi::HidDevice, model: Model) -> Option<u8> {
    match model {
        Model::K75Lite => {
            // Exact identity prevents claiming another product sharing the receiver ID.
            if k75_command(h, 5, 6, 6)? != [3, 0, 0, 0, 1, 0x44] {
                return None;
            }
            if k75_command(h, 7, 0, 1)?[0] == 0 {
                return None;
            }
            power(&k75_command(h, 0x4a, 0, 2)?)
        }
        Model::VxeR1 => {
            let online = vxe_command(h, 3)?;
            if online[5] < 1 || online[6] == 0 {
                return None;
            }
            vxe_command(h, 1)?;
            let r = vxe_command(h, 4)?;
            if r[5] < 2 {
                return None;
            }
            power(&r[6..8])
        }
    }
}

#[cfg(windows)]
fn query() -> Vec<Device> {
    let Ok(api) = hidapi::HidApi::new() else {
        return vec![];
    };
    let mut devices = Vec::new();
    for d in api.device_list() {
        let Some(model) = model(
            d.vendor_id(),
            d.product_id(),
            d.interface_number(),
            d.usage_page(),
            d.usage(),
        ) else {
            continue;
        };
        let name = match model {
            Model::K75Lite => "KZZI K75 Lite · USB 2.4G",
            Model::VxeR1 => "VXE R1 系列 · USB 2.4G",
        };
        let battery = match d.open_device(&api) {
            Ok(h) => {
                if model == Model::K75Lite {
                    if let Some(identity) = k75_command(&h, 5, 6, 6) {
                        if identity != [3, 0, 0, 0, 1, 0x44] {
                            continue;
                        }
                    }
                }
                read(&h, model)
            }
            Err(_) => None,
        };
        // Receiver presence is not proof the wireless peripheral is awake.
        devices.push(Device {
            id: format!("usb:{}", d.path().to_string_lossy()),
            name: name.into(),
            battery,
            kind: if model == Model::K75Lite {
                DeviceKind::Keyboard
            } else {
                DeviceKind::Mouse
            },
        });
    }
    devices
}

pub fn spawn(ctx: eframe::egui::Context) -> std::sync::mpsc::Receiver<Vec<Device>> {
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    #[cfg(windows)]
    std::thread::spawn(move || loop {
        let devices = query();
        match tx.try_send(devices) {
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => break,
            _ => ctx.request_repaint(),
        }
        std::thread::sleep(std::time::Duration::from_secs(15));
    });
    #[cfg(not(windows))]
    {
        let _ = (tx, ctx);
    }
    rx
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_known_vendor_collections_are_probed() {
        assert_eq!(model(0x3554, 0xfa09, 1, 0xff02, 2), Some(Model::K75Lite));
        assert_eq!(model(0x3554, 0xf58a, 1, 0xff02, 2), Some(Model::VxeR1));
        assert_eq!(model(0x373b, 0x1085, 1, 0xff02, 2), Some(Model::VxeR1));
        assert_eq!(model(0x3554, 0xf58a, 0, 1, 2), None);
        assert_eq!(model(0x3554, 0x1234, 1, 0xff02, 2), None);
    }
    #[test]
    fn requests_match_documented_read_commands() {
        assert_eq!(
            k75_request(5, 6),
            [0x13, 5, 1, 0, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x1f]
        );
        assert_eq!(
            vxe_request(4),
            [8, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x49]
        );
        assert_eq!(vxe_request(1)[16], 0x20);
    }
    #[test]
    fn invalid_or_sleeping_power_is_never_zero_percent() {
        assert_eq!(power(&[85, 0]), Some(85));
        assert_eq!(power(&[0, 1]), Some(0));
        assert_eq!(power(&[]), None);
        assert_eq!(power(&[255, 0]), None);
        assert_eq!(power(&[60, 3]), None);
        let mut r = vxe_request(4);
        r[5] = 2;
        r[6] = 95;
        r[16] = vxe_checksum(&r);
        assert!(vxe_response(&r, 4));
        r[6] = 94;
        assert!(!vxe_response(&r, 4));
        assert!(!vxe_response(&r[..8], 4));
    }
    #[test]
    fn keyboard_multipart_is_ordered_and_bounded() {
        let mut response = K75Response::default();
        let mut r = [0u8; 20];
        r[0] = 0x13;
        r[1] = 5;
        r[2] = 2;
        r[3] = 1;
        r[4] = 3;
        r[5..8].copy_from_slice(&[0, 1, 0x44]);
        assert!(response.push(&r, 5, 6).is_none());
        r[3] = 0;
        r[5..8].copy_from_slice(&[3, 0, 0]);
        assert_eq!(response.push(&r, 5, 6).unwrap(), [3, 0, 0, 0, 1, 0x44]);
        let mut response = K75Response::default();
        r[2] = 127;
        assert!(response.push(&r, 5, 6).is_none());
        r[2] = 1;
        r[1] = 0x85;
        assert!(response.push(&r, 5, 6).is_none());
    }
}
