//! Native WebGPU backend shared by the PaddleOCR detector and recognizer.
use anyhow::{ensure, Result};
use ort::environment::EnvironmentBuilder;
use ort::ep::ArbitrarilyConfigurableExecutionProvider;
use std::path::{Path, PathBuf};

pub fn ort_library(runtime: &Path) -> PathBuf {
    let lib = runtime.join("lib");
    let dir = if lib.is_dir() { lib.as_path() } else { runtime };
    dir.join(format!(
        "{}onnxruntime{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    ))
}

fn provider(index: Option<u32>) -> ort::ep::WebGPU {
    let provider = ort::ep::WebGPU::default();
    match index {
        Some(index) => provider.with_arbitrary_config("adapterIndex", index.to_string()),
        None => provider.with_arbitrary_config("powerPreference", "high-performance"),
    }
}

pub fn probe(runtime: &Path) -> Result<()> {
    let library_path = ort_library(runtime);
    #[cfg(windows)]
    reader_core::native_paths::configure_windows_dll_search(runtime, &library_path)?;

    #[derive(serde::Serialize)]
    struct Adapter {
        index: u32,
        kind: u32,
        vendor_id: u32,
        device_id: u32,
        name: Option<String>,
        available: bool,
    }

    use std::ffi::{c_char, c_void};
    type Visitor = unsafe extern "system" fn(*mut c_void, u32, u32, u32, u32, *const c_char, usize);
    type Query = unsafe extern "system" fn(Visitor, *mut c_void) -> i32;

    unsafe extern "system" fn collect(
        user_data: *mut c_void,
        index: u32,
        kind: u32,
        vendor_id: u32,
        device_id: u32,
        name: *const c_char,
        length: usize,
    ) {
        // The native query calls the visitor synchronously. It owns the
        // borrowed name until the callback returns.
        let adapters = unsafe { &mut *user_data.cast::<Vec<Adapter>>() };
        let name = (!name.is_null() && (1..=4096).contains(&length)).then(|| {
            let bytes = unsafe { std::slice::from_raw_parts(name.cast::<u8>(), length) };
            String::from_utf8_lossy(bytes).into_owned()
        });
        adapters.push(Adapter {
            index,
            kind,
            vendor_id,
            device_id,
            name,
            available: true,
        });
    }

    let mut adapters = Vec::<Adapter>::new();
    unsafe {
        let library = libloading::Library::new(library_path)?;
        let query = library.get::<Query>(b"SmallTalkAIWebGpuVisitAdaptersV1")?;
        ensure!(
            query(collect, (&mut adapters as *mut Vec<Adapter>).cast()) == 0,
            "Dawn adapter enumeration failed"
        );
    }
    println!("{}", serde_json::to_string(&adapters)?);
    Ok(())
}

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
        ensure!(device != "cpu", "Dawn is incompatible with --device cpu");
        let index = std::env::var(reader_core::routing::WEBGPU_ADAPTER_ENV)
            .ok()
            .map(|index| index.parse::<u32>())
            .transpose()?;
        let ep = provider(index);
        let ep = ep.build().error_on_failure();
        Ok((environment.with_execution_providers([ep]), true))
    }
}
