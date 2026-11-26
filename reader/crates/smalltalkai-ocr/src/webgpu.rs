//! Native WebGPU backend shared by the PaddleOCR detector and recognizer.
use anyhow::{ensure, Result};
use ort::environment::EnvironmentBuilder;
use ort::ep::ArbitrarilyConfigurableExecutionProvider;

#[cfg(target_os = "linux")]
pub fn retain_environment_until_exit() -> Result<()> {
    use ort::environment::Environment;
    use std::sync::{Arc, OnceLock};

    // ort's finalizer can release the Dawn environment after the Vulkan driver
    // has shut down. Keep one reference for the lifetime of this OCR worker.
    // Model sessions still drop normally, and process exit releases the device.
    static ENVIRONMENT: OnceLock<Arc<Environment>> = OnceLock::new();
    if ENVIRONMENT.get().is_none() {
        let _ = ENVIRONMENT.set(Environment::current()?);
    }
    Ok(())
}

#[derive(clap::Args, Debug, Default)]
pub struct Options {
    /// Use a native ONNX Runtime build with WebGPU compiled in.
    #[arg(long)]
    webgpu: bool,
    /// Write verbose ONNX Runtime operator placement to the worker log.
    #[arg(long, requires = "webgpu")]
    pub webgpu_trace: bool,
}

impl Options {
    pub fn configure(
        &self,
        environment: EnvironmentBuilder,
        device: &str,
    ) -> Result<(EnvironmentBuilder, bool)> {
        if !self.webgpu {
            return Ok((environment, false));
        }
        ensure!(device != "cpu", "WebGPU is incompatible with --device cpu");
        #[cfg(target_os = "linux")]
        reader_core::vulkan::require_webgpu_adapter()?;
        let mut ep = ort::ep::WebGPU::default();
        if let Ok(index) = std::env::var(reader_core::routing::WEBGPU_ADAPTER_ENV) {
            ep = ep.with_arbitrary_config("adapterIndex", index.parse::<u32>()?.to_string());
        }
        let ep = ep.build().error_on_failure();
        Ok((environment.with_execution_providers([ep]), true))
    }
}
