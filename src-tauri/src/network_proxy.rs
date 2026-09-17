use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{lookup_host, TcpListener, TcpStream, UdpSocket};
use tokio::sync::{mpsc, watch, Mutex, Semaphore};
use tokio::task::JoinSet;
use tokio::time::{sleep, sleep_until, timeout, Duration, Instant};

#[cfg(test)]
use crate::network_scenario::NetworkScenario;
use crate::network_scenario::{Direction, NetworkRuntime, PhaseControl};
use crate::types::WeakNetworkConfig;

const SOCKS_VERSION: u8 = 5;
const AUTH_USERNAME_PASSWORD: u8 = 2;
const COMMAND_CONNECT: u8 = 1;
const COMMAND_FORWARD_UDP: u8 = 5;
const RESPONSE_SUCCESS: u8 = 0;
const RESPONSE_HOST_UNREACHABLE: u8 = 4;
const RESPONSE_COMMAND_UNSUPPORTED: u8 = 7;
const MAX_UDP_DATAGRAM: usize = 65_507;
const STREAM_QUEUE_DEPTH: usize = 32;
// Shared across both directions and every UDP association: at most ~16 MiB payload.
const MAX_PENDING_DATAGRAMS: usize = 256;

static RANDOM_STATE: AtomicU64 = AtomicU64::new(0x4d59_5df4_d0f3_3173);

pub struct ProxyHandle {
    pub port: u16,
    pub runtime: Arc<NetworkRuntime>,
    completion: watch::Receiver<bool>,
    shutdown: watch::Sender<bool>,
}

impl ProxyHandle {
    pub async fn stop_and_wait(&self) -> Result<(), String> {
        self.stop();
        let mut completion = self.completion.clone();
        timeout(Duration::from_secs(3), async {
            while !*completion.borrow_and_update() {
                if completion.changed().await.is_err() {
                    break;
                }
            }
            while self.runtime.pending_udp() > 0 {
                sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .map_err(|_| "代理任务清理超时，最终统计可能不完整".to_string())
    }
    pub fn stop(&self) {
        let _ = self.shutdown.send(true);
    }
}

impl Drop for ProxyHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Clone)]
struct Credentials {
    username: String,
    password: String,
}

#[derive(Clone, Debug)]
struct SocksAddress {
    host: String,
    port: u16,
    wire: Vec<u8>,
}

#[derive(Clone)]
struct UdpPacket {
    address: SocksAddress,
    data: Vec<u8>,
}

struct ScheduledChunk {
    generation: usize,
    due: Instant,
    bytes: Vec<u8>,
}

/// A single, non-bursting bandwidth budget shared by TCP and UDP in one direction.
struct Bandwidth {
    kbps: u32,
    runtime: Option<Arc<NetworkRuntime>>,
    direction: Direction,
    gate: Mutex<()>,
}

impl Bandwidth {
    fn new(kbps: u32) -> Self {
        Self {
            kbps,
            gate: Mutex::new(()),
            runtime: None,
            direction: Direction::Upload,
        }
    }
    fn managed(kbps: u32, runtime: Arc<NetworkRuntime>, direction: Direction) -> Self {
        Self {
            runtime: Some(runtime),
            direction,
            ..Self::new(kbps)
        }
    }
    fn rate(&self) -> u32 {
        self.runtime
            .as_ref()
            .map(|runtime| match self.direction {
                Direction::Upload => runtime.current().phase.profile.upload_kbps,
                Direction::Download => runtime.current().phase.profile.download_kbps,
            })
            .unwrap_or(self.kbps)
    }
    async fn acquire(&self, bytes: usize) {
        if bytes == 0 {
            return;
        }
        let begin = Instant::now();
        loop {
            let mut changes = self.runtime.as_ref().map(|runtime| runtime.subscribe());
            if let Some(changes) = changes.as_mut() {
                wait_online(changes).await;
            }
            let payment = async {
                let _slot = self.gate.lock().await;
                let rate = self.rate();
                if rate > 0 {
                    sleep(Duration::from_secs_f64(
                        bytes as f64 * 8.0 / (rate as f64 * 1000.0),
                    ))
                    .await;
                }
            };
            if let Some(changes) = changes.as_mut() {
                tokio::select! {
                    biased;
                    _ = changes.changed() => continue,
                    _ = payment => {}
                }
            } else {
                payment.await;
            }
            break;
        }
        if let Some(runtime) = &self.runtime {
            runtime.direction(self.direction, |counts| {
                counts.shaping_wait_ms += begin.elapsed().as_millis() as u64
            });
        }
    }
    fn stream_chunk_size(&self) -> usize {
        let rate = self.rate();
        if rate == 0 {
            16 * 1024
        } else {
            ((rate as usize * 1000 / 8) / 50).clamp(1, 16 * 1024)
        }
    }
}

async fn wait_online(changes: &mut watch::Receiver<PhaseControl>) {
    loop {
        let state = changes.borrow_and_update().clone();
        if state.started && !state.phase.offline {
            return;
        }
        if changes.changed().await.is_err() {
            return;
        }
    }
}

struct ProxySession {
    config: WeakNetworkConfig,
    upload: Arc<Bandwidth>,
    download: Arc<Bandwidth>,
    udp_slots: Arc<Semaphore>,
    runtime: Arc<NetworkRuntime>,
}

impl ProxySession {
    #[cfg(test)]
    fn new(config: WeakNetworkConfig) -> Self {
        let runtime = NetworkRuntime::new(
            NetworkScenario::single(&config),
            "test".into(),
            "test".into(),
        );
        runtime.activate();
        Self::managed(config, runtime)
    }
    fn managed(config: WeakNetworkConfig, runtime: Arc<NetworkRuntime>) -> Self {
        Self {
            upload: Arc::new(Bandwidth::managed(
                config.upload_kbps,
                runtime.clone(),
                Direction::Upload,
            )),
            download: Arc::new(Bandwidth::managed(
                config.download_kbps,
                runtime.clone(),
                Direction::Download,
            )),
            udp_slots: Arc::new(Semaphore::new(MAX_PENDING_DATAGRAMS)),
            runtime,
            config,
        }
    }
    fn admit_udp(&self, direction: Direction, bytes: usize) -> Option<UdpAdmission> {
        let phase = self.runtime.current();
        self.runtime.direction(direction, |counts| {
            counts.udp_received_datagrams += 1;
            counts.udp_received_bytes += bytes as u64;
        });
        if phase.phase.offline {
            self.runtime
                .direction(direction, |counts| counts.udp_outage_drops += 1);
            return None;
        }
        self.runtime
            .direction(direction, |counts| counts.udp_policy_evaluated += 1);
        if self.runtime.hit(phase.phase.profile.loss_percent) {
            self.runtime
                .direction(direction, |counts| counts.udp_config_drops += 1);
            return None;
        }
        let Ok(slot) = self.udp_slots.clone().try_acquire_owned() else {
            self.runtime
                .direction(direction, |counts| counts.udp_queue_overflow_drops += 1);
            return None;
        };
        self.runtime.update(|counts| {
            counts.udp_queue_current += 1;
            counts.udp_queue_peak = counts.udp_queue_peak.max(counts.udp_queue_current);
        });
        let copies = if self.runtime.hit(phase.phase.profile.duplicate_percent) {
            2
        } else {
            1
        };
        let reordered = self.runtime.hit(phase.phase.profile.reorder_percent);
        Some(UdpAdmission {
            runtime: self.runtime.clone(),
            direction,
            phase,
            copies,
            reordered,
            _slot: slot,
            completed: false,
        })
    }
}

struct UdpAdmission {
    runtime: Arc<NetworkRuntime>,
    direction: Direction,
    phase: PhaseControl,
    copies: usize,
    reordered: bool,
    _slot: tokio::sync::OwnedSemaphorePermit,
    completed: bool,
}

impl UdpAdmission {
    async fn prepare<T>(
        &self,
        future: impl Future<Output = Result<T, String>>,
    ) -> Result<Option<T>, String> {
        let mut changes = self.runtime.subscribe();
        if changes.borrow_and_update().generation != self.phase.generation {
            return Ok(None);
        }
        tokio::select! {
            biased;
            _ = changes.changed() => Ok(None),
            result = future => result.map(Some),
        }
    }
    fn complete(mut self, result: Result<bool, String>) -> Result<(), String> {
        self.completed = true;
        match result {
            Ok(true) => Ok(()),
            Ok(false) => {
                self.runtime
                    .direction(self.direction, |counts| counts.udp_phase_change_drops += 1);
                Ok(())
            }
            Err(error) => {
                self.runtime
                    .direction(self.direction, |counts| counts.udp_send_errors += 1);
                Err(error)
            }
        }
    }
}
impl Drop for UdpAdmission {
    fn drop(&mut self) {
        self.runtime
            .update(|counts| counts.udp_queue_current = counts.udp_queue_current.saturating_sub(1));
        if !self.completed {
            self.runtime
                .direction(self.direction, |counts| counts.udp_cancelled_datagrams += 1);
        }
    }
}

struct ConnectionGuard {
    runtime: Arc<NetworkRuntime>,
    udp: bool,
}
impl ConnectionGuard {
    fn new(runtime: Arc<NetworkRuntime>, udp: bool) -> Self {
        runtime.update(|counts| {
            if udp {
                counts.active_udp_associations += 1;
                counts.total_udp_associations += 1;
                counts.peak_udp_associations = counts
                    .peak_udp_associations
                    .max(counts.active_udp_associations);
            } else {
                counts.active_tcp_connections += 1;
                counts.total_tcp_connections += 1;
                counts.peak_tcp_connections = counts
                    .peak_tcp_connections
                    .max(counts.active_tcp_connections);
            }
        });
        Self { runtime, udp }
    }
}
impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.runtime.update(|counts| {
            if self.udp {
                counts.active_udp_associations = counts.active_udp_associations.saturating_sub(1);
            } else {
                counts.active_tcp_connections = counts.active_tcp_connections.saturating_sub(1);
            }
        });
    }
}

