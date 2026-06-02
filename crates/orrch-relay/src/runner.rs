//! Builds the `llama-server` command line for deepseek-v4-flash on a
//! RAM-constrained + NVMe box, and supervises the process (restart on exit).
use std::process::Command;

#[derive(Debug, Clone)]
pub struct RunnerConfig {
    pub llama_server_bin: String,
    pub gguf_path: String,
    pub gpu_layers: u32,
    pub n_cpu_moe: u32, // expert layers kept CPU/RAM side (mmap-streamed)
    pub ctx_size: u32,
    pub port: u16,
}

impl RunnerConfig {
    /// Sane defaults for an 8GB-VRAM / ~27GB-RAM / NVMe box per the spec.
    pub fn deepseek_default(gguf_path: impl Into<String>) -> Self {
        Self {
            llama_server_bin: "llama-server".into(),
            gguf_path: gguf_path.into(),
            gpu_layers: 999, // offload attention/dense to GPU; experts go CPU-side
            n_cpu_moe: 999,  // keep all MoE expert tensors CPU/mmap side
            ctx_size: 8192,  // OPERATING context, not the model's 1M max
            port: 8080,
        }
    }

    /// The argv `llama-server` is launched with (order-stable for testing).
    pub fn argv(&self) -> Vec<String> {
        vec![
            "-m".into(), self.gguf_path.clone(),
            "--n-gpu-layers".into(), self.gpu_layers.to_string(),
            "--n-cpu-moe".into(), self.n_cpu_moe.to_string(),
            "--ctx-size".into(), self.ctx_size.to_string(),
            "--mmap".into(),
            "--host".into(), "127.0.0.1".into(),
            "--port".into(), self.port.to_string(),
        ]
    }

    pub fn command(&self) -> Command {
        let mut c = Command::new(&self.llama_server_bin);
        c.args(self.argv());
        c
    }
}

use std::time::Duration;

/// Launch llama-server and restart it if it exits. Runs until killed externally.
/// Intended to be spawned on the tokio runtime.
pub async fn supervise(cfg: RunnerConfig) {
    loop {
        tracing::info!("starting llama-server: {:?}", cfg.argv());
        match cfg.command().spawn() {
            Ok(mut child) => {
                let status = tokio::task::spawn_blocking(move || child.wait())
                    .await
                    .ok()
                    .and_then(|r| r.ok());
                tracing::warn!("llama-server exited ({status:?}); restarting in 3s");
            }
            Err(e) => tracing::error!("failed to spawn llama-server: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn argv_uses_cpu_moe_mmap_and_operating_context() {
        let cfg = RunnerConfig::deepseek_default("/models/dsv4f-Q4_K_M.gguf");
        let argv = cfg.argv();
        assert!(argv.windows(2).any(|w| w == ["--n-cpu-moe", "999"]));
        assert!(argv.contains(&"--mmap".to_string()));
        assert!(argv.windows(2).any(|w| w == ["--ctx-size", "8192"]),
            "operating context, not the model's 1M max");
        assert!(argv.windows(2).any(|w| w[0] == "-m" && w[1].ends_with(".gguf")));
    }
}
