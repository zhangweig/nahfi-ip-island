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

pub const ENDPOINT: &str = "https://www.cloudflare.com/cdn-cgi/trace";
pub const FALLBACK_ENDPOINT: &str = "https://ipwho.is/";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Provider {
    #[default]
    Auto,
    Cloudflare,
    Ipwho,
}
impl Provider {
    pub fn parse(value: &str) -> Self {
        match value {
            "cloudflare" => Self::Cloudflare,
            "ipwho" => Self::Ipwho,
            _ => Self::Auto,
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Cloudflare => "cloudflare",
            Self::Ipwho => "ipwho",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "自动主备",
            Self::Cloudflare => "Cloudflare Trace",
            Self::Ipwho => "ipwho.is",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub enabled: bool,
    pub provider: Provider,
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
            provider: Provider::parse(&s.ip_source),
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
    pub source: Provider,
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
        source: Provider::Ipwho,
        ip,
        country_code: code,
        country: country.into(),
        checked_at: SystemTime::now(),
    })
}

fn parse_trace(body: &str) -> Result<Observation, String> {
    let mut ip = None;
    let mut country = None;
    for line in body.lines() {
        let Some((key, value)) = line.trim_end_matches('\r').split_once('=') else {
            continue;
        };
        match key {
            "ip" => {
                if ip.is_some() {
                    return Err("Cloudflare 返回重复 IP 字段".into());
                }
                ip = Some(value.trim());
            }
            "loc" => {
                if country.is_some() {
                    return Err("Cloudflare 返回重复国家字段".into());
                }
                country = Some(value.trim());
            }
            _ => {}
        }
    }
    let ip = ip
        .and_then(|s| s.parse::<IpAddr>().ok())
        .ok_or("Cloudflare 未返回有效公网 IP")?;
    let code = country
        .ok_or("Cloudflare 未返回国家代码")?
        .to_ascii_uppercase();
    if code.len() != 2
        || !code.bytes().all(|b| b.is_ascii_uppercase())
        || matches!(code.as_str(), "XX" | "ZZ")
    {
        return Err("Cloudflare 国家代码不可用".into());
    }
    // loc is the visitor's country; colo is the serving data center and is not used.
    Ok(Observation {
        ip,
        country_code: code.clone(),
        country: code,
        source: Provider::Cloudflare,
        checked_at: SystemTime::now(),
    })
}
#[derive(Debug)]
struct Failure {
    message: String,
    retry_after: Option<u64>,
}
impl From<String> for Failure {
    fn from(message: String) -> Self {
        Self {
            message,
            retry_after: None,
        }
    }
}
fn retry_seconds(value: Option<&str>, now: chrono::DateTime<chrono::Utc>) -> u64 {
    value
        .and_then(|v| {
            v.parse::<u64>().ok().or_else(|| {
                chrono::DateTime::parse_from_rfc2822(v)
                    .ok()
                    .map(|date| date.timestamp().saturating_sub(now.timestamp()).max(0) as u64)
            })
        })
        .unwrap_or(60)
        .clamp(5, 604800)
}
fn query(config: &Config, endpoint: &str, provider: Provider) -> Result<Observation, Failure> {
    let mut builder = ureq::AgentBuilder::new()
        .try_proxy_from_env(false)
        .timeout(Duration::from_secs(4))
        .redirects(0);
    let proxy_url = if config.follow_system {
        config.system_proxy.clone()
    } else if config.proxy_port > 0 {
        Some(format!("http://127.0.0.1:{}", config.proxy_port))
    } else {
        None
    };
    if let Some(url) = proxy_url {
        builder = builder
            .proxy(ureq::Proxy::new(url).map_err(|_| Failure::from("代理配置无效".to_string()))?);
    }
    let response = builder
        .build()
        .get(endpoint)
        .set("Cache-Control", "no-cache, no-store")
        .set("Pragma", "no-cache")
        .call()
        .map_err(|e| match e {
            ureq::Error::Status(429, r) => Failure {
                message: format!("{} 暂时限流", provider.label()),
                retry_after: Some(retry_seconds(r.header("Retry-After"), chrono::Utc::now())),
            },
            ureq::Error::Status(status, _) => Failure {
                message: format!("{} 返回 HTTP {status}", provider.label()),
                retry_after: None,
            },
            _ => Failure::from(if config.follow_system || config.proxy_port > 0 {
                "代理或系统路径查询失败；未回退直连".to_string()
            } else {
                "系统路由 IP 查询失败".to_string()
            }),
        })?;
    let mut body = String::new();
    response
        .into_reader()
        .take(65537)
        .read_to_string(&mut body)
        .map_err(|_| Failure::from("读取 IP 响应失败".to_string()))?;
    if body.len() > 65536 {
        return Err(Failure::from("IP 响应超出大小限制".to_string()));
    }
    match provider {
        Provider::Cloudflare => parse_trace(&body),
        Provider::Ipwho => parse(&body),
        Provider::Auto => Err("数据源配置无效".into()),
    }
    .map_err(Failure::from)
}
type Cooldowns = std::collections::HashMap<Provider, std::time::Instant>;
fn fetch_plan(
    config: &Config,
    plan: &[(Provider, &str)],
    cooldowns: &mut Cooldowns,
) -> Result<Observation, String> {
    let mut errors = Vec::new();
    for &(provider, url) in plan {
        if cooldowns
            .get(&provider)
            .is_some_and(|until| *until > std::time::Instant::now())
        {
            errors.push(format!("{} 等待限流冷却", provider.label()));
            continue;
        }
        match query(config, url, provider) {
            Ok(value) => {
                cooldowns.remove(&provider);
                return Ok(value);
            }
            Err(error) => {
                if let Some(seconds) = error.retry_after {
                    cooldowns.insert(
                        provider,
                        std::time::Instant::now() + Duration::from_secs(seconds),
                    );
                }
                errors.push(error.message);
            }
        }
    }
    Err(errors.join("；"))
}
fn fetch(config: &Config, cooldowns: &mut Cooldowns) -> Result<Observation, String> {
    let plan = match config.provider {
        Provider::Auto => vec![
            (Provider::Cloudflare, ENDPOINT),
            (Provider::Ipwho, FALLBACK_ENDPOINT),
        ],
        Provider::Cloudflare => vec![(Provider::Cloudflare, ENDPOINT)],
        Provider::Ipwho => vec![(Provider::Ipwho, FALLBACK_ENDPOINT)],
    };
    fetch_plan(config, &plan, cooldowns)
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
            let mut cooldowns = Cooldowns::new();
            let mut previous = None;
            while let Ok(config) = commands.recv() {
                if previous.as_ref() != Some(&config) {
                    cooldowns.clear();
                    previous = Some(config.clone());
                }
                let result = fetch(&config, &mut cooldowns);
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
            provider: Provider::Auto,
            follow_system: false,
            system_proxy: None,
            network_tag: 0,
            proxy_port: 0,
            interval: 60,
        };
        assert_eq!(
            query(&config, &format!("http://{address}/"), Provider::Ipwho)
                .unwrap()
                .ip
                .to_string(),
            "203.0.113.7"
        );
        server.join().unwrap();
        // A closed loopback port must fail without falling back to the endpoint.
        config.proxy_port = address.port();
        assert!(query(&config, "https://ipwho.is/", Provider::Ipwho)
            .unwrap_err()
            .message
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
                .starts_with("CONNECT www.cloudflare.com:443 HTTP/1.1"));
            connection
                .write_all(
                    b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
        });
        let config = Config {
            enabled: true,
            provider: Provider::Auto,
            follow_system: false,
            system_proxy: None,
            network_tag: 0,
            proxy_port: port,
            interval: 60,
        };
        assert!(query(&config, ENDPOINT, Provider::Cloudflare)
            .unwrap_err()
            .message
            .contains("未回退直连"));
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
            assert!(
                String::from_utf8_lossy(&bytes[..n]).starts_with("CONNECT www.cloudflare.com:443")
            );
            h.write_all(
                b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .unwrap();
        });
        let mut c = Config::from_settings(&crate::model::Settings::default());
        c.system_proxy = Some(format!("http://127.0.0.1:{port}"));
        assert!(query(&c, ENDPOINT, Provider::Cloudflare).is_err());
        worker.join().unwrap();
    }
}