/// Covers already-stopped receivers as well as sender destruction.
async fn stopped(mut shutdown: watch::Receiver<bool>) {
    loop {
        if *shutdown.borrow_and_update() {
            return;
        }
        if shutdown.changed().await.is_err() {
            return;
        }
    }
}

async fn cancellable<T>(
    shutdown: watch::Receiver<bool>,
    operation: impl Future<Output = Result<T, String>>,
) -> Result<T, String> {
    tokio::select! {
        biased;
        _ = stopped(shutdown) => Err("弱网代理已停止".to_string()),
        result = operation => result,
    }
}

#[cfg(test)]
pub async fn start_proxy(
    config: WeakNetworkConfig,
    username: String,
    password: String,
) -> Result<ProxyHandle, String> {
    let runtime = NetworkRuntime::new(
        NetworkScenario::single(&config),
        "test".into(),
        "test".into(),
    );
    runtime.activate();
    start_proxy_observed(config, username, password, runtime).await
}

pub async fn start_proxy_observed(
    config: WeakNetworkConfig,
    username: String,
    password: String,
    runtime: Arc<NetworkRuntime>,
) -> Result<ProxyHandle, String> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .map_err(|error| format!("启动本地弱网代理失败：{error}"))?;
    let port = listener
        .local_addr()
        .map_err(|error| format!("读取本地弱网代理端口失败：{error}"))?
        .port();
    let credentials = Arc::new(Credentials { username, password });
    let session = Arc::new(ProxySession::managed(config, runtime.clone()));
    let (completed, completion) = watch::channel(false);
    let (shutdown, shutdown_rx) = watch::channel(false);

    tokio::spawn(async move {
        let mut clients = JoinSet::new();
        loop {
            tokio::select! {
                biased;
                _ = stopped(shutdown_rx.clone()) => break,
                Some(_) = clients.join_next(), if !clients.is_empty() => {},
                accepted = listener.accept() => {
                    let Ok((stream, _)) = accepted else { break; };
                    clients.spawn(handle_client(
                        stream, credentials.clone(), session.clone(), shutdown_rx.clone(),
                    ));
                }
            }
        }
        // No detached connection tasks survive a stopped proxy.
        clients.abort_all();
        while clients.join_next().await.is_some() {}
        completed.send_replace(true);
    });

    Ok(ProxyHandle {
        port,
        shutdown,
        runtime,
        completion,
    })
}

async fn handle_client(
    mut client: TcpStream,
    credentials: Arc<Credentials>,
    session: Arc<ProxySession>,
    shutdown: watch::Receiver<bool>,
) -> Result<(), String> {
    cancellable(shutdown.clone(), async {
        client
            .set_nodelay(true)
            .map_err(|error| error.to_string())?;
        let (command, target) = timeout(Duration::from_secs(10), async {
            authenticate(&mut client, &credentials).await?;
            read_request(&mut client).await
        })
        .await
        .map_err(|_| "SOCKS5 握手超时".to_string())??;

        match command {
            COMMAND_CONNECT => {
                wait_online(&mut session.runtime.subscribe()).await;
                let connection = timeout(Duration::from_secs(10), async {
                    let target_addr = resolve_address(&target).await?;
                    TcpStream::connect(target_addr)
                        .await
                        .map_err(|error| error.to_string())
                })
                .await;
                match connection {
                    Ok(Ok(remote)) => {
                        let _connection = ConnectionGuard::new(session.runtime.clone(), false);
                        write_response(&mut client, RESPONSE_SUCCESS).await?;
                        relay_tcp(client, remote, session, shutdown).await
                    }
                    result => {
                        session
                            .runtime
                            .update(|counts| counts.connection_errors += 1);
                        write_response(&mut client, RESPONSE_HOST_UNREACHABLE).await?;
                        Err(format!("连接目标失败：{result:?}"))
                    }
                }
            }
            COMMAND_FORWARD_UDP => {
                write_response(&mut client, RESPONSE_SUCCESS).await?;
                relay_udp_over_tcp(client, session, shutdown).await
            }
            _ => {
                write_response(&mut client, RESPONSE_COMMAND_UNSUPPORTED).await?;
                Err(format!("不支持的 SOCKS5 命令：{command}"))
            }
        }
    })
    .await
}

