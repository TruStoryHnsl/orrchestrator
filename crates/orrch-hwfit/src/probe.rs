// ── module `probe` (src/probe.rs) — hardware detection ──
// Faithful port of odysseus services/hwfit/hardware.py. Formulas, table values,
// and detection logic are VERBATIM from the Python — the point is parity, not
// improvement. Local probing uses std::fs + std::process::Command; remote
// probing shells out to `ssh -o ConnectTimeout=5 -o StrictHostKeyChecking=no`.

#[allow(unused_imports)]
use crate::types::{Gpu, GpuGroup, SystemInfo};

use std::collections::HashMap;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// 30 min — hardware rarely changes; pass fresh=true to force a re-probe.
const CACHE_TTL: Duration = Duration::from_secs(1800);
const STORAGE_GBPS_ENV: &str = "ORRCH_HWFIT_STORAGE_GBPS";

/// Per-probe context: where to run commands (local vs SSH) and which platform.
/// Mirrors the Python module-level `_remote_host` / `_remote_port` globals, but
/// threaded explicitly so detect_system is reentrant/thread-safe.
struct Ctx {
    /// Some("user@server") → run over SSH. None → local.
    remote_host: Option<String>,
    /// SSH port; ignored when "" or "22".
    remote_port: Option<String>,
    /// Set by detect_nvidia when nvidia-smi errors (driver mismatch, etc.).
    last_gpu_error: Option<String>,
}

impl Ctx {
    fn new(host: &str, ssh_port: &str) -> Self {
        Ctx {
            remote_host: if host.is_empty() { None } else { Some(host.to_string()) },
            remote_port: if ssh_port.is_empty() { None } else { Some(ssh_port.to_string()) },
            last_gpu_error: None,
        }
    }

    fn is_remote(&self) -> bool {
        self.remote_host.is_some()
    }

    /// Run a command, locally or via SSH, returning trimmed stdout on success
    /// (returncode 0). Mirrors Python `_run`. `args` is the argv vector that, for
    /// SSH, gets joined with spaces into a single remote command string.
    fn run(&self, args: &[&str]) -> Option<String> {
        if let Some(host) = &self.remote_host {
            let cmd_str = args.join(" ");
            self.run_remote(host, &cmd_str)
        } else {
            let (prog, rest) = args.split_first()?;
            let out = Command::new(prog)
                .args(rest)
                .output()
                .ok()?;
            if out.status.success() {
                Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
            } else {
                None
            }
        }
    }

    /// Run a raw shell command string over SSH (used for the nvidia-smi
    /// fallbacks where the Python passed a string rather than a list).
    fn run_remote_str(&self, cmd_str: &str) -> Option<String> {
        let host = self.remote_host.as_ref()?;
        self.run_remote(host, cmd_str)
    }

    fn run_remote(&self, host: &str, cmd_str: &str) -> Option<String> {
        let mut ssh: Vec<String> = vec![
            "ssh".into(),
            "-o".into(),
            "ConnectTimeout=5".into(),
            "-o".into(),
            "StrictHostKeyChecking=no".into(),
        ];
        if let Some(port) = &self.remote_port {
            if port != "22" {
                ssh.push("-p".into());
                ssh.push(port.clone());
            }
        }
        ssh.push(host.to_string());
        ssh.push(cmd_str.to_string());

        let out = Command::new(&ssh[0]).args(&ssh[1..]).output().ok()?;
        if out.status.success() {
            Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
        } else {
            None
        }
    }

    /// Read a file, locally or via SSH (`cat path`). Mirrors `_read_file`.
    fn read_file(&self, path: &str) -> Option<String> {
        if self.is_remote() {
            self.run(&["cat", path])
        } else {
            std::fs::read_to_string(path).ok()
        }
    }