#[cfg(test)]
mod provider_tests {
    use super::*;
    const TRACE: &str = "fl=123\nip=203.0.113.9\ncolo=HKG\nloc=US\nwarp=off\n";
    const WHO: &str =
        r#"{"success":true,"ip":"203.0.113.7","country_code":"JP","country":"Japan"}"#;
    fn serve(status: &str, body: &str, extra: &str) -> (String, std::thread::JoinHandle<()>) {
        use std::{io::Write, net::TcpListener};
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\n{extra}Connection: close\r\n\r\n{body}",
            body.len()
        );
        let thread = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut bytes = [0u8; 4096];
            stream.read(&mut bytes).unwrap();
            stream.write_all(response.as_bytes()).unwrap();
        });
        (url, thread)
    }
    fn config() -> Config {
        let mut c = Config::from_settings(&crate::model::Settings::default());
        c.follow_system = false;
        c
    }
    #[test]
    fn cloudflare_country_is_visitor_loc_not_edge_colo_and_ipv6_works() {
        let o = parse_trace(TRACE).unwrap();
        assert_eq!(o.country_code, "US");
        assert_eq!(o.source, Provider::Cloudflare);
        assert!(parse_trace(&TRACE.replace("203.0.113.9", "2001:db8::9")).is_ok());
        for bad in [
            "ip=bad\nloc=US\n",
            "ip=203.0.113.9\nloc=USA\n",
            "ip=203.0.113.9\nloc=XX\n",
            "ip=203.0.113.9\ncolo=HKG\n",
            "ip=203.0.113.9\nloc=US\nloc=JP\n",
        ] {
            assert!(parse_trace(bad).is_err());
        }
    }
    #[test]
    fn retry_after_supports_seconds_dates_and_invalid_headers() {
        let now = chrono::DateTime::<chrono::Utc>::from_timestamp(0, 0).unwrap();
        assert_eq!(retry_seconds(Some("90"), now), 90);
        assert_eq!(
            retry_seconds(Some("Thu, 01 Jan 1970 00:02:00 GMT"), now),
            120
        );
        assert_eq!(retry_seconds(Some("invalid"), now), 60);
    }
    #[test]
    fn primary_success_does_not_call_or_mix_backup() {
        let (url, server) = serve("200 OK", TRACE, "");
        let o = fetch_plan(
            &config(),
            &[
                (Provider::Cloudflare, &url),
                (Provider::Ipwho, "http://127.0.0.1:1/"),
            ],
            &mut Cooldowns::new(),
        )
        .unwrap();
        server.join().unwrap();
        assert_eq!(o.source, Provider::Cloudflare);
        assert_eq!(o.ip.to_string(), "203.0.113.9");
    }
    #[test]
    fn provider_failure_falls_back_and_labels_the_actual_source() {
        let (a, first) = serve("503 Unavailable", "private content", "");
        let (b, backup) = serve("200 OK", WHO, "");
        let o = fetch_plan(
            &config(),
            &[(Provider::Cloudflare, &a), (Provider::Ipwho, &b)],
            &mut Cooldowns::new(),
        )
        .unwrap();
        first.join().unwrap();
        backup.join().unwrap();
        assert_eq!(o.source, Provider::Ipwho);
        assert_eq!(o.country_code, "JP");
        assert_eq!(o.ip.to_string(), "203.0.113.7");
    }
    #[test]
    fn rate_limit_cooldown_skips_requests_and_never_echoes_body() {
        let (a, first) = serve(
            "429 Too Many Requests",
            "private-token",
            "Retry-After: 90\r\n",
        );
        let mut cooldowns = Cooldowns::new();
        let error =
            fetch_plan(&config(), &[(Provider::Cloudflare, &a)], &mut cooldowns).unwrap_err();
        first.join().unwrap();
        assert!(!error.contains("private-token"));
        assert!(
            cooldowns[&Provider::Cloudflare].duration_since(std::time::Instant::now())
                > Duration::from_secs(80)
        );
        assert!(
            fetch_plan(&config(), &[(Provider::Cloudflare, &a)], &mut cooldowns)
                .unwrap_err()
                .contains("等待限流冷却")
        );
    }
    #[test]
    fn both_sources_keep_the_explicit_proxy_when_failing() {
        use std::{io::Write, net::TcpListener};
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let worker = std::thread::spawn(move || {
            let mut hosts = Vec::new();
            let until = std::time::Instant::now() + Duration::from_secs(4);
            while hosts.len() < 2 && std::time::Instant::now() < until {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream
                            .set_read_timeout(Some(Duration::from_secs(1)))
                            .unwrap();
                        let mut b = [0u8; 4096];
                        let n = stream.read(&mut b).unwrap();
                        hosts.push(
                            String::from_utf8_lossy(&b[..n])
                                .lines()
                                .next()
                                .unwrap()
                                .to_owned(),
                        );
                        stream.write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(e) => panic!("{e}"),
                }
            }
            hosts
        });
        let mut c = config();
        c.proxy_port = port;
        assert!(fetch(&c, &mut Cooldowns::new()).is_err());
        let hosts = worker.join().unwrap();
        assert_eq!(
            hosts,
            vec![
                "CONNECT www.cloudflare.com:443 HTTP/1.1",
                "CONNECT ipwho.is:443 HTTP/1.1"
            ]
        );
    }
}