async fn authenticate(stream: &mut TcpStream, credentials: &Credentials) -> Result<(), String> {
    let mut header = [0_u8; 2];
    stream
        .read_exact(&mut header)
        .await
        .map_err(|error| format!("读取 SOCKS5 握手失败：{error}"))?;
    if header[0] != SOCKS_VERSION || header[1] == 0 {
        return Err("无效的 SOCKS5 握手".to_string());
    }

    let mut methods = vec![0_u8; header[1] as usize];
    stream
        .read_exact(&mut methods)
        .await
        .map_err(|error| format!("读取 SOCKS5 认证方式失败：{error}"))?;
    if !methods.contains(&AUTH_USERNAME_PASSWORD) {
        stream
            .write_all(&[SOCKS_VERSION, 0xff])
            .await
            .map_err(|error| error.to_string())?;
        return Err("弱网代理要求用户名密码认证".to_string());
    }
    stream
        .write_all(&[SOCKS_VERSION, AUTH_USERNAME_PASSWORD])
        .await
        .map_err(|error| format!("写入 SOCKS5 认证方式失败：{error}"))?;

    let mut auth_header = [0_u8; 2];
    stream
        .read_exact(&mut auth_header)
        .await
        .map_err(|error| format!("读取 SOCKS5 用户名失败：{error}"))?;
    if auth_header[0] != 1 || auth_header[1] == 0 {
        return Err("无效的 SOCKS5 用户名认证请求".to_string());
    }
    let mut username = vec![0_u8; auth_header[1] as usize];
    stream
        .read_exact(&mut username)
        .await
        .map_err(|error| format!("读取 SOCKS5 用户名失败：{error}"))?;
    let password_len = stream
        .read_u8()
        .await
        .map_err(|error| format!("读取 SOCKS5 密码长度失败：{error}"))?;
    let mut password = vec![0_u8; password_len as usize];
    stream
        .read_exact(&mut password)
        .await
        .map_err(|error| format!("读取 SOCKS5 密码失败：{error}"))?;

    let accepted =
        username == credentials.username.as_bytes() && password == credentials.password.as_bytes();
    stream
        .write_all(&[1, if accepted { 0 } else { 1 }])
        .await
        .map_err(|error| format!("写入 SOCKS5 认证结果失败：{error}"))?;
    if accepted {
        Ok(())
    } else {
        Err("弱网代理认证失败".to_string())
    }
}

async fn read_request(stream: &mut TcpStream) -> Result<(u8, SocksAddress), String> {
    let mut header = [0_u8; 4];
    stream
        .read_exact(&mut header)
        .await
        .map_err(|error| format!("读取 SOCKS5 请求失败：{error}"))?;
    if header[0] != SOCKS_VERSION || header[2] != 0 {
        return Err("无效的 SOCKS5 请求".to_string());
    }
    let address = read_address(stream, header[3]).await?;
    Ok((header[1], address))
}

async fn read_address<R>(reader: &mut R, address_type: u8) -> Result<SocksAddress, String>
where
    R: AsyncRead + Unpin,
{
    let mut wire = vec![address_type];
    let host = match address_type {
        1 => {
            let mut octets = [0_u8; 4];
            reader
                .read_exact(&mut octets)
                .await
                .map_err(|error| format!("读取 IPv4 地址失败：{error}"))?;
            wire.extend_from_slice(&octets);
            IpAddr::V4(Ipv4Addr::from(octets)).to_string()
        }
        4 => {
            let mut octets = [0_u8; 16];
            reader
                .read_exact(&mut octets)
                .await
                .map_err(|error| format!("读取 IPv6 地址失败：{error}"))?;
            wire.extend_from_slice(&octets);
            IpAddr::V6(Ipv6Addr::from(octets)).to_string()
        }
        3 => {
            let length = reader
                .read_u8()
                .await
                .map_err(|error| format!("读取域名长度失败：{error}"))?;
            if length == 0 {
                return Err("SOCKS5 域名不能为空".to_string());
            }
            let mut bytes = vec![0_u8; length as usize];
            reader
                .read_exact(&mut bytes)
                .await
                .map_err(|error| format!("读取域名失败：{error}"))?;
            wire.push(length);
            wire.extend_from_slice(&bytes);
            String::from_utf8(bytes).map_err(|_| "SOCKS5 域名不是 UTF-8".to_string())?
        }
        _ => return Err(format!("不支持的 SOCKS5 地址类型：{address_type}")),
    };

    let port = reader
        .read_u16()
        .await
        .map_err(|error| format!("读取目标端口失败：{error}"))?;
    wire.extend_from_slice(&port.to_be_bytes());
    Ok(SocksAddress { host, port, wire })
}

async fn resolve_address(address: &SocksAddress) -> Result<SocketAddr, String> {
    if let Ok(ip) = address.host.parse::<IpAddr>() {
        return Ok(SocketAddr::new(ip, address.port));
    }
    lookup_host((address.host.as_str(), address.port))
        .await
        .map_err(|error| format!("解析 {} 失败：{error}", address.host))?
        .next()
        .ok_or_else(|| format!("没有找到 {} 的地址", address.host))
}

async fn write_response(stream: &mut TcpStream, response: u8) -> Result<(), String> {
    stream
        .write_all(&[SOCKS_VERSION, response, 0, 1, 0, 0, 0, 0, 0, 0])
        .await
        .map_err(|error| format!("写入 SOCKS5 响应失败：{error}"))
}

async fn relay_tcp(
    client: TcpStream,
    remote: TcpStream,
    session: Arc<ProxySession>,
    shutdown: watch::Receiver<bool>,
) -> Result<(), String> {
    let (client_reader, client_writer) = client.into_split();
    let (remote_reader, remote_writer) = remote.into_split();
    let upload = relay_stream(
        client_reader,
        remote_writer,
        session.upload.clone(),
        session.config.latency_ms,
        session.config.jitter_ms,
        shutdown.clone(),
    );
    let download = relay_stream(
        remote_reader,
        client_writer,
        session.download.clone(),
        session.config.latency_ms,
        session.config.jitter_ms,
        shutdown,
    );
    tokio::try_join!(upload, download)?;
    Ok(())
}

async fn relay_stream<R, W>(
    mut reader: R,
    mut writer: W,
    bandwidth: Arc<Bandwidth>,
    latency_ms: u32,
    jitter_ms: u32,
    shutdown: watch::Receiver<bool>,
) -> Result<(), String>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    cancellable(shutdown, async move {
        let (sender, mut receiver) = mpsc::channel::<ScheduledChunk>(STREAM_QUEUE_DEPTH);
        let read_budget = bandwidth.clone();
        let read_loop = async move {
            loop {
                if let Some(runtime) = &read_budget.runtime { wait_online(&mut runtime.subscribe()).await; }
                let mut buffer = vec![0_u8; read_budget.stream_chunk_size()];
                let read = reader.read(&mut buffer).await.map_err(|error| format!("读取 TCP 流失败：{error}"))?;
                if read == 0 { return Ok::<(), String>(()); }
                buffer.truncate(read);
                let (generation, delay) = if let Some(runtime) = &read_budget.runtime {
                    runtime.direction(read_budget.direction, |counts| counts.tcp_received_bytes += read as u64);
                    let phase = runtime.current();
                    (phase.generation, phase_delay(runtime, &phase, false))
                } else { (0, randomized_delay(latency_ms, jitter_ms, false)) };
                let chunk = ScheduledChunk { due: Instant::now() + delay, generation, bytes: buffer };
                if sender.send(chunk).await.is_err() { return Ok(()); }
            }
        };
        let write_loop = async move {
            while let Some(chunk) = receiver.recv().await {
                let mut due = chunk.due;
                if let Some(runtime) = &bandwidth.runtime {
                    let mut changes = runtime.subscribe();
                    let mut generation = chunk.generation;
                    loop {
                        wait_online(&mut changes).await;
                        let phase = changes.borrow_and_update().clone();
                        if phase.generation != generation {
                            generation = phase.generation;
                            due = Instant::now() + phase_delay(runtime, &phase, false);
                        }
                        tokio::select! { biased; _ = changes.changed() => continue, _ = sleep_until(due) => break }
                    }
                } else { sleep_until(due).await; }
                write_counted(&mut writer, &chunk.bytes, 0, &bandwidth, true, None).await?;
            }
            writer.shutdown().await.map_err(|error| format!("关闭 TCP 流失败：{error}"))
        };
        tokio::try_join!(read_loop, write_loop)?;
        Ok(())
    }).await
}

