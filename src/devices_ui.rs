use crate::{
    app::{App, Env},
    bt::DeviceKind,
    paint::{Card, H},
    theme::{self, W},
};

impl App {
    pub(crate) fn page_devices(&mut self, c: &Card, env: &Env) {
        let devices: Vec<_> = env
            .bt
            .iter()
            .filter(|d| matches!(d.kind, DeviceKind::Keyboard | DeviceKind::Mouse))
            .collect();
        if devices.is_empty() {
            c.text(
                44.0,
                65.0,
                if env.bt_ready || env.usb_ready {
                    "未检测到支持的键鼠或 USB 接收器"
                } else {
                    "正在扫描键鼠…"
                },
                12.0,
                W::Semibold,
                theme::WHITE,
                H::Left,
            );
            c.text(
                44.0,
                88.0,
                "支持蓝牙、K75 Lite 和已适配的 VXE R1 接收器",
                10.0,
                W::Regular,
                theme::GRAY,
                H::Left,
            );
            return;
        }
        self.devices_first = self.devices_first.min((devices.len() - 1) / 2 * 2);
        for (index, device) in devices.iter().skip(self.devices_first).take(2).enumerate() {
            let y = 36.0 + index as f32 * 43.0;
            let name = c.fit(
                &format!(
                    "{} · {}",
                    if device.kind == DeviceKind::Keyboard {
                        "键盘"
                    } else {
                        "鼠标"
                    },
                    device.name
                ),
                11.0,
                W::Semibold,
                235.0,
            );
            c.text(
                44.0,
                y + 13.0,
                &name,
                11.0,
                W::Semibold,
                theme::WHITE,
                H::Left,
            );
            let level = device.battery.filter(|b| *b <= 100);
            let value = level
                .map(|b| format!("{b}%"))
                .unwrap_or_else(|| "电量未知".into());
            let color = if level.is_some_and(|b| b <= 20) {
                theme::YELLOW
            } else {
                theme::GREEN
            };
            c.text(375.0, y + 13.0, &value, 12.0, W::Bold, color, H::Right);
            c.fill(44.0, y + 21.0, 330.0, 4.0, 2.0, theme::TRACK);
            if let Some(b) = level {
                c.fill(44.0, y + 21.0, 330.0 * b as f32 / 100.0, 4.0, 2.0, color);
            }
            c.hit(42.0, y, 334.0, 29.0, ("peripheral", &device.id))
                .on_hover_text(if level.is_some() {
                    format!("{} · 电量读取成功", device.name)
                } else {
                    format!("{} · 电量未知：可能休眠、驱动占用或协议不匹配", device.name)
                });
        }
        c.text(
            44.0,
            126.0,
            "蓝牙 2 秒 / USB 15 秒 · 无有效响应则未知",
            9.0,
            W::Regular,
            theme::GRAY,
            H::Left,
        );
        if devices.len() > 2 {
            let label = format!(
                "{}/{} 下一组",
                self.devices_first / 2 + 1,
                devices.len().div_ceil(2)
            );
            c.text(
                375.0,
                126.0,
                &label,
                10.0,
                W::Semibold,
                theme::CYAN,
                H::Right,
            );
            if c.hit(291.0, 111.0, 85.0, 18.0, "peripherals-next")
                .clicked()
            {
                self.devices_first = if self.devices_first + 2 >= devices.len() {
                    0
                } else {
                    self.devices_first + 2
                };
            }
        }
    }
}