    fn list_dir_names(&self, path: &str) -> Vec<String> {
        if self.is_remote() {
            return self
                .run(&["ls", "-1", path])
                .unwrap_or_default()
                .split_whitespace()
                .map(|s| s.to_string())
                .collect();
        }

        std::fs::read_dir(path)
            .ok()
            .into_iter()
            .flat_map(|rd| rd.filter_map(|e| e.ok()))
            .filter_map(|e| e.file_name().into_string().ok())
            .collect()
    }
}

/// Round to one decimal place (Python `round(x, 1)`, banker's-rounding caveat
/// aside — values here are positive magnitudes where round-half-up vs
/// round-half-even rarely diverges; matches odysseus output in practice).
fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

fn storage_override_gbps() -> Option<f64> {
    let raw = std::env::var(STORAGE_GBPS_ENV).ok()?;
    let parsed = raw.trim().parse::<f64>().ok()?;
    if parsed > 0.0 { Some(parsed) } else { None }
}

fn mount_source_for(ctx: &Ctx, target: &str) -> Option<String> {
    if let Some(out) = ctx.run(&["findmnt", "-no", "SOURCE", "-T", target]) {
        let first = out.lines().next().unwrap_or("").trim();
        if !first.is_empty() {
            return Some(first.to_string());
        }
    }

    let out = ctx.run(&["df", "-P", target])?;
    out.lines()
        .skip(1)
        .last()
        .and_then(|line| line.split_whitespace().next())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn block_name_from_source(source: &str) -> Option<String> {
    let source = source.split('[').next().unwrap_or(source);
    let name = source.strip_prefix("/dev/")?.rsplit('/').next()?;
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

fn partition_parent(name: &str) -> Option<String> {
    let trimmed = name.trim_end_matches(|c: char| c.is_ascii_digit());
    let base = trimmed.strip_suffix('p').unwrap_or(trimmed);
    if !base.is_empty() && base != name {
        return Some(base.to_string());
    }
    None
}

fn resolve_base_blocks(ctx: &Ctx, name: &str, depth: u8) -> Vec<String> {
    if depth > 4 || name.is_empty() {
        return Vec::new();
    }

    let slaves = ctx.list_dir_names(&format!("/sys/class/block/{name}/slaves"));
    if !slaves.is_empty() {
        let mut out = Vec::new();
        for slave in slaves {
            out.extend(resolve_base_blocks(ctx, &slave, depth + 1));
        }
        return out;
    }

    if ctx
        .read_file(&format!("/sys/class/block/{name}/partition"))
        .is_some()
    {
        if let Some(parent) = partition_parent(name) {
            return resolve_base_blocks(ctx, &parent, depth + 1);
        }
    }

    vec![name.to_string()]
}

fn parse_pcie_link_speed_gbps(raw: &str) -> Option<f64> {
    let speed = raw
        .split_whitespace()
        .next()
        .and_then(|n| n.parse::<f64>().ok())?;
    if speed >= 16.0 {
        Some(2.0)
    } else if speed >= 8.0 {
        Some(1.0)
    } else {
        Some(1.0)
    }
}

fn storage_class_gbps(ctx: &Ctx, block: &str) -> Option<f64> {
    let rotational = ctx
        .read_file(&format!("/sys/block/{block}/queue/rotational"))
        .or_else(|| ctx.read_file(&format!("/sys/class/block/{block}/queue/rotational")))
        .map(|s| s.trim().to_string());

    if block.starts_with("nvme") {
        for path in [
            format!("/sys/block/{block}/device/device/current_link_speed"),
            format!("/sys/block/{block}/device/current_link_speed"),
            format!("/sys/class/block/{block}/device/device/current_link_speed"),
            format!("/sys/class/block/{block}/device/current_link_speed"),
        ] {
            if let Some(speed) = ctx
                .read_file(&path)
                .and_then(|raw| parse_pcie_link_speed_gbps(&raw))
            {
                return Some(speed);
            }
        }
        return Some(1.0);
    }

    match rotational.as_deref() {
        Some("1") => Some(0.02),
        Some("0") => Some(0.4),
        _ => None,
    }
}

fn detect_storage_rand_read_gbps(ctx: &Ctx) -> Option<f64> {
    if let Some(override_gbps) = storage_override_gbps() {
        return Some(override_gbps);
    }

    let source = mount_source_for(ctx, ".").or_else(|| mount_source_for(ctx, "/"))?;
    let block = block_name_from_source(&source)?;
    let base_blocks = resolve_base_blocks(ctx, &block, 0);
    let mut speeds: Vec<f64> = base_blocks
        .iter()
        .filter_map(|b| storage_class_gbps(ctx, b))
        .collect();

    speeds.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    speeds.first().copied()
}

/// Group identical GPUs by (name, rounded VRAM). vLLM tensor-parallel only works
/// across IDENTICAL GPUs, so a mixed box is split into homogeneous pools. Biggest
/// pool (by total VRAM) first. Verbatim port of `_group_gpus`.
pub(crate) fn group_gpus(gpus: &[Gpu]) -> Vec<GpuGroup> {
    // Preserve insertion order of first-seen keys, like the Python `order` list.
    let mut order: Vec<(String, i64)> = Vec::new();
    let mut groups: HashMap<(String, i64), GpuGroup> = HashMap::new();

    for g in gpus {
        let key = (g.name.clone(), g.vram_gb.round() as i64);
        if !groups.contains_key(&key) {
            groups.insert(
                key.clone(),
                GpuGroup {
                    name: g.name.clone(),
                    vram_each: round1(g.vram_gb),
                    count: 0,
                    indices: Vec::new(),
                    vram_total: 0.0,
                },
            );
            order.push(key.clone());
        }
        let grp = groups.get_mut(&key).unwrap();
        grp.count += 1;
        grp.indices.push(g.index);
    }

    let mut out: Vec<GpuGroup> = Vec::with_capacity(order.len());
    for key in &order {
        let mut grp = groups.remove(key).unwrap();
        grp.vram_total = round1(grp.vram_each * grp.count as f64);
        out.push(grp);
    }
    // Stable sort by vram_total DESC (Python list.sort is stable).
    out.sort_by(|a, b| {
        b.vram_total
            .partial_cmp(&a.vram_total)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

/// Result of an NVIDIA/AMD probe — the subset of fields the gpu branch needs.
struct GpuDetect {
    gpu_name: String,
    gpu_vram_gb: f64,
    gpu_count: u32,
    gpus: Vec<Gpu>,
    gpu_groups: Vec<GpuGroup>,
    homogeneous: bool,
    backend: &'static str,
    unified_memory: Option<bool>,
}

/// Detect NVIDIA GPUs via `nvidia-smi --query-gpu=memory.total,name
/// --format=csv,noheader,nounits`. Verbatim port of `_detect_nvidia`, including
/// the remote PATH / absolute-path fallbacks and the driver-error surfacing.
fn detect_nvidia(ctx: &mut Ctx) -> Option<GpuDetect> {
    ctx.last_gpu_error = None;
    let mut out = ctx.run(&[
        "nvidia-smi",
        "--query-gpu=memory.total,name",
        "--format=csv,noheader,nounits",
    ]);

    // Remote fallback: non-interactive SSH shell often has a minimal PATH that
    // omits where nvidia-smi lives. Retry through a login shell with the common
    // CUDA bin dirs on PATH.
    if out.as_deref().unwrap_or("").is_empty() && ctx.is_remote() {
        out = ctx.run_remote_str(
            "bash -lc 'export PATH=\"$PATH:/usr/bin:/usr/local/bin:/usr/local/cuda/bin\"; \
             nvidia-smi --query-gpu=memory.total,name --format=csv,noheader,nounits'",
        );
    }
    // Last resort: nvidia-smi by absolute path.
    if out.as_deref().unwrap_or("").is_empty() && ctx.is_remote() {
        for p in [
            "/usr/bin/nvidia-smi",
            "/usr/local/bin/nvidia-smi",
            "/usr/local/cuda/bin/nvidia-smi",
        ] {
            out = ctx.run_remote_str(&format!(
                "{p} --query-gpu=memory.total,name --format=csv,noheader,nounits"
            ));
            if !out.as_deref().unwrap_or("").is_empty() {
                break;
            }
        }
    }

    let out = match out {
        Some(s) if !s.is_empty() => s,
        _ => return None,
    };

    // nvidia-smi present but unable to talk to the driver — surface as error.
    let low = out.to_lowercase();
    if low.contains("nvml")
        || low.contains("driver/library version mismatch")
        || low.contains("couldn't communicate")
        || low.contains("no devices were found")
        || low.contains("failed to initialize")
    {
        let first_line = out.trim().split('\n').next().unwrap_or("");
        let truncated: String = first_line.chars().take(140).collect();
        ctx.last_gpu_error = Some(if truncated.is_empty() {
            "NVIDIA driver error".to_string()
        } else {
            truncated
        });
        return None;
    }

    let mut gpus: Vec<Gpu> = Vec::new();
    for (idx, line) in out.trim().split('\n').enumerate() {
        let parts: Vec<&str> = line.split(',').map(|p| p.trim()).collect();
        if parts.len() >= 2 {
            if let Ok(vram_mb) = parts[0].parse::<f64>() {
                gpus.push(Gpu {
                    index: Some(idx as u32),
                    name: parts[1].to_string(),
                    vram_gb: vram_mb / 1024.0,
                });
            }
            // ValueError → skip (continue)
        }
    }

    if gpus.is_empty() {
        return None;
    }
    let total_vram: f64 = gpus.iter().map(|g| g.vram_gb).sum();
    let groups = group_gpus(&gpus);
    Some(GpuDetect {
        gpu_name: gpus[0].name.clone(),
        gpu_vram_gb: round1(total_vram),
        gpu_count: gpus.len() as u32,
        homogeneous: groups.len() <= 1,
        gpu_groups: groups,
        gpus,
        backend: "cuda",
        unified_memory: None,
    })
}

/// Detect AMD GPUs via /sys/class/drm card* sysfs (vendor 0x1002). Handles
/// discrete cards (mem_info_vram_total) and APUs/unified-memory SoCs
/// (mem_info_vis_vram_total / mem_info_gtt_total). Verbatim port of `_detect_amd`.
fn detect_amd(ctx: &Ctx) -> Option<GpuDetect> {
    // Local closure mirroring the Python nested `_read`.
    let read = |path: &str| -> Option<String> {
        if ctx.is_remote() {
            ctx.run(&["cat", path]).and_then(|v| {
                let t = v.trim().to_string();
                if t.is_empty() { None } else { Some(t) }
            })
        } else {
            std::fs::read_to_string(path).ok().map(|s| s.trim().to_string())
        }
    };

    // List DRM cards: entries starting with "card" and containing no "-".
    let list_drm_cards = || -> Vec<String> {
        if ctx.is_remote() {
            match ctx.run(&["ls", "/sys/class/drm"]) {
                Some(out) if !out.is_empty() => out
                    .split_whitespace()
                    .filter(|e| e.starts_with("card") && !e.contains('-'))
                    .map(|e| e.to_string())
                    .collect(),
                _ => Vec::new(),
            }
        } else {
            match std::fs::read_dir("/sys/class/drm") {
                Ok(rd) => rd
                    .filter_map(|e| e.ok())
                    .filter_map(|e| e.file_name().into_string().ok())
                    .filter(|e| e.starts_with("card") && !e.contains('-'))
                    .collect(),
                Err(_) => Vec::new(),
            }
        }
    };

    let mut cards: Vec<Gpu> = Vec::new();
    let mut is_apu = false;

    for (cidx, entry) in list_drm_cards().into_iter().enumerate() {
        let base = format!("/sys/class/drm/{entry}/device");
        let vendor = read(&format!("{base}/vendor"));
        if vendor.as_deref() != Some("0x1002") {
            continue;
        }

        let vram_raw = read(&format!("{base}/mem_info_vram_total"));
        let vis_raw = read(&format!("{base}/mem_info_vis_vram_total"));
        let gtt_raw = read(&format!("{base}/mem_info_gtt_total"));

        let to_int = |raw: &Option<String>| -> u64 {
            match raw {
                Some(s) if !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()) => {
                    s.parse::<u64>().unwrap_or(0)
                }
                _ => 0,
            }
        };
        let vram_val = to_int(&vram_raw);
        let vis_val = to_int(&vis_raw);
        let gtt_val = to_int(&gtt_raw);

        let mut vram_bytes = vram_val.max(vis_val);
        if vram_bytes == 0 {
            vram_bytes = gtt_val;
        }
        if vis_val != 0 && vis_val >= vram_val {
            is_apu = true;
        }
        if vram_bytes == 0 {
            continue;
        }
        let name = read(&format!("{base}/product_name"))
            .unwrap_or_else(|| format!("AMD GPU ({entry})"));
        cards.push(Gpu {
            index: Some(cidx as u32),
            name,
            vram_gb: vram_bytes as f64 / 1024f64.powi(3),
        });
    }

    if cards.is_empty() {
        return None;
    }
    let total_vram: f64 = cards.iter().map(|c| c.vram_gb).sum();
    let groups = group_gpus(&cards);
    Some(GpuDetect {
        gpu_name: cards[0].name.clone(),
        gpu_vram_gb: round1(total_vram),
        gpu_count: cards.len() as u32,
        homogeneous: groups.len() <= 1,
        gpu_groups: groups,
        gpus: cards,
        backend: "rocm",
        unified_memory: Some(is_apu),
    })
}

/// Parse /proc/meminfo into key -> KB values. Verbatim port of `_parse_meminfo`.
pub(crate) fn parse_meminfo(text: &str) -> HashMap<String, u64> {
    let mut result = HashMap::new();
    for line in text.split('\n') {
        if let Some((key, val)) = line.split_once(':') {
            let mut parts = val.trim().split_whitespace();
            if let Some(first) = parts.next() {
                if let Ok(n) = first.parse::<u64>() {
                    result.insert(key.trim().to_string(), n);
                }
                // ValueError → skip
            }
        }
    }
    result
}

fn parse_meminfo_ctx(ctx: &Ctx) -> HashMap<String, u64> {
    match ctx.read_file("/proc/meminfo") {
        Some(text) if !text.is_empty() => parse_meminfo(&text),
        _ => HashMap::new(),
    }
}

/// total RAM in GB. MemTotal/1024^2; local fallback to sysconf. Port of `_get_ram_gb`.
fn get_ram_gb(ctx: &Ctx) -> f64 {
    let meminfo = parse_meminfo_ctx(ctx);
    if let Some(&kb) = meminfo.get("MemTotal") {
        return kb as f64 / 1024f64.powi(2);
    }
    if !ctx.is_remote() {
        // os.sysconf(SC_PHYS_PAGES) * SC_PAGE_SIZE / 1024^3
        #[cfg(unix)]
        unsafe {
            let pages = libc_sysconf(SC_PHYS_PAGES);
            let page_size = libc_sysconf(SC_PAGE_SIZE);
            if pages > 0 && page_size > 0 {
                return (pages as f64 * page_size as f64) / 1024f64.powi(3);
            }
        }
    }
    0.0
}

// Minimal sysconf binding (avoids pulling in the `libc` crate, which isn't a dep
// of this crate). These names are stable across Linux/glibc and macOS.
#[cfg(unix)]
unsafe extern "C" {
    #[link_name = "sysconf"]
    fn libc_sysconf(name: i32) -> i64;
}
#[cfg(all(unix, target_os = "linux"))]
const SC_PHYS_PAGES: i32 = 85;
#[cfg(all(unix, target_os = "linux"))]
const SC_PAGE_SIZE: i32 = 30;
#[cfg(all(unix, target_os = "macos"))]
const SC_PHYS_PAGES: i32 = 200;
#[cfg(all(unix, target_os = "macos"))]
const SC_PAGE_SIZE: i32 = 29;

/// available RAM in GB. MemAvailable/1024^2 else total*0.7. Port of `_get_available_ram_gb`.
fn get_available_ram_gb(ctx: &Ctx) -> f64 {
    let meminfo = parse_meminfo_ctx(ctx);
    if let Some(&kb) = meminfo.get("MemAvailable") {
        return kb as f64 / 1024f64.powi(2);
    }
    get_ram_gb(ctx) * 0.7
}

/// CPU model name from /proc/cpuinfo "model name". Port of `_get_cpu_name`.
fn get_cpu_name(ctx: &Ctx) -> String {
    if let Some(text) = ctx.read_file("/proc/cpuinfo") {
        for line in text.split('\n') {
            if line.starts_with("model name") {
                if let Some((_, v)) = line.split_once(':') {
                    return v.trim().to_string();
                }
            }
        }
    }
    if !ctx.is_remote() {
        // platform.processor() — typically empty on Linux; Python falls back to
        // "unknown". We mirror that fallback directly.
        return "unknown".to_string();
    }
    "unknown".to_string()
}

/// CPU logical-core count. Port of `_get_cpu_count`.
fn get_cpu_count(ctx: &Ctx) -> u32 {
    if ctx.is_remote() {
        if let Some(out) = ctx.run(&["nproc"]) {
            if let Ok(n) = out.trim().parse::<u32>() {
                return n;
            }
        }
        // fallback: count "processor" lines in /proc/cpuinfo
        if let Some(text) = ctx.read_file("/proc/cpuinfo") {
            return text
                .split('\n')
                .filter(|line| line.starts_with("processor"))
                .count() as u32;
        }
        return 1;
    }
    // local: os.cpu_count() or 1
    std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(1)
}

/// Local machine architecture string (lowercased), for the CPU backend branch.
fn local_arch() -> String {
    std::env::consts::ARCH.to_lowercase()
}

/// aarch64/arm → cpu_arm else cpu_x86. Port of the backend rule.
pub(crate) fn classify_cpu_backend(arch: &str) -> &'static str {
    if arch.contains("aarch64") || arch.contains("arm") {
        "cpu_arm"
    } else {
        "cpu_x86"
    }
}

// ── per-host cache: host -> (probed_at, SystemInfo) ──
fn cache() -> &'static Mutex<HashMap<String, (Instant, SystemInfo)>> {
    static CACHE: OnceLock<Mutex<HashMap<String, (Instant, SystemInfo)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Local + remote probe. host="" → local /proc + nvidia-smi/sysfs.
/// host="user@server" → SSH. plat ∈ {"", "linux", "windows", "termux"}.
/// fresh=true bypasses the 1800s per-host cache.
pub fn detect_system(host: &str, ssh_port: &str, plat: &str, fresh: bool) -> SystemInfo {
    let cache_key = if host.is_empty() { "_local".to_string() } else { host.to_string() };

    if !fresh {
        if let Ok(map) = cache().lock() {
            if let Some((ts, cached)) = map.get(&cache_key) {
                if ts.elapsed() < CACHE_TTL {
                    return cached.clone();
                }
            }
        }
    }

    let mut ctx = Ctx::new(host, ssh_port);

    // Windows: PowerShell parity is deferred. Return an error SystemInfo so the
    // caller gets a sensible "can't probe" result rather than a panic.
    if plat == "windows" && ctx.is_remote() {
        let result = SystemInfo {
            error: Some(format!("Windows probing not implemented (host {host})")),
            cpu_cores: 1,
            ..Default::default()
        };
        store(&cache_key, &result);
        return result;
    }

    // Linux/Termux multi-command detection.
    let total_ram = round1(get_ram_gb(&ctx));
    // If remote host returns 0 RAM, connection likely failed.
    if ctx.is_remote() && total_ram <= 0.0 {
        let result = SystemInfo {
            error: Some(format!("Cannot connect to {host}")),
            cpu_cores: 1,
            ..Default::default()
        };
        store(&cache_key, &result);
        return result;
    }

    let available_ram = round1(get_available_ram_gb(&ctx));
    let cpu_cores = get_cpu_count(&ctx);
    let cpu_name = get_cpu_name(&ctx);
    let storage_rand_read_gbps = detect_storage_rand_read_gbps(&ctx);

    let gpu_info = match detect_nvidia(&mut ctx) {
        Some(g) => Some(g),
        None => detect_amd(&ctx),
    };

    let result = if let Some(g) = gpu_info {
        SystemInfo {
            total_ram_gb: total_ram,
            available_ram_gb: available_ram,
            cpu_cores,
            cpu_name,
            has_gpu: true,
            gpu_name: Some(g.gpu_name),
            gpu_vram_gb: Some(g.gpu_vram_gb),
            gpu_count: g.gpu_count,
            gpus: g.gpus,
            gpu_groups: g.gpu_groups,
            backend: g.backend.to_string(),
            homogeneous: g.homogeneous,
            unified_memory: g.unified_memory,
            gpu_error: None,
            error: None,
            gpu_only: false,
            storage_rand_read_gbps,
        }
    } else {
        let arch_out = if ctx.is_remote() {
            ctx.run(&["uname", "-m"]).unwrap_or_default()
        } else {
            local_arch()
        };
        let backend = classify_cpu_backend(&arch_out);
        SystemInfo {
            total_ram_gb: total_ram,
            available_ram_gb: available_ram,
            cpu_cores,
            cpu_name,
            has_gpu: false,
            gpu_name: None,
            gpu_vram_gb: None,
            gpu_count: 0,
            gpus: Vec::new(),
            gpu_groups: Vec::new(),
            backend: backend.to_string(),
            homogeneous: false,
            unified_memory: None,
            gpu_error: ctx.last_gpu_error.clone(),
            error: None,
            gpu_only: false,
            storage_rand_read_gbps,
        }
    };

    store(&cache_key, &result);
    result
}

fn store(cache_key: &str, result: &SystemInfo) {
    if let Ok(mut map) = cache().lock() {
        map.insert(cache_key.to_string(), (Instant::now(), result.clone()));
    }
}

/// Convenience: detect_system("", "", "", false).
pub fn detect_local() -> SystemInfo {
    detect_system("", "", "", false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_meminfo_extracts_kb() {
        let txt = "MemTotal:       32791234 kB\nMemAvailable:   16000000 kB\nBad line\nSwapTotal: 0 kB\n";
        let m = parse_meminfo(txt);
        assert_eq!(m.get("MemTotal"), Some(&32791234));
        assert_eq!(m.get("MemAvailable"), Some(&16000000));
        assert_eq!(m.get("SwapTotal"), Some(&0));
        assert!(!m.contains_key("Bad line"));
    }

    #[test]
    fn group_gpus_buckets_and_sorts() {
        let gpus = vec![
            Gpu { index: Some(0), name: "RTX 3070".into(), vram_gb: 8.0 },
            Gpu { index: Some(1), name: "RTX 3090".into(), vram_gb: 24.0 },
            Gpu { index: Some(2), name: "RTX 3090".into(), vram_gb: 24.0 },
        ];
        let groups = group_gpus(&gpus);
        // Two distinct groups; 3090 pool (48 total) sorts before 3070 pool (8).
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].name, "RTX 3090");
        assert_eq!(groups[0].count, 2);
        assert_eq!(groups[0].vram_total, 48.0);
        assert_eq!(groups[0].indices, vec![Some(1), Some(2)]);
        assert_eq!(groups[1].name, "RTX 3070");
        assert_eq!(groups[1].count, 1);
    }

    #[test]
    fn cpu_backend_classification() {
        assert_eq!(classify_cpu_backend("x86_64"), "cpu_x86");
        assert_eq!(classify_cpu_backend("aarch64"), "cpu_arm");
        assert_eq!(classify_cpu_backend("armv7l"), "cpu_arm");
    }
}