fn phase_delay(runtime: &NetworkRuntime, phase: &PhaseControl, reordered: bool) -> Duration {
    let profile = &phase.phase.profile;
    let jitter =
        ((runtime.random_unit() * 2.0 - 1.0) * f64::from(profile.jitter_ms)).round() as i64;
    let mut millis = (i64::from(profile.latency_ms) + jitter).max(0) as u64;
    if reordered {
        millis += u64::from(profile.jitter_ms.max(20));
    }
    Duration::from_millis(millis)
}

/// Count only successful payload writes, including partial TCP writes. A partially
/// written UDP-over-TCP frame must finish intact; otherwise the association corrupts.
async fn write_counted<W: AsyncWrite + Unpin>(
    writer: &mut W,
    bytes: &[u8],
    payload_offset: usize,
    bandwidth: &Bandwidth,
    tcp: bool,
    prepaid: Option<usize>,
) -> Result<bool, String> {
    let mut offset = 0;
    while offset < bytes.len() {
        let mut changes = bandwidth
            .runtime
            .as_ref()
            .map(|runtime| runtime.subscribe());
        if let Some(changes) = changes.as_mut() {
            if prepaid.is_some_and(|generation| generation != changes.borrow().generation)
                && offset == 0
            {
                return Ok(false);
            }
            wait_online(changes).await;
        }
        let end = bytes.len().min(offset + bandwidth.stream_chunk_size());
        let charge = end.saturating_sub(payload_offset.max(offset));
        let credited = prepaid.is_some_and(|generation| {
            bandwidth
                .runtime
                .as_ref()
                .is_some_and(|runtime| runtime.current().generation == generation)
        });
        if !credited {
            bandwidth.acquire(charge).await;
        }
        if let Some(changes) = changes.as_mut() {
            let phase = changes.borrow_and_update().clone();
            if prepaid.is_some_and(|generation| generation != phase.generation) && offset == 0 {
                return Ok(false);
            }
            if !phase.started || phase.phase.offline {
                continue;
            }
        }
        while offset < end {
            let result = if let Some(changes) = changes.as_mut() {
                tokio::select! { biased; _ = changes.changed() => break, result = writer.write(&bytes[offset..end]) => result }
            } else {
                writer.write(&bytes[offset..end]).await
            };
            let written = result.map_err(|error| format!("写入代理有效载荷失败：{error}"))?;
            if written == 0 {
                return Err("代理写入返回零字节".into());
            }
            let payload = (offset + written).saturating_sub(payload_offset.max(offset)) as u64;
            if let Some(runtime) = &bandwidth.runtime {
                runtime.direction(bandwidth.direction, |counts| {
                    if tcp {
                        counts.tcp_forwarded_bytes += payload;
                    } else {
                        counts.udp_forwarded_bytes += payload;
                    }
                });
            }
            offset += written;
        }
    }
    Ok(true)
}

fn randomized_delay(latency_ms: u32, jitter_ms: u32, reordered: bool) -> Duration {
    let jitter = if jitter_ms == 0 {
        0_i64
    } else {
        let width = jitter_ms as f64;
        ((random_unit() * 2.0 - 1.0) * width).round() as i64
    };
    let mut millis = (latency_ms as i64 + jitter).max(0) as u64;
    if reordered {
        millis = millis.saturating_add(u64::from(jitter_ms.max(20)));
    }
    Duration::from_millis(millis)
}

fn random_unit() -> f64 {
    let mut current = RANDOM_STATE.load(Ordering::Relaxed);
    loop {
        let mut next = current;
        next ^= next << 13;
        next ^= next >> 7;
        next ^= next << 17;
        match RANDOM_STATE.compare_exchange_weak(
            current,
            next,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return next as f64 / u64::MAX as f64,
            Err(actual) => current = actual,
        }
    }
}

async fn relay_udp_over_tcp(
    client: TcpStream,
    session: Arc<ProxySession>,
    shutdown: watch::Receiver<bool>,
) -> Result<(), String> {
    cancellable(shutdown, async move {
        let _connection = ConnectionGuard::new(session.runtime.clone(), true);
        while !session.runtime.current().started {
            let mut changes = session.runtime.subscribe();
            if !changes.borrow_and_update().started {
                let _ = changes.changed().await;
            }
        }
        let socket_v4 = Arc::new(
            UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
                .await
                .map_err(|error| format!("创建 IPv4 UDP 转发器失败：{error}"))?,
        );
        let socket_v6 = UdpSocket::bind((Ipv6Addr::UNSPECIFIED, 0))
            .await
            .ok()
            .map(Arc::new);
        let (reader, writer) = client.into_split();
        let writer = Arc::new(Mutex::new(writer));
        let upload = udp_upload_loop(
            reader,
            socket_v4.clone(),
            socket_v6.clone(),
            session.clone(),
        );
        let download_v4 = udp_download_loop(socket_v4, writer.clone(), session.clone());
        let download_v6 = async {
            if let Some(socket) = socket_v6 {
                udp_download_loop(socket, writer, session).await
            } else {
                std::future::pending::<Result<(), String>>().await
            }
        };
        // Dropping any loop drops its JoinSet and aborts all queued datagrams.
        tokio::select! {
            result = upload => result,
            result = download_v4 => result,
            result = download_v6 => result,
        }
    })
    .await
}

async fn udp_upload_loop(
    mut reader: tokio::net::tcp::OwnedReadHalf,
    socket_v4: Arc<UdpSocket>,
    socket_v6: Option<Arc<UdpSocket>>,
    session: Arc<ProxySession>,
) -> Result<(), String> {
    let mut tasks = JoinSet::new();
    loop {
        // Do not select task completions against read_exact: partial frames must survive.
        let packet = read_udp_frame(&mut reader).await?;
        while tasks.try_join_next().is_some() {}
        let Some(admission) = session.admit_udp(Direction::Upload, packet.data.len()) else {
            continue;
        };
        let (v4, v6, session) = (socket_v4.clone(), socket_v6.clone(), session.clone());
        tasks.spawn(send_udp_packet(packet, v4, v6, session, admission));
    }
}

#[cfg(test)]
async fn send_udp_to_network(
    packet: UdpPacket,
    v4: Arc<UdpSocket>,
    v6: Option<Arc<UdpSocket>>,
    session: Arc<ProxySession>,
) -> Result<(), String> {
    let Some(admission) = session.admit_udp(Direction::Upload, packet.data.len()) else {
        return Ok(());
    };
    send_udp_packet(packet, v4, v6, session, admission).await
}

