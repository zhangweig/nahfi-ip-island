//! Explicit-route public IP checks. No UI-thread I/O, no direct fallback.
use std::{
    io::Read,
    net::IpAddr,
    sync::mpsc,
    time::{Duration, SystemTime},
};

/// Screenshot-safe display; full addresses are only exposed by explicit UI actions.
pub fn masked_ip(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(ip) => {
            let b = ip.octets();
            format!("{}.***.***.{}", b[0], b[3])
        }
        IpAddr::V6(ip) => format!("{:x}:****:****:****", ip.segments()[0]),
    }
}

pub(crate) fn system_proxy() -> Option<String> {
    [
        "HTTPS_PROXY",
        "https_proxy",
        "ALL_PROXY",
        "all_proxy",
        "HTTP_PROXY",
        "http_proxy",
    ]
    .iter()
    .find_map(|key| std::env::var(key).ok().filter(|v| !v.trim().is_empty()))
    .or_else(crate::quota::system_proxy)
}

pub const ENDPOINT: &str = "https://ipwho.is/";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub enabled: bool,
    pub follow_system: bool,
    pub system_proxy: Option<String>,
    pub network_tag: u64,
    pub proxy_port: u16,
    pub interval: u32,
}
impl Config {
    pub fn from_settings(s: &crate::model::Settings) -> Self {
        Self {
            enabled: s.ip_enabled,
            follow_system: s.ip_follow_system,
            system_proxy: None,
            network_tag: 0,
            proxy_port: s.ip_proxy_port,
            interval: s.ip_interval.clamp(5, 3600),
        }
    }
    pub fn route(&self) -> String {
        if self.follow_system {
            if self.system_proxy.is_some() {
                "跟随系统代理".into()
            } else {
                "系统路由（可含 TUN）".into()
            }
        } else if self.proxy_port == 0 {
            "系统路由（不指定代理）".into()
        } else {
            format!("HTTP 代理 127.0.0.1:{}", self.proxy_port)
        }
    }
}

