// Ported from mojovoice (MIT, Copyright (c) 2024-2026 itsdevcoffee).

use candle_core::Device;
use tracing::info;

/// Select the best available Candle device at runtime.
///
/// CUDA and Metal are attempted only when their Cargo features are compiled in.
/// Default builds never require GPU toolkits and always fall back to CPU.
pub fn select_device() -> Device {
    #[cfg(feature = "cuda")]
    {
        match Device::new_cuda(0) {
            Ok(device) => {
                info!("Using CUDA device");
                return device;
            }
            Err(err) => {
                tracing::warn!("CUDA initialization failed: {err}");
            }
        }
    }

    #[cfg(feature = "metal")]
    {
        match Device::new_metal(0) {
            Ok(device) => {
                info!("Using Metal device");
                return device;
            }
            Err(err) => {
                tracing::warn!("Metal initialization failed: {err}");
            }
        }
    }

    info!("Using CPU device");
    Device::Cpu
}

pub fn device_label(device: &Device) -> &'static str {
    match device {
        Device::Cpu => "cpu",
        Device::Cuda(_) => "cuda",
        Device::Metal(_) => "metal",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(not(any(feature = "cuda", feature = "metal")))]
    fn default_build_selects_cpu_without_panicking() {
        let device = select_device();
        assert!(matches!(device, Device::Cpu));
        assert_eq!(device_label(&device), "cpu");
    }
}