async fn send_udp_packet(
    packet: UdpPacket,
    socket_v4: Arc<UdpSocket>,
    socket_v6: Option<Arc<UdpSocket>>,
    session: Arc<ProxySession>,
    admission: UdpAdmission,
) -> Result<(), String> {
    let result = admission
        .prepare(async {
            sleep(phase_delay(
                &session.runtime,
                &admission.phase,
                admission.reordered,
            ))
            .await;
            let destination = resolve_address(&packet.address).await?;
            let socket = match destination {
                SocketAddr::V4(_) => socket_v4,
                SocketAddr::V6(_) => {
                    socket_v6.ok_or_else(|| "当前主机无法创建 IPv6 UDP 转发器".to_string())?
                }
            };
            for copy in 0..admission.copies {
                session.upload.acquire(packet.data.len()).await;
                let written = socket
                    .send_to(&packet.data, destination)
                    .await
                    .map_err(|error| format!("发送 UDP 数据报失败：{error}"))?;
                session.runtime.direction(Direction::Upload, |counts| {
                    counts.udp_forwarded_bytes += written as u64;
                    counts.udp_forwarded_datagrams += 1;
                    if copy > 0 {
                        counts.udp_duplicate_datagrams += 1;
                    }
                });
            }
            Ok(())
        })
        .await
        .map(|result| result.is_some());
    admission.complete(result)
}

async fn udp_download_loop(
    socket: Arc<UdpSocket>,
    writer: Arc<Mutex<tokio::net::tcp::OwnedWriteHalf>>,
    session: Arc<ProxySession>,
) -> Result<(), String> {
    let mut buffer = vec![0_u8; MAX_UDP_DATAGRAM];
    let mut tasks = JoinSet::new();
    loop {
        let (length, source) = tokio::select! {
            Some(_) = tasks.join_next(), if !tasks.is_empty() => continue,
            received = socket.recv_from(&mut buffer) => received.map_err(|error| format!("接收 UDP 数据报失败：{error}"))?,
        };
        while tasks.try_join_next().is_some() {}
        let Some(admission) = session.admit_udp(Direction::Download, length) else {
            continue;
        };
        tasks.spawn(send_udp_to_client(
            buffer[..length].to_vec(),
            source,
            writer.clone(),
            session.clone(),
            admission,
        ));
    }
}

async fn send_udp_to_client(
    data: Vec<u8>,
    source: SocketAddr,
    writer: Arc<Mutex<tokio::net::tcp::OwnedWriteHalf>>,
    session: Arc<ProxySession>,
    admission: UdpAdmission,
) -> Result<(), String> {
    let result = async {
        let frame = encode_udp_frame(socket_address(source), &data)?;
        let Some(()) = admission
            .prepare(async {
                sleep(phase_delay(
                    &session.runtime,
                    &admission.phase,
                    admission.reordered,
                ))
                .await;
                Ok(())
            })
            .await?
        else {
            return Ok(false);
        };
        for copy in 0..admission.copies {
            // Only pre-write preparation is cancelled on phase change. Once a frame
            // starts, write_counted pauses/resumes it intact instead of corrupting framing.
            let Some(mut writer) = admission
                .prepare(async {
                    let writer = writer.lock().await;
                    session.download.acquire(data.len()).await;
                    Ok(writer)
                })
                .await?
            else {
                return Ok(false);
            };
            let written = write_counted(
                &mut *writer,
                &frame,
                frame.len() - data.len(),
                &session.download,
                false,
                Some(admission.phase.generation),
            )
            .await?;
            if !written {
                return Ok(false);
            }
            session.runtime.direction(Direction::Download, |counts| {
                counts.udp_forwarded_datagrams += 1;
                if copy > 0 {
                    counts.udp_duplicate_datagrams += 1;
                }
            });
        }
        Ok(true)
    }
    .await;
    admission.complete(result)
}

async fn read_udp_frame<R>(reader: &mut R) -> Result<UdpPacket, String>
where
    R: AsyncRead + Unpin,
{
    let mut header = [0_u8; 3];
    reader
        .read_exact(&mut header)
        .await
        .map_err(|error| format!("读取 UDP-over-TCP 帧头失败：{error}"))?;
    let data_length = u16::from_be_bytes([header[0], header[1]]) as usize;
    let header_length = header[2] as usize;
    if data_length > MAX_UDP_DATAGRAM || header_length < 8 {
        return Err("无效的 UDP-over-TCP 帧长度".to_string());
    }

    let mut wire = vec![0_u8; header_length - 3];
    reader
        .read_exact(&mut wire)
        .await
        .map_err(|error| format!("读取 UDP-over-TCP 地址失败：{error}"))?;
    let address = parse_wire_address(wire)?;
    let mut data = vec![0_u8; data_length];
    reader
        .read_exact(&mut data)
        .await
        .map_err(|error| format!("读取 UDP-over-TCP 数据失败：{error}"))?;
    Ok(UdpPacket { address, data })
}

fn encode_udp_frame(address: SocksAddress, data: &[u8]) -> Result<Vec<u8>, String> {
    let data_length =
        u16::try_from(data.len()).map_err(|_| "UDP 数据报超过 65535 字节".to_string())?;
    let header_length =
        u8::try_from(3 + address.wire.len()).map_err(|_| "UDP-over-TCP 地址头过长".to_string())?;
    let mut frame = Vec::with_capacity(3 + address.wire.len() + data.len());
    frame.extend_from_slice(&data_length.to_be_bytes());
    frame.push(header_length);
    frame.extend_from_slice(&address.wire);
    frame.extend_from_slice(data);
    Ok(frame)
}

fn parse_wire_address(wire: Vec<u8>) -> Result<SocksAddress, String> {
    let Some(address_type) = wire.first().copied() else {
        return Err("UDP-over-TCP 地址为空".to_string());
    };
    let (host, port_offset) = match address_type {
        1 if wire.len() == 7 => (
            IpAddr::V4(Ipv4Addr::new(wire[1], wire[2], wire[3], wire[4])).to_string(),
            5,
        ),
        4 if wire.len() == 19 => {
            let octets: [u8; 16] = wire[1..17]
                .try_into()
                .map_err(|_| "无效的 IPv6 地址".to_string())?;
            (IpAddr::V6(Ipv6Addr::from(octets)).to_string(), 17)
        }
        3 if wire.len() >= 4 => {
            let domain_length = wire[1] as usize;
            if wire.len() != domain_length + 4 {
                return Err("UDP-over-TCP 域名长度不匹配".to_string());
            }
            let domain = String::from_utf8(wire[2..2 + domain_length].to_vec())
                .map_err(|_| "UDP-over-TCP 域名不是 UTF-8".to_string())?;
            (domain, 2 + domain_length)
        }
        _ => return Err("无效的 UDP-over-TCP 地址".to_string()),
    };
    let port = u16::from_be_bytes([wire[port_offset], wire[port_offset + 1]]);
    Ok(SocksAddress { host, port, wire })
}