#[derive(Clone, Debug)]
pub struct Observation {
    pub ip: IpAddr,
    pub country_code: String,
    pub country: String,
    pub checked_at: SystemTime,
}
#[derive(Clone, Default)]
pub struct State {
    pub last_success: Option<Observation>,
    pub error: Option<String>,
    pub attempted_at: Option<SystemTime>,
    pub refreshing: bool,
}
impl State {
    pub fn complete(&mut self, result: Result<Observation, String>) {
        self.refreshing = false;
        self.attempted_at = Some(SystemTime::now());
        match result {
            Ok(value) => {
                self.last_success = Some(value);
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }
    pub fn code(&self) -> &str {
        self.last_success
            .as_ref()
            .map_or("--", |o| o.country_code.as_str())
    }
}

fn parse(body: &str) -> Result<Observation, String> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|_| "IP 服务返回了无效 JSON".to_string())?;
    if value["success"].as_bool() != Some(true) {
        return Err("IP 查询服务未返回成功结果".into());
    }
    let ip = value["ip"]
        .as_str()
        .and_then(|s| s.parse::<IpAddr>().ok())
        .ok_or("IP 服务未返回有效 IP 地址")?;
    let code = value["country_code"]
        .as_str()
        .ok_or("缺少国家代码")?
        .to_ascii_uppercase();
    if code.len() != 2 || !code.bytes().all(|c| c.is_ascii_uppercase()) {
        return Err("国家代码无效".into());
    }
    let country = value["country"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("缺少国家名称")?;
    if country.len() > 128 {
        return Err("国家名称过长".into());
    }
    Ok(Observation {
        ip,
        country_code: code,
        country: country.into(),
        checked_at: SystemTime::now(),
    })
}

fn query(config: &Config, endpoint: &str) -> Result<Observation, String> {
    let mut builder = ureq::AgentBuilder::new()
        .try_proxy_from_env(false)
        .timeout(Duration::from_secs(8))
        .redirects(0);
    let proxy_url = if config.follow_system {
        config.system_proxy.clone()
    } else if config.proxy_port > 0 {
        Some(format!("http://127.0.0.1:{}", config.proxy_port))
    } else {
        None
    };
    if let Some(url) = proxy_url {
        let proxy = ureq::Proxy::new(url).map_err(|_| "代理配置无效")?;
        builder = builder.proxy(proxy);
    }
    let response = builder
        .build()
        .get(endpoint)
        .set("Cache-Control", "no-cache, no-store")
        .set("Pragma", "no-cache")
        .call()
        .map_err(|_| {
            if config.proxy_port == 0 {
                "IP 查询失败，请检查当前系统路由或代理"
            } else {
                "代理 IP 查询失败，请检查本地 HTTP/mixed 端口；未回退直连"
            }
            .to_string()
        })?;
    let mut body = String::new();
    response
        .into_reader()
        .take(65537)
        .read_to_string(&mut body)
        .map_err(|_| "读取 IP 响应失败")?;
    if body.len() > 65536 {
        return Err("IP 响应超出大小限制".into());
    }
    parse(&body)
}

type ResultMessage = (Config, Result<Observation, String>);
pub struct Service {
    tx: mpsc::SyncSender<Config>,
    rx: mpsc::Receiver<ResultMessage>,
    busy: bool,
}
impl Service {
    pub fn start(ctx: eframe::egui::Context) -> Self {
        let (tx, commands) = mpsc::sync_channel::<Config>(1);
        let (results, rx) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            while let Ok(config) = commands.recv() {
                let result = query(&config, ENDPOINT);
                if results.send((config, result)).is_err() {
                    break;
                }
                ctx.request_repaint();
            }
        });
        Self {
            tx,
            rx,
            busy: false,
        }
    }
    pub fn refresh(&mut self, config: Config) -> bool {
        if !config.enabled || self.busy {
            return false;
        }
        if self.tx.try_send(config).is_err() {
            return false;
        }
        self.busy = true;
        true
    }
    pub fn poll(&mut self) -> Option<ResultMessage> {
        let result = self.rx.try_recv().ok()?;
        self.busy = false;
        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const GOOD: &str =
        r#"{"success":true,"ip":"203.0.113.7","country_code":"US","country":"United States"}"#;
    #[test]
    fn validates_service_results() {
        assert_eq!(parse(GOOD).unwrap().country_code, "US");
        assert!(parse(&GOOD.replace("203.0.113.7", "2001:db8::1")).is_ok());
        for bad in [
            "{}".into(),
            "not JSON".into(),
            GOOD.replace("true", "false"),
            GOOD.replace("203.0.113.7", "invalid"),
            GOOD.replace("US", "USA"),
            GOOD.replace("US", "!!"),
        ] {
            assert!(parse(&bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn failure_retains_but_marks_previous_success() {
        let mut state = State::default();
        state.complete(Ok(parse(GOOD).unwrap()));
        state.complete(Err("offline".into()));
        assert_eq!(state.code(), "US");
        assert!(state.error.is_some());
        assert!(!state.refreshing);
    }
    #[test]
    fn http_request_and_explicit_proxy_failure() {
        use std::{io::Write, net::TcpListener};
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut connection, _) = listener.accept().unwrap();
            connection
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut request = [0; 4096];
            connection.read(&mut request).unwrap();
            write!(
                connection,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                GOOD.len(),
                GOOD
            )
            .unwrap();
        });
        let mut config = Config {
            enabled: true,
            follow_system: false,
            system_proxy: None,
            network_tag: 0,
            proxy_port: 0,
            interval: 60,
        };
        assert_eq!(
            query(&config, &format!("http://{address}/"))
                .unwrap()
                .ip
                .to_string(),
            "203.0.113.7"
        );
        server.join().unwrap();
        // A closed loopback port must fail without falling back to the endpoint.
        config.proxy_port = address.port();
        assert!(query(&config, "https://ipwho.is/")
            .unwrap_err()
            .contains("未回退直连"));
    }
    #[test]
    fn configured_proxy_receives_the_connect_request() {
        use std::{io::Write, net::TcpListener};
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut connection, _) = listener.accept().unwrap();
            connection
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut request = [0; 4096];
            let length = connection.read(&mut request).unwrap();
            assert!(String::from_utf8_lossy(&request[..length])
                .starts_with("CONNECT ipwho.is:443 HTTP/1.1"));
            connection
                .write_all(
                    b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
        });
        let config = Config {
            enabled: true,
            follow_system: false,
            system_proxy: None,
            network_tag: 0,
            proxy_port: port,
            interval: 60,
        };
        assert!(query(&config, ENDPOINT).unwrap_err().contains("未回退直连"));
        server.join().unwrap();
    }
}

#[cfg(test)]
mod route_tests {
    use super::*;
    #[test]
    fn automatic_route_changes_invalidate_results_without_claiming_direct() {
        let mut s = crate::model::Settings::default();
        let mut a = Config::from_settings(&s);
        assert!(a.follow_system);
        assert!(a.route().contains("系统路由"));
        a.system_proxy = Some("http://127.0.0.1:7890".into());
        assert!(a.route().contains("系统代理"));
        let mut b = a.clone();
        b.network_tag = 42;
        assert_ne!(a, b);
        b.system_proxy = None;
        assert_ne!(a, b);
        s.ip_follow_system = false;
        s.ip_proxy_port = 7891;
        let c = Config::from_settings(&s);
        assert!(c.route().contains("7891"));
    }
    #[test]
    fn automatic_proxy_is_used_for_the_actual_request() {
        use std::{io::Write, net::TcpListener};
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let worker = std::thread::spawn(move || {
            let (mut h, _) = listener.accept().unwrap();
            h.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            let mut bytes = [0u8; 4096];
            let n = h.read(&mut bytes).unwrap();
            assert!(String::from_utf8_lossy(&bytes[..n]).starts_with("CONNECT ipwho.is:443"));
            h.write_all(
                b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .unwrap();
        });
        let mut c = Config::from_settings(&crate::model::Settings::default());
        c.system_proxy = Some(format!("http://127.0.0.1:{port}"));
        assert!(query(&c, ENDPOINT).is_err());
        worker.join().unwrap();
    }
}
