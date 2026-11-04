use crate::{
    app::{App, Env},
    computer_status::{bytes, percent},
    paint::{Card, H},
    theme::{self, W},
};
impl App {
    pub(crate) fn page_computer(&mut self, c: &Card, env: &Env) {
        let data = &env.computer;
        if data.sampled_at.is_none() {
            c.text(
                44.0,
                66.0,
                "正在采样电脑状态…",
                13.0,
                W::Semibold,
                theme::GRAY,
                H::Left,
            );
            return;
        }
        for (x, label, value) in [
            (44.0, "CPU", data.cpu),
            (220.0, "内存", percent(data.used_memory, data.total_memory)),
        ] {
            let text = value
                .map(|v| format!("{label} {v:.0}%"))
                .unwrap_or_else(|| format!("{label} --"));
            c.text(x, 52.0, &text, 13.0, W::Bold, theme::WHITE, H::Left);
            c.fill(x, 59.0, 150.0, 4.0, 2.0, theme::TRACK);
            if let Some(v) = value {
                c.fill(
                    x,
                    59.0,
                    150.0 * v / 100.0,
                    4.0,
                    2.0,
                    if v >= 90.0 {
                        theme::YELLOW
                    } else {
                        theme::CYAN
                    },
                );
            }
        }
        c.text(
            44.0,
            79.0,
            "后台采样 · 每 2 秒刷新",
            9.0,
            W::Regular,
            theme::GRAY,
            H::Left,
        );
        c.text(
            220.0,
            79.0,
            &format!(
                "{} / {}",
                bytes(data.used_memory as f64),
                bytes(data.total_memory as f64)
            ),
            9.0,
            W::Regular,
            theme::GRAY,
            H::Left,
        );
        let speed = |v: Option<f64>| {
            v.map(|v| format!("{}/s", bytes(v)))
                .unwrap_or_else(|| "--".into())
        };
        c.text(
            44.0,
            99.0,
            &format!("下载 {}", speed(data.down)),
            11.0,
            W::Semibold,
            theme::GREEN,
            H::Left,
        );
        c.text(
            220.0,
            99.0,
            &format!("上传 {}", speed(data.up)),
            11.0,
            W::Semibold,
            theme::CYAN,
            H::Left,
        );
        c.hit(43.0, 84.0, 331.0, 19.0, "computer-net")
            .on_hover_text("网卡合计速率，虚拟网卡可能重复计入流量。");
        if !data.disks.is_empty() {
            self.computer_disk %= data.disks.len();
            let disk = &data.disks[self.computer_disk];
            let text = c.fit(
                &format!(
                    "{} 可用 {} / {}",
                    disk.label,
                    bytes(disk.available as f64),
                    bytes(disk.total as f64)
                ),
                9.0,
                W::Regular,
                270.0,
            );
            c.text(44.0, 121.0, &text, 9.0, W::Regular, theme::GRAY, H::Left);
            if data.disks.len() > 1 {
                c.text(
                    375.0,
                    121.0,
                    "下一盘",
                    10.0,
                    W::Semibold,
                    theme::CYAN,
                    H::Right,
                );
                if c.hit(324.0, 106.0, 53.0, 22.0, "disk-next").clicked() {
                    self.computer_disk = (self.computer_disk + 1) % data.disks.len();
                }
            }
        }
    }
}
