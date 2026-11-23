//! Native WebGPU backend shared by the PaddleOCR detector and recognizer.
use anyhow::{ensure, Result};
use ort::environment::EnvironmentBuilder;
use ort::ep::ArbitrarilyConfigurableExecutionProvider;

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
        let mut ep = ort::ep::WebGPU::default();
        if let Ok(index) = std::env::var(reader_core::routing::WEBGPU_ADAPTER_ENV) {
            ep = ep.with_arbitrary_config("adapterIndex", index.parse::<u32>()?.to_string());
        }
        let ep = ep.build().error_on_failure();
        Ok((environment.with_execution_providers([ep]), true))
    }
}