fn socket_address(address: SocketAddr) -> SocksAddress {
    let mut wire = Vec::new();
    match address.ip() {
        IpAddr::V4(ip) => {
            wire.push(1);
            wire.extend_from_slice(&ip.octets());
        }
        IpAddr::V6(ip) => {
            wire.push(4);
            wire.extend_from_slice(&ip.octets());
        }
    }
    wire.extend_from_slice(&address.port().to_be_bytes());
    SocksAddress {
        host: address.ip().to_string(),
        port: address.port(),
        wire,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zero_config() -> WeakNetworkConfig {
        WeakNetworkConfig {
            target_package: "com.example.game".to_string(),
            upload_kbps: 0,
            download_kbps: 0,
            latency_ms: 0,
            jitter_ms: 0,
            loss_percent: 0.0,
            duplicate_percent: 0.0,
            reorder_percent: 0.0,
            duration_seconds: 60,
        }
    }

    async fn connect_authenticated_proxy(port: u16) -> TcpStream {
        let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port))
            .await
            .expect("connect proxy");
        stream
            .write_all(&[SOCKS_VERSION, 1, AUTH_USERNAME_PASSWORD])
            .await
            .expect("write greeting");
        let mut method = [0_u8; 2];
        stream
            .read_exact(&mut method)
            .await
            .expect("read auth method");
        assert_eq!(method, [SOCKS_VERSION, AUTH_USERNAME_PASSWORD]);

        stream
            .write_all(&[1, 4, b'u', b's', b'e', b'r', 4, b'p', b'a', b's', b's'])
            .await
            .expect("write credentials");
        let mut result = [0_u8; 2];
        stream
            .read_exact(&mut result)
            .await
            .expect("read auth result");
        assert_eq!(result, [1, 0]);
        stream
    }

    async fn send_request(stream: &mut TcpStream, command: u8, target: SocketAddr) {
        let address = socket_address(target);
        let mut request = vec![SOCKS_VERSION, command, 0];
        request.extend_from_slice(&address.wire);
        stream.write_all(&request).await.expect("write request");
        let mut response = [0_u8; 10];
        stream
            .read_exact(&mut response)
            .await
            .expect("read response");
        assert_eq!(&response[..2], &[SOCKS_VERSION, RESPONSE_SUCCESS]);
    }

    #[tokio::test]
    async fn relays_authenticated_tcp_connection() {
        let echo = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("bind echo");
        let echo_address = echo.local_addr().expect("echo address");
        let echo_task = tokio::spawn(async move {
            let (mut stream, _) = echo.accept().await.expect("accept echo");
            let mut bytes = [0_u8; 4];
            stream.read_exact(&mut bytes).await.expect("read echo");
            stream.write_all(&bytes).await.expect("write echo");
        });

        let proxy = start_proxy(zero_config(), "user".to_string(), "pass".to_string())
            .await
            .expect("start proxy");
        let mut stream = connect_authenticated_proxy(proxy.port).await;
        send_request(&mut stream, COMMAND_CONNECT, echo_address).await;
        stream.write_all(b"ping").await.expect("write payload");
        let mut response = [0_u8; 4];
        timeout(Duration::from_secs(2), stream.read_exact(&mut response))
            .await
            .expect("TCP relay timeout")
            .expect("read payload");
        assert_eq!(&response, b"ping");

        proxy.stop_and_wait().await.unwrap();
        let report = proxy.runtime.finish("test");
        assert_eq!(report.measured.upload.tcp_forwarded_bytes, 4);
        assert_eq!(report.measured.download.tcp_forwarded_bytes, 4);
        assert_eq!(report.measured.active_tcp_connections, 0);
        assert_eq!(report.measured.total_tcp_connections, 1);
        echo_task.await.expect("echo task");
    }

    #[tokio::test]
    async fn real_proxy_connections_obey_one_upload_limit() {
        let echo = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let address = echo.local_addr().unwrap();
        let echo_task = tokio::spawn(async move {
            let mut peers = JoinSet::new();
            for _ in 0..2 {
                let (mut peer, _) = echo.accept().await.unwrap();
                peers.spawn(async move {
                    let mut payload = vec![0_u8; 16 * 1024];
                    peer.read_exact(&mut payload).await.unwrap();
                    peer.write_all(b"ok").await.unwrap();
                });
            }
            while let Some(result) = peers.join_next().await {
                result.unwrap();
            }
        });
        let mut config = zero_config();
        config.upload_kbps = 256;
        let proxy = start_proxy(config, "user".into(), "pass".into())
            .await
            .unwrap();
        let mut first = connect_authenticated_proxy(proxy.port).await;
        let mut second = connect_authenticated_proxy(proxy.port).await;
        send_request(&mut first, COMMAND_CONNECT, address).await;
        send_request(&mut second, COMMAND_CONNECT, address).await;
        let transfer = |mut client: TcpStream| async move {
            client.write_all(&vec![0_u8; 16 * 1024]).await.unwrap();
            let mut response = [0_u8; 2];
            client.read_exact(&mut response).await.unwrap();
            assert_eq!(&response, b"ok");
        };
        let start = Instant::now();
        timeout(Duration::from_secs(5), async {
            tokio::join!(transfer(first), transfer(second));
        })
        .await
        .unwrap();
        assert!(start.elapsed() >= Duration::from_millis(1_024));
        proxy.stop();
        echo_task.await.unwrap();
    }

    #[tokio::test]
    async fn relays_hev_udp_over_tcp_frame() {
        let echo = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("bind UDP echo");
        let echo_address = echo.local_addr().expect("UDP echo address");
        let echo_task = tokio::spawn(async move {
            let mut bytes = [0_u8; 32];
            let (length, peer) = echo.recv_from(&mut bytes).await.expect("receive UDP");
            echo.send_to(&bytes[..length], peer)
                .await
                .expect("send UDP");
        });

        let proxy = start_proxy(zero_config(), "user".to_string(), "pass".to_string())
            .await
            .expect("start proxy");
        let mut stream = connect_authenticated_proxy(proxy.port).await;
        send_request(
            &mut stream,
            COMMAND_FORWARD_UDP,
            SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
        )
        .await;

        let frame =
            encode_udp_frame(socket_address(echo_address), b"datagram").expect("encode frame");
        stream.write_all(&frame).await.expect("write UDP frame");
        let packet = timeout(Duration::from_secs(2), read_udp_frame(&mut stream))
            .await
            .expect("UDP relay timeout")
            .expect("read UDP frame");
        assert_eq!(packet.address.port, echo_address.port());
        assert_eq!(packet.data, b"datagram");

        proxy.stop_and_wait().await.unwrap();
        let report = proxy.runtime.finish("test");
        assert_eq!(report.measured.upload.udp_forwarded_bytes, 8);
        assert_eq!(report.measured.download.udp_forwarded_bytes, 8);
        assert_eq!(report.measured.upload.udp_forwarded_datagrams, 1);
        assert_eq!(report.measured.download.udp_forwarded_datagrams, 1);
        assert_eq!(report.measured.active_udp_associations, 0);
        assert_eq!(report.measured.total_udp_associations, 1);
        assert_eq!(report.measured.udp_queue_current, 0);
        echo_task.await.expect("UDP echo task");
    }

    #[tokio::test(start_paused = true)]
    async fn first_tcp_payload_is_charged_at_low_bandwidth() {
        let payload = vec![0_u8; 16 * 1024];
        let (_stop, shutdown) = watch::channel(false);
        let start = Instant::now();
        relay_stream(
            payload.as_slice(),
            tokio::io::sink(),
            Arc::new(Bandwidth::new(8)),
            0,
            0,
            shutdown,
        )
        .await
        .unwrap();
        assert!(start.elapsed() >= Duration::from_millis(16_384));
    }

    #[tokio::test(start_paused = true)]
    async fn parallel_tcp_connections_share_total_bandwidth() {
        let payload = vec![0_u8; 64 * 1024];
        let budget = Arc::new(Bandwidth::new(256));
        let (_stop, shutdown) = watch::channel(false);
        let start = Instant::now();
        tokio::try_join!(
            relay_stream(
                payload.as_slice(),
                tokio::io::sink(),
                budget.clone(),
                0,
                0,
                shutdown.clone()
            ),
            relay_stream(
                payload.as_slice(),
                tokio::io::sink(),
                budget,
                0,
                0,
                shutdown
            ),
        )
        .unwrap();
        assert!(start.elapsed() >= Duration::from_millis(4_096));
    }

    #[tokio::test(start_paused = true)]
    async fn udp_and_tcp_share_the_session_upload_budget() {
        let mut config = zero_config();
        config.upload_kbps = 64;
        let session = Arc::new(ProxySession::new(config));
        let sender = Arc::new(UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap());
        let receiver = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let packet = UdpPacket {
            address: socket_address(receiver.local_addr().unwrap()),
            data: vec![0_u8; 4096],
        };
        let payload = vec![0_u8; 4096];
        let (_stop, shutdown) = watch::channel(false);
        let start = Instant::now();
        tokio::try_join!(
            relay_stream(
                payload.as_slice(),
                tokio::io::sink(),
                session.upload.clone(),
                0,
                0,
                shutdown
            ),
            send_udp_to_network(packet, sender, None, session),
        )
        .unwrap();
        assert!(start.elapsed() >= Duration::from_millis(1_024));
    }

    struct StalledWriter {
        entered: Option<tokio::sync::oneshot::Sender<()>>,
        dropped: Arc<std::sync::atomic::AtomicBool>,
    }

    impl Drop for StalledWriter {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::SeqCst);
        }
    }

    impl AsyncWrite for StalledWriter {
        fn poll_write(
            mut self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
            _data: &[u8],
        ) -> std::task::Poll<std::io::Result<usize>> {
            if let Some(entered) = self.entered.take() {
                let _ = entered.send(());
            }
            std::task::Poll::Pending
        }
        fn poll_flush(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::task::Poll::Ready(Ok(()))
        }
        fn poll_shutdown(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::task::Poll::Ready(Ok(()))
        }
    }

    #[tokio::test]
    async fn stop_cancels_blocked_writer_and_drops_resources() {
        let (stop, shutdown) = watch::channel(false);
        let (entered, waiting) = tokio::sync::oneshot::channel();
        let dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let writer = StalledWriter {
            entered: Some(entered),
            dropped: dropped.clone(),
        };
        let relay = tokio::spawn(async move {
            relay_stream(
                &b"payload"[..],
                writer,
                Arc::new(Bandwidth::new(0)),
                0,
                0,
                shutdown,
            )
            .await
        });
        timeout(Duration::from_secs(1), waiting)
            .await
            .unwrap()
            .unwrap();
        stop.send(true).unwrap();
        assert!(timeout(Duration::from_secs(1), relay)
            .await
            .unwrap()
            .unwrap()
            .is_err());
        assert!(dropped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn stopping_proxy_closes_an_incomplete_handshake() {
        let proxy = start_proxy(zero_config(), "user".into(), "pass".into())
            .await
            .unwrap();
        let mut client = TcpStream::connect((Ipv4Addr::LOCALHOST, proxy.port))
            .await
            .unwrap();
        client.write_all(&[5]).await.unwrap();
        proxy.stop();
        let mut response = [0_u8; 1];
        let read = timeout(Duration::from_secs(1), client.read(&mut response))
            .await
            .unwrap();
        assert!(matches!(read, Ok(0) | Err(_)));
    }

    #[tokio::test]
    async fn udp_backlog_is_bounded_and_released_on_stop() {
        let mut config = zero_config();
        config.latency_ms = 5_000;
        let session = Arc::new(ProxySession::new(config));
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let (client, server) = tokio::join!(
            TcpStream::connect(listener.local_addr().unwrap()),
            listener.accept(),
        );
        let mut client = client.unwrap();
        let (server, _) = server.unwrap();
        let (stop, shutdown) = watch::channel(false);
        let relay = tokio::spawn(relay_udp_over_tcp(server, session.clone(), shutdown));
        let sink = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let frame =
            encode_udp_frame(socket_address(sink.local_addr().unwrap()), &[1; 128]).unwrap();
        client
            .write_all(&frame.repeat(MAX_PENDING_DATAGRAMS * 4))
            .await
            .unwrap();
        timeout(Duration::from_secs(1), async {
            while session.udp_slots.available_permits() != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("UDP queue should fill without allocating unlimited tasks");
        stop.send(true).unwrap();
        assert!(timeout(Duration::from_secs(1), relay)
            .await
            .unwrap()
            .unwrap()
            .is_err());
        timeout(Duration::from_secs(1), async {
            while session.udp_slots.available_permits() != MAX_PENDING_DATAGRAMS {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("queued datagrams must be released on stop");
    }

    fn staged_session(loss: f32, initially_offline: bool) -> Arc<ProxySession> {
        let mut config = zero_config();
        config.duration_seconds = 12;
        config.loss_percent = loss;
        let mut scenario = NetworkScenario::single(&config);
        let first = scenario.phases[0].clone();
        scenario.phases = (0..4)
            .map(|index| crate::network_scenario::NetworkPhase {
                name: format!("phase {index}"),
                duration_seconds: 3,
                offline: if initially_offline {
                    index == 0
                } else {
                    index == 2
                },
                profile: crate::network_scenario::NetworkProfile {
                    loss_percent: if index == 0 { loss } else { 0.0 },
                    ..first.profile.clone()
                },
            })
            .collect();
        let runtime = NetworkRuntime::new(scenario, "test".into(), "test".into());
        runtime.activate();
        Arc::new(ProxySession::managed(config, runtime))
    }

    #[tokio::test(start_paused = true)]
    async fn measures_config_overflow_outage_phase_change_and_cancel_separately() {
        let session = staged_session(100.0, false);
        assert!(session.admit_udp(Direction::Upload, 8).is_none());
        let counts = session.runtime.snapshot().measured.upload;
        assert_eq!(counts.udp_policy_evaluated, 1);
        assert_eq!(counts.udp_config_drops, 1);
        assert_eq!(counts.udp_queue_overflow_drops, 0);
        tokio::time::advance(Duration::from_secs(3)).await;
        session.runtime.tick();
        let pending: Vec<_> = (0..MAX_PENDING_DATAGRAMS)
            .map(|_| session.admit_udp(Direction::Download, 8).unwrap())
            .collect();
        assert!(session.admit_udp(Direction::Upload, 8).is_none());
        assert_eq!(session.runtime.snapshot().measured.udp_queue_peak, 256);
        assert_eq!(
            session
                .runtime
                .snapshot()
                .measured
                .upload
                .udp_queue_overflow_drops,
            1
        );
        drop(pending);
        assert_eq!(
            session
                .runtime
                .snapshot()
                .measured
                .download
                .udp_cancelled_datagrams,
            256
        );
        let stale = session.admit_udp(Direction::Upload, 8).unwrap();
        tokio::time::advance(Duration::from_secs(3)).await;
        session.runtime.tick();
        assert!(session.admit_udp(Direction::Upload, 8).is_none());
        let prepared = stale.prepare(async { Ok(()) }).await.unwrap();
        assert!(prepared.is_none());
        stale.complete(Ok(false)).unwrap();
        let counts = session.runtime.snapshot().measured;
        assert_eq!(counts.upload.udp_config_drops, 1);
        assert_eq!(counts.upload.udp_queue_overflow_drops, 1);
        assert_eq!(counts.upload.udp_outage_drops, 1);
        assert_eq!(counts.upload.udp_phase_change_drops, 1);
        assert_eq!(counts.upload.udp_policy_evaluated, 3);
        assert_eq!(counts.udp_queue_current, 0);
        assert_eq!(session.udp_slots.available_permits(), MAX_PENDING_DATAGRAMS);
    }

    #[tokio::test(start_paused = true)]
    async fn offline_tcp_payload_resumes_on_the_same_writer() {
        let session = staged_session(0.0, true);
        let (mut writer, mut reader) = tokio::io::duplex(64);
        let budget = session.upload.clone();
        let write = tokio::spawn(async move {
            write_counted(&mut writer, b"resumed", 0, &budget, true, None)
                .await
                .unwrap()
        });
        tokio::task::yield_now().await;
        assert!(!write.is_finished());
        assert_eq!(
            session
                .runtime
                .snapshot()
                .measured
                .upload
                .tcp_forwarded_bytes,
            0
        );
        tokio::time::advance(Duration::from_secs(3)).await;
        session.runtime.tick();
        let mut payload = [0; 7];
        timeout(Duration::from_secs(1), reader.read_exact(&mut payload))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&payload, b"resumed");
        assert!(write.await.unwrap());
        assert_eq!(
            session
                .runtime
                .snapshot()
                .measured
                .upload
                .tcp_forwarded_bytes,
            7
        );
    }

    #[tokio::test(start_paused = true)]
    async fn stage_change_interrupts_old_shared_bandwidth_debt() {
        let mut config = zero_config();
        config.upload_kbps = 1;
        let mut scenario = NetworkScenario::single(&config);
        scenario.phases[0].duration_seconds = 3;
        let mut recovery = scenario.phases[0].clone();
        recovery.duration_seconds = 9;
        recovery.profile.upload_kbps = 0;
        scenario.phases.push(recovery);
        let runtime = NetworkRuntime::new(scenario, "test".into(), "test".into());
        runtime.activate();
        let budget = Arc::new(Bandwidth::managed(1, runtime.clone(), Direction::Upload));
        let payment = tokio::spawn(async move {
            budget.acquire(8000).await;
        });
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(3)).await;
        assert!(!payment.is_finished());
        runtime.tick();
        timeout(Duration::from_millis(100), payment)
            .await
            .unwrap()
            .unwrap();
        assert!(runtime.snapshot().measured.upload.shaping_wait_ms >= 3000);
    }

    #[tokio::test(start_paused = true)]
    async fn partially_written_udp_frame_survives_outage_without_counting_headers() {
        let session = staged_session(0.0, false);
        let frame =
            encode_udp_frame(socket_address("127.0.0.1:1234".parse().unwrap()), &[7; 32]).unwrap();
        let head = frame.len() - 32;
        let (mut writer, mut reader) = tokio::io::duplex(head + 4);
        let budget = session.download.clone();
        let original = frame.clone();
        let writing = tokio::spawn(async move {
            write_counted(&mut writer, &frame, head, &budget, false, Some(0))
                .await
                .unwrap()
        });
        tokio::task::yield_now().await;
        assert_eq!(
            session
                .runtime
                .snapshot()
                .measured
                .download
                .udp_forwarded_bytes,
            4
        );
        tokio::time::advance(Duration::from_secs(6)).await;
        session.runtime.tick();
        assert!(session.runtime.current().phase.offline);
        let mut first = vec![0; head + 4];
        reader.read_exact(&mut first).await.unwrap();
        tokio::task::yield_now().await;
        assert!(!writing.is_finished());
        assert_eq!(
            session
                .runtime
                .snapshot()
                .measured
                .download
                .udp_forwarded_bytes,
            4
        );
        tokio::time::advance(Duration::from_secs(3)).await;
        session.runtime.tick();
        let mut rest = Vec::new();
        timeout(Duration::from_secs(1), reader.read_to_end(&mut rest))
            .await
            .unwrap()
            .unwrap();
        assert!(writing.await.unwrap());
        first.extend(rest);
        assert_eq!(first, original);
        assert_eq!(
            session
                .runtime
                .snapshot()
                .measured
                .download
                .udp_forwarded_bytes,
            32
        );
        assert_eq!(
            read_udp_frame(&mut first.as_slice()).await.unwrap().data,
            vec![7; 32]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn phase_change_cancels_queued_udp_before_network_send() {
        let session = staged_session(0.0, false);
        let admission = session.admit_udp(Direction::Upload, 16).unwrap();
        let runtime = session.runtime.clone();
        let waiting = tokio::spawn(async move {
            let result = admission
                .prepare(async {
                    sleep(Duration::from_secs(60)).await;
                    Ok(())
                })
                .await
                .map(|prepared| prepared.is_some());
            admission.complete(result).unwrap();
        });
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(3)).await;
        runtime.tick();
        timeout(Duration::from_millis(100), waiting)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(runtime.snapshot().measured.upload.udp_phase_change_drops, 1);
        assert_eq!(runtime.snapshot().measured.upload.udp_forwarded_bytes, 0);
        assert_eq!(runtime.pending_udp(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn connection_gauges_follow_carryover_and_resource_drop() {
        let session = staged_session(0.0, false);
        let tcp = ConnectionGuard::new(session.runtime.clone(), false);
        let udp = ConnectionGuard::new(session.runtime.clone(), true);
        tokio::time::advance(Duration::from_secs(3)).await;
        session.runtime.tick();
        drop(tcp);
        drop(udp);
        let report = session.runtime.snapshot();
        assert_eq!(report.measured.total_tcp_connections, 1);
        assert_eq!(report.measured.active_tcp_connections, 0);
        assert_eq!(report.measured.active_udp_associations, 0);
        assert_eq!(report.phases[1].measured.total_tcp_connections, 0);
        assert_eq!(report.phases[1].measured.peak_tcp_connections, 1);
        assert_eq!(report.phases[1].measured.active_tcp_connections, 0);
    }

    #[test]
    fn rejects_malformed_udp_wire_address() {
        assert!(parse_wire_address(vec![1, 127, 0, 0, 1]).is_err());
        assert!(parse_wire_address(vec![3, 4, b't', b'e', b's', b't', 0]).is_err());
    }

    #[tokio::test]
    async fn accepts_short_domain_udp_frame() {
        let address =
            parse_wire_address(vec![3, 1, b'a', 0, 53]).expect("parse short domain address");
        let frame = encode_udp_frame(address, b"dns").expect("encode short domain frame");
        let mut input = frame.as_slice();
        let packet = read_udp_frame(&mut input)
            .await
            .expect("read short domain frame");
        assert_eq!(packet.address.host, "a");
        assert_eq!(packet.address.port, 53);
        assert_eq!(packet.data, b"dns");
    }
}
