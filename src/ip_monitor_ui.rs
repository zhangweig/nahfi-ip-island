use crate::{
    app::{App, Cmd},
    bar_layout::{self, Layout},
    paint::{Card, H},
    theme::{self, W},
};
use eframe::egui;

impl App {
    pub(crate) fn ip_settings(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.settings.ip_enabled, "启用 IP Monitor");
        ui.label("独立检测 ipwho.is 的请求出口，每次查询会向该服务发送请求。");
        ui.horizontal(|ui| {
            ui.label("本地 HTTP / mixed 代理端口（0 = 直连）");
            ui.add(egui::DragValue::new(&mut self.settings.ip_proxy_port).range(0..=65535));
        });
        ui.horizontal(|ui| {
            ui.label("刷新间隔（秒）");
            ui.add(egui::DragValue::new(&mut self.settings.ip_interval).range(15..=3600));
        });
        ui.label("例如 Clash mixed-port 7890。仅支持本机 HTTP CONNECT 代理，失败不回退直连。");
        ui.label("检测结果仅代表查询接口的出口，不代表所有网站或 Clash 节点名称。");
    }
    pub(crate) fn ip_bar(&mut self, c: &Card, cx: f32, layout: &Layout, expansion: f32) {
        let Some(x) = layout.x[bar_layout::IP] else {
            return;
        };
        let x = cx + x;
        let card = c.faded(1.0 - expansion);
        if let Some(image) = flag(self.ip.code()) {
            egui::Image::new(image)
                .tint(egui::Color32::from_white_alpha(
                    ((1.0 - expansion) * 255.0) as u8,
                ))
                .paint_at(c.ui, card.rect(x - 24.0, 9.0, 18.0, 12.0));
        } else {
            card.text(
                x - 15.0,
                21.0,
                "◎",
                14.0,
                W::Regular,
                theme::GRAY,
                H::Center,
            );
        }
        let color = if self.ip.error.is_some() {
            theme::YELLOW
        } else {
            theme::WHITE
        };
        card.text(
            x + 10.0,
            21.0,
            self.ip.code(),
            12.0,
            W::Bold,
            color,
            H::Center,
        );
        if self.ip.error.is_some() {
            card.circle(x + 25.0, 8.0, 2.0, theme::YELLOW);
        }
        if expansion < 0.5
            && c.hit(x - 26.0, 0.0, 52.0, 30.0, "ip-bar")
                .on_hover_text("IP Monitor · 点击查看出口详情")
                .clicked()
        {
            self.ip_details = true;
        }
    }
    pub(crate) fn ip_panel(&mut self, c: &Card, cmds: &mut Vec<Cmd>) {
        c.text(
            16.0,
            25.0,
            "IP Monitor",
            14.5,
            W::Bold,
            theme::WHITE,
            H::Left,
        );
        c.text(
            313.0,
            24.0,
            "设置",
            11.0,
            W::Regular,
            theme::GRAY,
            H::Center,
        );
        if c.hit(291.0, 4.0, 44.0, 26.0, "ip-settings").clicked() {
            self.settings_tab = 10;
            self.open_settings();
        }
        c.text(
            365.0,
            24.0,
            "返回",
            11.0,
            W::Regular,
            theme::GRAY,
            H::Center,
        );
        if c.hit(340.0, 4.0, 57.0, 26.0, "ip-back").clicked() {
            self.ip_details = false;
        }
        if let Some(o) = &self.ip.last_success {
            if let Some(image) = flag(&o.country_code) {
                egui::Image::new(image).paint_at(c.ui, c.rect(16.0, 40.0, 24.0, 16.0));
            }
            let country = c.fit(
                &format!("{} · {}", o.country_code, o.country),
                12.0,
                W::Semibold,
                340.0,
            );
            c.text(
                48.0,
                54.0,
                &country,
                12.0,
                W::Semibold,
                theme::WHITE,
                H::Left,
            );
            let ip = o.ip.to_string();
            c.text(16.0, 78.0, &ip, 14.0, W::Bold, theme::CYAN, H::Left);
            if c.hit(12.0, 60.0, 390.0, 22.0, "ip-copy")
                .on_hover_text("点击复制公网 IP")
                .clicked()
            {
                cmds.push(Cmd::Copy(ip));
            }
            let date: chrono::DateTime<chrono::Local> =
                self.ip.attempted_at.unwrap_or(o.checked_at).into();
            c.text(
                16.0,
                120.0,
                &format!(
                    "最近检测 {}{}",
                    date.format("%m-%d %H:%M:%S"),
                    if self.ip.error.is_some() {
                        " · 旧结果"
                    } else {
                        ""
                    }
                ),
                10.0,
                W::Regular,
                theme::GRAY,
                H::Left,
            );
        } else {
            c.text(
                16.0,
                65.0,
                if self.ip.refreshing {
                    "正在检测出口…"
                } else {
                    "暂无出口信息"
                },
                14.0,
                W::Semibold,
                theme::GRAY,
                H::Left,
            );
        }
        if self.ip.last_success.is_none() {
            if let Some(at) = self.ip.attempted_at {
                let at: chrono::DateTime<chrono::Local> = at.into();
                c.text(
                    16.0,
                    120.0,
                    &format!("最近检测 {}", at.format("%m-%d %H:%M:%S")),
                    10.0,
                    W::Regular,
                    theme::GRAY,
                    H::Left,
                );
            }
        }
        let route = crate::ip_monitor::Config::from_settings(&self.settings).route();
        let status = if self.ip.refreshing {
            format!("{route} · 检测中")
        } else if let Some(error) = &self.ip.error {
            error.clone()
        } else if self.ip.last_success.is_some() {
            format!("{route} · 查询成功")
        } else {
            format!("{route} · 等待首次检测")
        };
        let text = c.fit(&status, 10.0, W::Regular, 385.0);
        c.text(
            16.0,
            99.0,
            &text,
            10.0,
            W::Regular,
            if self.ip.error.is_some() {
                theme::YELLOW
            } else {
                theme::GRAY
            },
            H::Left,
        );
        c.text(
            368.0,
            121.0,
            if self.ip.refreshing {
                "检测中"
            } else {
                "刷新"
            },
            11.0,
            W::Semibold,
            theme::CYAN,
            H::Center,
        );
        if !self.ip.refreshing && c.hit(338.0, 103.0, 62.0, 28.0, "ip-refresh").clicked() {
            self.ip_refresh = true;
        }
        c.hit(12.0, 84.0, 395.0, 20.0, "ip-status")
            .on_hover_text(format!(
                "{status}\n仅代表 ipwho.is 查询出口，不代表所有网站。"
            ));
    }
}
// Bundled country-flag-icons 1.6.20 (MIT), see assets/flags/LICENSE.
fn flag(code: &str) -> Option<egui::ImageSource<'static>> {
    match code {
        "AC" => Some(egui::include_image!("../assets/flags/AC.svg")),
        "AD" => Some(egui::include_image!("../assets/flags/AD.svg")),
        "AE" => Some(egui::include_image!("../assets/flags/AE.svg")),
        "AF" => Some(egui::include_image!("../assets/flags/AF.svg")),
        "AG" => Some(egui::include_image!("../assets/flags/AG.svg")),
        "AI" => Some(egui::include_image!("../assets/flags/AI.svg")),
        "AL" => Some(egui::include_image!("../assets/flags/AL.svg")),
        "AM" => Some(egui::include_image!("../assets/flags/AM.svg")),
        "AO" => Some(egui::include_image!("../assets/flags/AO.svg")),
        "AQ" => Some(egui::include_image!("../assets/flags/AQ.svg")),
        "AR" => Some(egui::include_image!("../assets/flags/AR.svg")),
        "AS" => Some(egui::include_image!("../assets/flags/AS.svg")),
        "AT" => Some(egui::include_image!("../assets/flags/AT.svg")),
        "AU" => Some(egui::include_image!("../assets/flags/AU.svg")),
        "AW" => Some(egui::include_image!("../assets/flags/AW.svg")),
        "AX" => Some(egui::include_image!("../assets/flags/AX.svg")),
        "AZ" => Some(egui::include_image!("../assets/flags/AZ.svg")),
        "BA" => Some(egui::include_image!("../assets/flags/BA.svg")),
        "BB" => Some(egui::include_image!("../assets/flags/BB.svg")),
        "BD" => Some(egui::include_image!("../assets/flags/BD.svg")),
        "BE" => Some(egui::include_image!("../assets/flags/BE.svg")),
        "BF" => Some(egui::include_image!("../assets/flags/BF.svg")),
        "BG" => Some(egui::include_image!("../assets/flags/BG.svg")),
        "BH" => Some(egui::include_image!("../assets/flags/BH.svg")),
        "BI" => Some(egui::include_image!("../assets/flags/BI.svg")),
        "BJ" => Some(egui::include_image!("../assets/flags/BJ.svg")),
        "BL" => Some(egui::include_image!("../assets/flags/BL.svg")),
        "BM" => Some(egui::include_image!("../assets/flags/BM.svg")),
        "BN" => Some(egui::include_image!("../assets/flags/BN.svg")),
        "BO" => Some(egui::include_image!("../assets/flags/BO.svg")),
        "BQ-BO" => Some(egui::include_image!("../assets/flags/BQ-BO.svg")),
        "BQ-SA" => Some(egui::include_image!("../assets/flags/BQ-SA.svg")),
        "BQ-SE" => Some(egui::include_image!("../assets/flags/BQ-SE.svg")),
        "BQ" => Some(egui::include_image!("../assets/flags/BQ.svg")),
        "BR" => Some(egui::include_image!("../assets/flags/BR.svg")),
        "BS" => Some(egui::include_image!("../assets/flags/BS.svg")),
        "BT" => Some(egui::include_image!("../assets/flags/BT.svg")),
        "BV" => Some(egui::include_image!("../assets/flags/BV.svg")),
        "BW" => Some(egui::include_image!("../assets/flags/BW.svg")),
        "BY" => Some(egui::include_image!("../assets/flags/BY.svg")),
        "BZ" => Some(egui::include_image!("../assets/flags/BZ.svg")),
        "CA" => Some(egui::include_image!("../assets/flags/CA.svg")),
        "CC" => Some(egui::include_image!("../assets/flags/CC.svg")),
        "CD" => Some(egui::include_image!("../assets/flags/CD.svg")),
        "CF" => Some(egui::include_image!("../assets/flags/CF.svg")),
        "CG" => Some(egui::include_image!("../assets/flags/CG.svg")),
        "CH" => Some(egui::include_image!("../assets/flags/CH.svg")),
        "CI" => Some(egui::include_image!("../assets/flags/CI.svg")),
        "CK" => Some(egui::include_image!("../assets/flags/CK.svg")),
        "CL" => Some(egui::include_image!("../assets/flags/CL.svg")),
        "CM" => Some(egui::include_image!("../assets/flags/CM.svg")),
        "CN" => Some(egui::include_image!("../assets/flags/CN.svg")),
        "CO" => Some(egui::include_image!("../assets/flags/CO.svg")),
        "CR" => Some(egui::include_image!("../assets/flags/CR.svg")),
        "CU" => Some(egui::include_image!("../assets/flags/CU.svg")),
        "CV" => Some(egui::include_image!("../assets/flags/CV.svg")),
        "CW" => Some(egui::include_image!("../assets/flags/CW.svg")),
        "CX" => Some(egui::include_image!("../assets/flags/CX.svg")),
        "CY" => Some(egui::include_image!("../assets/flags/CY.svg")),
        "CZ" => Some(egui::include_image!("../assets/flags/CZ.svg")),
        "DE" => Some(egui::include_image!("../assets/flags/DE.svg")),
        "DJ" => Some(egui::include_image!("../assets/flags/DJ.svg")),
        "DK" => Some(egui::include_image!("../assets/flags/DK.svg")),
        "DM" => Some(egui::include_image!("../assets/flags/DM.svg")),
        "DO" => Some(egui::include_image!("../assets/flags/DO.svg")),
        "DZ" => Some(egui::include_image!("../assets/flags/DZ.svg")),
        "EC" => Some(egui::include_image!("../assets/flags/EC.svg")),
        "EE" => Some(egui::include_image!("../assets/flags/EE.svg")),
        "EG" => Some(egui::include_image!("../assets/flags/EG.svg")),
        "EH" => Some(egui::include_image!("../assets/flags/EH.svg")),
        "ER" => Some(egui::include_image!("../assets/flags/ER.svg")),
        "ES-CT" => Some(egui::include_image!("../assets/flags/ES-CT.svg")),
        "ES" => Some(egui::include_image!("../assets/flags/ES.svg")),
        "ET" => Some(egui::include_image!("../assets/flags/ET.svg")),
        "EU" => Some(egui::include_image!("../assets/flags/EU.svg")),
        "FI" => Some(egui::include_image!("../assets/flags/FI.svg")),
        "FJ" => Some(egui::include_image!("../assets/flags/FJ.svg")),
        "FK" => Some(egui::include_image!("../assets/flags/FK.svg")),
        "FM" => Some(egui::include_image!("../assets/flags/FM.svg")),
        "FO" => Some(egui::include_image!("../assets/flags/FO.svg")),
        "FR" => Some(egui::include_image!("../assets/flags/FR.svg")),
        "GA" => Some(egui::include_image!("../assets/flags/GA.svg")),
        "GB-ENG" => Some(egui::include_image!("../assets/flags/GB-ENG.svg")),
        "GB-NIR" => Some(egui::include_image!("../assets/flags/GB-NIR.svg")),
        "GB-SCT" => Some(egui::include_image!("../assets/flags/GB-SCT.svg")),
        "GB-WLS" => Some(egui::include_image!("../assets/flags/GB-WLS.svg")),
        "GB" => Some(egui::include_image!("../assets/flags/GB.svg")),
        "GD" => Some(egui::include_image!("../assets/flags/GD.svg")),
        "GE" => Some(egui::include_image!("../assets/flags/GE.svg")),
        "GF" => Some(egui::include_image!("../assets/flags/GF.svg")),
        "GG" => Some(egui::include_image!("../assets/flags/GG.svg")),
        "GH" => Some(egui::include_image!("../assets/flags/GH.svg")),
        "GI" => Some(egui::include_image!("../assets/flags/GI.svg")),
        "GL" => Some(egui::include_image!("../assets/flags/GL.svg")),
        "GM" => Some(egui::include_image!("../assets/flags/GM.svg")),
        "GN" => Some(egui::include_image!("../assets/flags/GN.svg")),
        "GP" => Some(egui::include_image!("../assets/flags/GP.svg")),
        "GQ" => Some(egui::include_image!("../assets/flags/GQ.svg")),
        "GR" => Some(egui::include_image!("../assets/flags/GR.svg")),
        "GS" => Some(egui::include_image!("../assets/flags/GS.svg")),
        "GT" => Some(egui::include_image!("../assets/flags/GT.svg")),
        "GU" => Some(egui::include_image!("../assets/flags/GU.svg")),
        "GW" => Some(egui::include_image!("../assets/flags/GW.svg")),
        "GY" => Some(egui::include_image!("../assets/flags/GY.svg")),
        "HK" => Some(egui::include_image!("../assets/flags/HK.svg")),
        "HM" => Some(egui::include_image!("../assets/flags/HM.svg")),
        "HN" => Some(egui::include_image!("../assets/flags/HN.svg")),
        "HR" => Some(egui::include_image!("../assets/flags/HR.svg")),
        "HT" => Some(egui::include_image!("../assets/flags/HT.svg")),
        "HU" => Some(egui::include_image!("../assets/flags/HU.svg")),
        "IC" => Some(egui::include_image!("../assets/flags/IC.svg")),
        "ID" => Some(egui::include_image!("../assets/flags/ID.svg")),
        "IE" => Some(egui::include_image!("../assets/flags/IE.svg")),
        "IL" => Some(egui::include_image!("../assets/flags/IL.svg")),
        "IM" => Some(egui::include_image!("../assets/flags/IM.svg")),
        "IN" => Some(egui::include_image!("../assets/flags/IN.svg")),
        "IO" => Some(egui::include_image!("../assets/flags/IO.svg")),
        "IQ" => Some(egui::include_image!("../assets/flags/IQ.svg")),
        "IR" => Some(egui::include_image!("../assets/flags/IR.svg")),
        "IS" => Some(egui::include_image!("../assets/flags/IS.svg")),
        "IT" => Some(egui::include_image!("../assets/flags/IT.svg")),
        "JE" => Some(egui::include_image!("../assets/flags/JE.svg")),
        "JM" => Some(egui::include_image!("../assets/flags/JM.svg")),
        "JO" => Some(egui::include_image!("../assets/flags/JO.svg")),
        "JP" => Some(egui::include_image!("../assets/flags/JP.svg")),
        "KE" => Some(egui::include_image!("../assets/flags/KE.svg")),
        "KG" => Some(egui::include_image!("../assets/flags/KG.svg")),
        "KH" => Some(egui::include_image!("../assets/flags/KH.svg")),
        "KI" => Some(egui::include_image!("../assets/flags/KI.svg")),
        "KM" => Some(egui::include_image!("../assets/flags/KM.svg")),
        "KN" => Some(egui::include_image!("../assets/flags/KN.svg")),
        "KP" => Some(egui::include_image!("../assets/flags/KP.svg")),
        "KR" => Some(egui::include_image!("../assets/flags/KR.svg")),
        "KW" => Some(egui::include_image!("../assets/flags/KW.svg")),
        "KY" => Some(egui::include_image!("../assets/flags/KY.svg")),
        "KZ" => Some(egui::include_image!("../assets/flags/KZ.svg")),
        "LA" => Some(egui::include_image!("../assets/flags/LA.svg")),
        "LB" => Some(egui::include_image!("../assets/flags/LB.svg")),
        "LC" => Some(egui::include_image!("../assets/flags/LC.svg")),
        "LI" => Some(egui::include_image!("../assets/flags/LI.svg")),
        "LK" => Some(egui::include_image!("../assets/flags/LK.svg")),
        "LR" => Some(egui::include_image!("../assets/flags/LR.svg")),
        "LS" => Some(egui::include_image!("../assets/flags/LS.svg")),
        "LT" => Some(egui::include_image!("../assets/flags/LT.svg")),
        "LU" => Some(egui::include_image!("../assets/flags/LU.svg")),
        "LV" => Some(egui::include_image!("../assets/flags/LV.svg")),
        "LY" => Some(egui::include_image!("../assets/flags/LY.svg")),
        "MA" => Some(egui::include_image!("../assets/flags/MA.svg")),
        "MC" => Some(egui::include_image!("../assets/flags/MC.svg")),
        "MD" => Some(egui::include_image!("../assets/flags/MD.svg")),
        "ME" => Some(egui::include_image!("../assets/flags/ME.svg")),
        "MF" => Some(egui::include_image!("../assets/flags/MF.svg")),
        "MG" => Some(egui::include_image!("../assets/flags/MG.svg")),
        "MH" => Some(egui::include_image!("../assets/flags/MH.svg")),
        "MK" => Some(egui::include_image!("../assets/flags/MK.svg")),
        "ML" => Some(egui::include_image!("../assets/flags/ML.svg")),
        "MM" => Some(egui::include_image!("../assets/flags/MM.svg")),
        "MN" => Some(egui::include_image!("../assets/flags/MN.svg")),
        "MO" => Some(egui::include_image!("../assets/flags/MO.svg")),
        "MP" => Some(egui::include_image!("../assets/flags/MP.svg")),
        "MQ" => Some(egui::include_image!("../assets/flags/MQ.svg")),
        "MR" => Some(egui::include_image!("../assets/flags/MR.svg")),
        "MS" => Some(egui::include_image!("../assets/flags/MS.svg")),
        "MT" => Some(egui::include_image!("../assets/flags/MT.svg")),
        "MU" => Some(egui::include_image!("../assets/flags/MU.svg")),
        "MV" => Some(egui::include_image!("../assets/flags/MV.svg")),
        "MW" => Some(egui::include_image!("../assets/flags/MW.svg")),
        "MX" => Some(egui::include_image!("../assets/flags/MX.svg")),
        "MY" => Some(egui::include_image!("../assets/flags/MY.svg")),
        "MZ" => Some(egui::include_image!("../assets/flags/MZ.svg")),
        "NA" => Some(egui::include_image!("../assets/flags/NA.svg")),
        "NC" => Some(egui::include_image!("../assets/flags/NC.svg")),
        "NE" => Some(egui::include_image!("../assets/flags/NE.svg")),
        "NF" => Some(egui::include_image!("../assets/flags/NF.svg")),
        "NG" => Some(egui::include_image!("../assets/flags/NG.svg")),
        "NI" => Some(egui::include_image!("../assets/flags/NI.svg")),
        "NL" => Some(egui::include_image!("../assets/flags/NL.svg")),
        "NO" => Some(egui::include_image!("../assets/flags/NO.svg")),
        "NP" => Some(egui::include_image!("../assets/flags/NP.svg")),
        "NR" => Some(egui::include_image!("../assets/flags/NR.svg")),
        "NU" => Some(egui::include_image!("../assets/flags/NU.svg")),
        "NZ" => Some(egui::include_image!("../assets/flags/NZ.svg")),
        "OM" => Some(egui::include_image!("../assets/flags/OM.svg")),
        "PA" => Some(egui::include_image!("../assets/flags/PA.svg")),
        "PE" => Some(egui::include_image!("../assets/flags/PE.svg")),
        "PF" => Some(egui::include_image!("../assets/flags/PF.svg")),
        "PG" => Some(egui::include_image!("../assets/flags/PG.svg")),
        "PH" => Some(egui::include_image!("../assets/flags/PH.svg")),
        "PK" => Some(egui::include_image!("../assets/flags/PK.svg")),
        "PL" => Some(egui::include_image!("../assets/flags/PL.svg")),
        "PM" => Some(egui::include_image!("../assets/flags/PM.svg")),
        "PN" => Some(egui::include_image!("../assets/flags/PN.svg")),
        "PR" => Some(egui::include_image!("../assets/flags/PR.svg")),
        "PS" => Some(egui::include_image!("../assets/flags/PS.svg")),
        "PT" => Some(egui::include_image!("../assets/flags/PT.svg")),
        "PW" => Some(egui::include_image!("../assets/flags/PW.svg")),
        "PY" => Some(egui::include_image!("../assets/flags/PY.svg")),
        "QA" => Some(egui::include_image!("../assets/flags/QA.svg")),
        "RE" => Some(egui::include_image!("../assets/flags/RE.svg")),
        "RO" => Some(egui::include_image!("../assets/flags/RO.svg")),
        "RS" => Some(egui::include_image!("../assets/flags/RS.svg")),
        "RU" => Some(egui::include_image!("../assets/flags/RU.svg")),
        "RW" => Some(egui::include_image!("../assets/flags/RW.svg")),
        "SA" => Some(egui::include_image!("../assets/flags/SA.svg")),
        "SB" => Some(egui::include_image!("../assets/flags/SB.svg")),
        "SC" => Some(egui::include_image!("../assets/flags/SC.svg")),
        "SD" => Some(egui::include_image!("../assets/flags/SD.svg")),
        "SE" => Some(egui::include_image!("../assets/flags/SE.svg")),
        "SG" => Some(egui::include_image!("../assets/flags/SG.svg")),
        "SH" => Some(egui::include_image!("../assets/flags/SH.svg")),
        "SI" => Some(egui::include_image!("../assets/flags/SI.svg")),
        "SJ" => Some(egui::include_image!("../assets/flags/SJ.svg")),
        "SK" => Some(egui::include_image!("../assets/flags/SK.svg")),
        "SL" => Some(egui::include_image!("../assets/flags/SL.svg")),
        "SM" => Some(egui::include_image!("../assets/flags/SM.svg")),
        "SN" => Some(egui::include_image!("../assets/flags/SN.svg")),
        "SO" => Some(egui::include_image!("../assets/flags/SO.svg")),
        "SR" => Some(egui::include_image!("../assets/flags/SR.svg")),
        "SS" => Some(egui::include_image!("../assets/flags/SS.svg")),
        "ST" => Some(egui::include_image!("../assets/flags/ST.svg")),
        "SV" => Some(egui::include_image!("../assets/flags/SV.svg")),
        "SX" => Some(egui::include_image!("../assets/flags/SX.svg")),
        "SY" => Some(egui::include_image!("../assets/flags/SY.svg")),
        "SZ" => Some(egui::include_image!("../assets/flags/SZ.svg")),
        "TA" => Some(egui::include_image!("../assets/flags/TA.svg")),
        "TC" => Some(egui::include_image!("../assets/flags/TC.svg")),
        "TD" => Some(egui::include_image!("../assets/flags/TD.svg")),
        "TF" => Some(egui::include_image!("../assets/flags/TF.svg")),
        "TG" => Some(egui::include_image!("../assets/flags/TG.svg")),
        "TH" => Some(egui::include_image!("../assets/flags/TH.svg")),
        "TJ" => Some(egui::include_image!("../assets/flags/TJ.svg")),
        "TK" => Some(egui::include_image!("../assets/flags/TK.svg")),
        "TL" => Some(egui::include_image!("../assets/flags/TL.svg")),
        "TM" => Some(egui::include_image!("../assets/flags/TM.svg")),
        "TN" => Some(egui::include_image!("../assets/flags/TN.svg")),
        "TO" => Some(egui::include_image!("../assets/flags/TO.svg")),
        "TR" => Some(egui::include_image!("../assets/flags/TR.svg")),
        "TT" => Some(egui::include_image!("../assets/flags/TT.svg")),
        "TV" => Some(egui::include_image!("../assets/flags/TV.svg")),
        "TW" => Some(egui::include_image!("../assets/flags/TW.svg")),
        "TZ" => Some(egui::include_image!("../assets/flags/TZ.svg")),
        "UA" => Some(egui::include_image!("../assets/flags/UA.svg")),
        "UG" => Some(egui::include_image!("../assets/flags/UG.svg")),
        "UM" => Some(egui::include_image!("../assets/flags/UM.svg")),
        "US" => Some(egui::include_image!("../assets/flags/US.svg")),
        "UY" => Some(egui::include_image!("../assets/flags/UY.svg")),
        "UZ" => Some(egui::include_image!("../assets/flags/UZ.svg")),
        "VA" => Some(egui::include_image!("../assets/flags/VA.svg")),
        "VC" => Some(egui::include_image!("../assets/flags/VC.svg")),
        "VE" => Some(egui::include_image!("../assets/flags/VE.svg")),
        "VG" => Some(egui::include_image!("../assets/flags/VG.svg")),
        "VI" => Some(egui::include_image!("../assets/flags/VI.svg")),
        "VN" => Some(egui::include_image!("../assets/flags/VN.svg")),
        "VU" => Some(egui::include_image!("../assets/flags/VU.svg")),
        "WF" => Some(egui::include_image!("../assets/flags/WF.svg")),
        "WS" => Some(egui::include_image!("../assets/flags/WS.svg")),
        "XA" => Some(egui::include_image!("../assets/flags/XA.svg")),
        "XC" => Some(egui::include_image!("../assets/flags/XC.svg")),
        "XK" => Some(egui::include_image!("../assets/flags/XK.svg")),
        "XO" => Some(egui::include_image!("../assets/flags/XO.svg")),
        "YE" => Some(egui::include_image!("../assets/flags/YE.svg")),
        "YT" => Some(egui::include_image!("../assets/flags/YT.svg")),
        "ZA" => Some(egui::include_image!("../assets/flags/ZA.svg")),
        "ZM" => Some(egui::include_image!("../assets/flags/ZM.svg")),
        "ZW" => Some(egui::include_image!("../assets/flags/ZW.svg")),
        _ => None,
    }
}
