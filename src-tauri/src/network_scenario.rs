//! Scenario clock and proxy-side measurements. No URLs, packet contents or credentials.
use crate::types::WeakNetworkConfig;
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use tokio::sync::watch;
use tokio::time::Instant;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NetworkProfile {
    pub upload_kbps: u32,
    pub download_kbps: u32,
    pub latency_ms: u32,
    pub jitter_ms: u32,
    pub loss_percent: f32,
    pub duplicate_percent: f32,
    pub reorder_percent: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NetworkPhase {
    pub name: String,
    pub duration_seconds: u32,
    pub offline: bool,
    pub profile: NetworkProfile,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NetworkScenario {
    pub schema_version: u32,
    pub name: String,
    pub target_package: String,
    pub seed: u32,
    pub phases: Vec<NetworkPhase>,
}

impl NetworkScenario {
    pub fn single(config: &WeakNetworkConfig) -> Self {
        Self {
            schema_version: 1,
            name: "单阶段弱网".into(),
            target_package: config.target_package.clone(),
            seed: 1,
            phases: vec![NetworkPhase {
                name: "弱网".into(),
                duration_seconds: config.duration_seconds,
                offline: false,
                profile: NetworkProfile {
                    upload_kbps: config.upload_kbps,
                    download_kbps: config.download_kbps,
                    latency_ms: config.latency_ms,
                    jitter_ms: config.jitter_ms,
                    loss_percent: config.loss_percent,
                    duplicate_percent: config.duplicate_percent,
                    reorder_percent: config.reorder_percent,
                },
            }],
        }
    }

    pub fn total_seconds(&self) -> u32 {
        self.phases.iter().map(|phase| phase.duration_seconds).sum()
    }

    pub fn initial_config(&self) -> WeakNetworkConfig {
        let profile = &self.phases[0].profile;
        WeakNetworkConfig {
            target_package: self.target_package.clone(),
            duration_seconds: self.total_seconds(),
            upload_kbps: profile.upload_kbps,
            download_kbps: profile.download_kbps,
            latency_ms: profile.latency_ms,
            jitter_ms: profile.jitter_ms,
            loss_percent: profile.loss_percent,
            duplicate_percent: profile.duplicate_percent,
            reorder_percent: profile.reorder_percent,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err("不支持的场景版本".into());
        }
        if self.name.trim().is_empty() || self.name.chars().count() > 80 {
            return Err("场景名称须为 1～80 字".into());
        }
        crate::adb::manager::validate_package_name(&self.target_package)
            .map_err(|error| error.message)?;
        if self.target_package == "com.rendongfang.adbmonster.vpnhelper" {
            return Err("不能代理弱网助手自身".into());
        }
        if self.phases.is_empty() || self.phases.len() > 20 {
            return Err("场景须包含 1～20 个阶段".into());
        }
        for phase in &self.phases {
            if phase.name.trim().is_empty()
                || phase.name.chars().count() > 80
                || !(1..=3600).contains(&phase.duration_seconds)
            {
                return Err("阶段名称须为 1～80 字，时长须为 1～3600 秒".into());
            }
            let p = &phase.profile;
            if p.upload_kbps > 1_000_000
                || p.download_kbps > 1_000_000
                || p.latency_ms > 5000
                || p.jitter_ms > p.latency_ms
            {
                return Err("带宽上限 1,000,000 Kbps，延迟上限 5000 ms，抖动不能大于延迟".into());
            }
            if [p.loss_percent, p.duplicate_percent, p.reorder_percent]
                .iter()
                .any(|value| !value.is_finite() || !(0.0..=100.0).contains(value))
            {
                return Err("丢包、重复和乱序须在 0～100% 之间".into());
            }
        }
        if !(10..=3600).contains(&self.total_seconds()) {
            return Err("场景总时长须为 10～3600 秒".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub enum Direction {
    Upload,
    Download,
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectionCounters {
    pub tcp_received_bytes: u64,
    pub tcp_forwarded_bytes: u64,
    pub udp_received_bytes: u64,
    pub udp_received_datagrams: u64,
    pub udp_policy_evaluated: u64,
    pub udp_forwarded_bytes: u64,
    pub udp_forwarded_datagrams: u64,
    pub udp_config_drops: u64,
    pub udp_queue_overflow_drops: u64,
    pub udp_outage_drops: u64,
    pub udp_phase_change_drops: u64,
    pub udp_cancelled_datagrams: u64,
    pub udp_send_errors: u64,
    pub udp_duplicate_datagrams: u64,
    pub shaping_wait_ms: u64,
}

impl DirectionCounters {
    pub fn bytes(&self) -> u64 {
        self.tcp_forwarded_bytes + self.udp_forwarded_bytes
    }
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Counters {
    pub upload: DirectionCounters,
    pub download: DirectionCounters,
    pub active_tcp_connections: u64,
    pub total_tcp_connections: u64,
    pub peak_tcp_connections: u64,
    pub active_udp_associations: u64,
    pub total_udp_associations: u64,
    pub peak_udp_associations: u64,
    pub connection_errors: u64,
    pub udp_queue_current: u64,
    pub udp_queue_peak: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhaseResult {
    pub phase_index: usize,
    pub configured: NetworkPhase,
    pub scheduled_start_ms: u64,
    pub actual_start_ms: u64,
    pub actual_end_ms: Option<u64>,
    pub measured: Counters,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkSample {
    pub elapsed_ms: u64,
    pub phase_index: usize,
    pub interval_ms: u64,
    pub upload_payload_kbps: f64,
    pub download_payload_kbps: f64,
    pub active_tcp_connections: u64,
    pub active_udp_associations: u64,
    pub udp_queue_current: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkReport {
    pub schema_version: u32,
    pub run_id: String,
    pub device_id: String,
    pub configured: NetworkScenario,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub elapsed_ms: u64,
    pub active: bool,
    pub end_reason: Option<String>,
    pub current_phase_index: usize,
    pub measured: Counters,
    pub phases: Vec<PhaseResult>,
    pub samples: Vec<NetworkSample>,
    pub interpretation: Vec<String>,
}

#[derive(Clone)]
pub struct PhaseControl {
    pub generation: usize,
    pub phase: NetworkPhase,
    pub started: bool,
}

struct Observation {
    report: NetworkReport,
    clock: Option<Instant>,
    last_sample_ms: u64,
    last_upload: u64,
    last_download: u64,
}

pub struct NetworkRuntime {
    control: watch::Sender<PhaseControl>,
    observation: Mutex<Observation>,
    random: AtomicU64,
}

impl NetworkRuntime {
    pub fn new(scenario: NetworkScenario, run_id: String, device_id: String) -> Arc<Self> {
        let initial = PhaseControl {
            generation: 0,
            phase: scenario.phases[0].clone(),
            started: false,
        };
        let seed = u64::from(scenario.seed) + 1;
        let (control, _) = watch::channel(initial);
        Arc::new(Self {
            control,
            random: AtomicU64::new(seed),
            observation: Mutex::new(Observation {
                report: NetworkReport {
                    schema_version: 1,
                    run_id,
                    device_id,
                    configured: scenario,
                    started_at: None,
                    finished_at: None,
                    elapsed_ms: 0,
                    active: false,
                    end_reason: None,
                    current_phase_index: 0,
                    measured: Counters::default(),
                    phases: Vec::new(),
                    samples: Vec::new(),
                    interpretation: Vec::new(),
                },
                clock: None,
                last_sample_ms: 0,
                last_upload: 0,
                last_download: 0,
            }),
        })
    }

    pub fn current(&self) -> PhaseControl {
        self.control.borrow().clone()
    }
    pub fn subscribe(&self) -> watch::Receiver<PhaseControl> {
        self.control.subscribe()
    }

    pub fn activate(&self) {
        let mut state = self
            .observation
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if state.clock.is_some() {
            return;
        }
        state.clock = Some(Instant::now());
        state.report.active = true;
        state.report.started_at = Some(chrono::Utc::now().to_rfc3339());
        let phase = state.report.configured.phases[0].clone();
        let mut measured = Counters::default();
        copy_gauges(&state.report.measured, &mut measured);
        state.report.phases.push(PhaseResult {
            phase_index: 0,
            configured: phase.clone(),
            scheduled_start_ms: 0,
            actual_start_ms: 0,
            actual_end_ms: None,
            measured,
        });
        self.control.send_replace(PhaseControl {
            generation: 0,
            phase,
            started: true,
        });
    }

    /// Uses absolute elapsed time, never accumulates timer drift. Returns true at the end.
    pub fn tick(&self) -> bool {
        let mut state = self
            .observation
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let Some(clock) = state.clock else {
            return false;
        };
        if !state.report.active {
            return true;
        }
        let elapsed = clock.elapsed().as_millis() as u64;
        state.report.elapsed_ms = elapsed;
        let mut index = state.report.current_phase_index;
        let mut boundary: u64 = state.report.configured.phases[..=index]
            .iter()
            .map(|phase| u64::from(phase.duration_seconds) * 1000)
            .sum();
        while elapsed >= boundary && index + 1 < state.report.configured.phases.len() {
            sample(&mut state, elapsed);
            if let Some(previous) = state.report.phases.last_mut() {
                previous.actual_end_ms = Some(elapsed);
            }
            index += 1;
            let configured = state.report.configured.phases[index].clone();
            let mut measured = Counters::default();
            copy_gauges(&state.report.measured, &mut measured);
            state.report.phases.push(PhaseResult {
                phase_index: index,
                configured: configured.clone(),
                scheduled_start_ms: boundary,
                actual_start_ms: elapsed,
                actual_end_ms: None,
                measured,
            });
            state.report.current_phase_index = index;
            self.control.send_replace(PhaseControl {
                generation: index,
                phase: configured,
                started: true,
            });
            boundary += u64::from(state.report.configured.phases[index].duration_seconds) * 1000;
        }
        if elapsed.saturating_sub(state.last_sample_ms) >= 1000 {
            sample(&mut state, elapsed);
        }
        elapsed >= u64::from(state.report.configured.total_seconds()) * 1000
    }

    pub fn update(&self, update: impl Fn(&mut Counters)) {
        let mut state = self
            .observation
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if state.report.finished_at.is_some() {
            return;
        }
        update(&mut state.report.measured);
        if let Some(phase) = state.report.phases.last_mut() {
            update(&mut phase.measured);
        }
    }

    pub fn direction(&self, direction: Direction, update: impl Fn(&mut DirectionCounters)) {
        self.update(|counts| {
            update(match direction {
                Direction::Upload => &mut counts.upload,
                Direction::Download => &mut counts.download,
            })
        });
    }

    pub fn snapshot(&self) -> NetworkReport {
        let state = self
            .observation
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let mut report = state.report.clone();
        if report.active {
            report.elapsed_ms = state
                .clock
                .map(|clock| clock.elapsed().as_millis() as u64)
                .unwrap_or(0);
        }
        report.interpretation = interpretation(&report);
        report
    }

    pub fn pending_udp(&self) -> u64 {
        self.observation
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .report
            .measured
            .udp_queue_current
    }

    pub fn finish(&self, reason: &str) -> NetworkReport {
        let mut state = self
            .observation
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if state.report.finished_at.is_none() {
            let elapsed = state
                .clock
                .map(|clock| clock.elapsed().as_millis() as u64)
                .unwrap_or(0);
            sample(&mut state, elapsed);
            state.report.elapsed_ms = elapsed;
            state.report.active = false;
            state.report.end_reason = Some(reason.to_string());
            state.report.finished_at = Some(chrono::Utc::now().to_rfc3339());
            if let Some(phase) = state.report.phases.last_mut() {
                phase.actual_end_ms = Some(elapsed);
            }
        }
        let mut report = state.report.clone();
        report.interpretation = interpretation(&report);
        report
    }

    pub fn random_unit(&self) -> f64 {
        let mut value = self.random.load(Ordering::Relaxed);
        loop {
            let mut next = value;
            next ^= next << 13;
            next ^= next >> 7;
            next ^= next << 17;
            match self.random.compare_exchange_weak(
                value,
                next,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    return next.wrapping_mul(2_685_821_657_736_338_717) as f64 / u64::MAX as f64
                }
                Err(actual) => value = actual,
            }
        }
    }
    pub fn hit(&self, percent: f32) -> bool {
        percent > 0.0 && self.random_unit() * 100.0 < f64::from(percent)
    }
}

fn copy_gauges(from: &Counters, to: &mut Counters) {
    to.active_tcp_connections = from.active_tcp_connections;
    to.peak_tcp_connections = from.active_tcp_connections;
    to.active_udp_associations = from.active_udp_associations;
    to.peak_udp_associations = from.active_udp_associations;
    to.udp_queue_current = from.udp_queue_current;
    to.udp_queue_peak = from.udp_queue_current;
}

fn sample(state: &mut Observation, elapsed: u64) {
    let interval = elapsed.saturating_sub(state.last_sample_ms);
    if interval == 0 {
        return;
    }
    let counts = &state.report.measured;
    if state.report.samples.len() < 3700 {
        state.report.samples.push(NetworkSample {
            elapsed_ms: elapsed,
            phase_index: state.report.current_phase_index,
            interval_ms: interval,
            upload_payload_kbps: counts.upload.bytes().saturating_sub(state.last_upload) as f64
                * 8.0
                / interval as f64,
            download_payload_kbps: counts.download.bytes().saturating_sub(state.last_download)
                as f64
                * 8.0
                / interval as f64,
            active_tcp_connections: counts.active_tcp_connections,
            active_udp_associations: counts.active_udp_associations,
            udp_queue_current: counts.udp_queue_current,
        });
    }
    state.last_sample_ms = elapsed;
    state.last_upload = counts.upload.bytes();
    state.last_download = counts.download.bytes();
}

fn interpretation(report: &NetworkReport) -> Vec<String> {
    let mut notes = vec![
        "测量点是桌面代理的成功 socket 写入：有效载荷吞吐不含 IP/TCP/SOCKS 帧头，包含模拟重复发送；不是设备网卡速率，也不能证明对端应用已经收到。".into(),
        "UDP-over-TCP 隧道具有 TCP 重传、队头阻塞和背压；这里的 UDP 丢包是代理显式丢弃数据报，不等同于无线链路丢包。未测量 RTT、真实链路丢包或应用唯一有效吞吐。".into(),
        "短时断网暂停 TCP 有效载荷并对新 TCP 转发施加等待，UDP 新数据报被丢弃；保留 TCP 字节顺序，应用自身仍可能超时。这不是关闭设备 Wi-Fi/移动网络。".into(),
        "阶段切换立即更新共享配额；未写入的排队 UDP 作废并单独计数，已开始的 UDP-over-TCP 帧保留完整性。TCP 排队数据按新阶段重新整形；内核中已经发送的数据不能撤回。".into(),
        "配置丢包比例的分母是 udpPolicyEvaluated；队列溢出发生在配置丢包之后。断网、阶段切换、取消和发送错误不混入配置丢包计数。各阶段按事件发生时归属，跨阶段的到达与发送数量可以不相等。".into(),
        "带宽为所有 TCP/UDP 连接合计的配额，不是保证达到的速率；无足够流量、服务器响应、ADB 隧道及 OS 调度均可能使实测速率更低。已付配额的数据遇到 socket 背压后集中写入，短窗口也可能高于配额。shapingWaitMs 为已完成的配额申请等待累计（含互斥等待），可超过墙钟时长；不含被取消的申请或附加延迟等待。".into(),
        "配置延迟是单向代理附加等待，并非实测 RTT；往返请求可叠加上下行延迟。队列是共享的 256 个待处理数据报（含正在发送者），不包含 OS/ADB/设备内核缓冲，因此不能把队列未溢出理解成真实链路没有丢包。重复发送跨阶段时，原包可能已发送而剩余副本被作废；取消/发送错误计数针对处理任务，不能直接与发送数相加还原到达数。".into(),
        "seed 与阶段配置一同保存；相同输入处理顺序下随机决策可重复，但真实流量、线程调度和对端行为不同，不能保证每次丢弃同一批数据报。UDP 通道数是 SOCKS 关联数，不是远端地址数。".into(),
    ];
    let a = &report.measured.upload;
    let b = &report.measured.download;
    let overflow = a.udp_queue_overflow_drops + b.udp_queue_overflow_drops;
    if overflow > 0 {
        notes.push(format!("本次已观测到 {overflow} 个 UDP 队列溢出丢弃；这是配置丢包之外的附加损失，队列上限为 256 个数据报。"));
    }
    if a.udp_policy_evaluated + b.udp_policy_evaluated == 0 {
        notes.push(
            "本次没有可用于配置丢包比例计算的 UDP 数据报；不能把没有样本解释成 0% 丢包。".into(),
        );
    }
    if a.bytes() + b.bytes() == 0 {
        notes.push("本次未观测到成功转发的有效载荷，不能据此验证配置带宽是否准确。".into());
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{advance, Duration};

    fn scenario() -> NetworkScenario {
        let profile = NetworkProfile {
            upload_kbps: 0,
            download_kbps: 0,
            latency_ms: 0,
            jitter_ms: 0,
            loss_percent: 0.0,
            duplicate_percent: 0.0,
            reorder_percent: 0.0,
        };
        NetworkScenario {
            schema_version: 1,
            name: "repeat".into(),
            target_package: "com.example.game".into(),
            seed: 42,
            phases: (0..4)
                .map(|index| NetworkPhase {
                    name: format!("phase {index}"),
                    duration_seconds: 3,
                    offline: index == 2,
                    profile: NetworkProfile {
                        latency_ms: if index == 1 { 500 } else { 0 },
                        ..profile.clone()
                    },
                })
                .collect(),
        }
    }

    #[test]
    fn validates_normal_phases_and_rejects_unbounded_or_invalid_scenarios() {
        let base = scenario();
        assert!(base.validate().is_ok());
        for mutate in [
            |s: &mut NetworkScenario| s.schema_version = 2,
            |s: &mut NetworkScenario| s.name = " ".into(),
            |s: &mut NetworkScenario| s.target_package = "com.bad;id".into(),
            |s: &mut NetworkScenario| s.phases.clear(),
            |s: &mut NetworkScenario| s.phases = vec![s.phases[0].clone(); 21],
            |s: &mut NetworkScenario| s.phases[0].duration_seconds = 0,
            |s: &mut NetworkScenario| s.phases[0].duration_seconds = u32::MAX,
            |s: &mut NetworkScenario| s.phases[0].duration_seconds = 3600,
            |s: &mut NetworkScenario| s.phases[0].profile.jitter_ms = 1,
            |s: &mut NetworkScenario| s.phases[0].profile.loss_percent = f32::NAN,
            |s: &mut NetworkScenario| s.phases[0].profile.download_kbps = 1_000_001,
        ] {
            let mut invalid = base.clone();
            mutate(&mut invalid);
            assert!(invalid.validate().is_err());
        }
    }

    #[test]
    fn saved_config_and_random_decisions_are_replayable_without_mutating_input() {
        let original = scenario();
        let serialized = serde_json::to_vec(&original).unwrap();
        let restored: NetworkScenario = serde_json::from_slice(&serialized).unwrap();
        assert_eq!(restored, original);
        let first = NetworkRuntime::new(original.clone(), "one".into(), "a".into());
        let second = NetworkRuntime::new(restored, "two".into(), "b".into());
        for _ in 0..1000 {
            assert_eq!(first.random_unit(), second.random_unit());
        }
        assert_eq!(first.snapshot().configured, original);
        let json = String::from_utf8(serialized)
            .unwrap()
            .replace("\"seed\":42", "\"seed\":42,\"unknown\":true");
        assert!(serde_json::from_str::<NetworkScenario>(&json).is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn clock_starts_after_activation_and_switches_all_four_phases() {
        let runtime = NetworkRuntime::new(scenario(), "run".into(), "device".into());
        advance(Duration::from_secs(90)).await;
        assert!(!runtime.tick());
        assert_eq!(runtime.snapshot().elapsed_ms, 0);
        runtime.activate();
        runtime.update(|c| {
            c.active_tcp_connections = 2;
            c.peak_tcp_connections = 2;
        });
        runtime.direction(Direction::Upload, |c| c.tcp_forwarded_bytes += 8000);
        advance(Duration::from_secs(2)).await;
        assert!(!runtime.tick());
        assert_eq!(runtime.snapshot().samples[0].upload_payload_kbps, 32.0);
        advance(Duration::from_secs(1)).await;
        assert!(!runtime.tick());
        assert_eq!(runtime.current().generation, 1);
        let report = runtime.snapshot();
        assert_eq!(report.phases[1].measured.active_tcp_connections, 2);
        assert_eq!(report.phases[1].measured.peak_tcp_connections, 2);
        assert_eq!(report.phases[1].measured.upload.tcp_forwarded_bytes, 0);
        advance(Duration::from_secs(3)).await;
        runtime.tick();
        assert!(runtime.current().phase.offline);
        runtime.direction(Direction::Download, |c| c.udp_outage_drops += 1);
        advance(Duration::from_secs(3)).await;
        runtime.tick();
        assert!(!runtime.current().phase.offline);
        advance(Duration::from_secs(3)).await;
        assert!(runtime.tick());
        let report = runtime.finish("complete");
        assert_eq!(report.elapsed_ms, 12000);
        assert_eq!(report.phases.len(), 4);
        assert_eq!(report.phases[2].measured.download.udp_outage_drops, 1);
        assert!(report
            .phases
            .iter()
            .all(|p| p.actual_end_ms.unwrap() - p.actual_start_ms == 3000));
        runtime.direction(Direction::Upload, |c| c.tcp_forwarded_bytes += 1);
        assert_eq!(
            runtime.finish("late").end_reason.as_deref(),
            Some("complete")
        );
        assert_eq!(runtime.snapshot().measured.upload.tcp_forwarded_bytes, 8000);
    }

    #[tokio::test(start_paused = true)]
    async fn late_ticks_record_actual_boundaries_without_accumulating_drift() {
        let runtime = NetworkRuntime::new(scenario(), "run".into(), "device".into());
        runtime.activate();
        advance(Duration::from_millis(7100)).await;
        runtime.tick();
        let report = runtime.snapshot();
        assert_eq!(report.current_phase_index, 2);
        assert_eq!(report.phases[1].scheduled_start_ms, 3000);
        assert_eq!(report.phases[1].actual_start_ms, 7100);
        assert_eq!(report.phases[1].actual_end_ms, Some(7100));
        advance(Duration::from_millis(1900)).await;
        runtime.tick();
        assert_eq!(runtime.snapshot().phases[3].actual_start_ms, 9000);
    }

    #[test]
    fn report_explains_missing_samples_and_additional_queue_loss() {
        let runtime = NetworkRuntime::new(scenario(), "run".into(), "device".into());
        runtime.activate();
        runtime.direction(Direction::Upload, |c| c.udp_queue_overflow_drops = 17);
        let notes = runtime.snapshot().interpretation.join("\n");
        assert!(notes.contains("17 个 UDP 队列溢出"));
        assert!(notes.contains("没有可用于配置丢包比例"));
        assert!(notes.contains("UDP-over-TCP"));
        assert!(notes.contains("未观测到成功转发"));
    }
}
