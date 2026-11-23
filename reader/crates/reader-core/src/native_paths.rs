//! Library discovery for optional installed runtimes; never loads native code.
use crate::routing::{asset_receipt, runtime_asset_installed};
use anyhow::{Context, Result};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

pub fn runtime_library_directories(runtime: &Path) -> Vec<PathBuf> {
    let mut paths = vec![runtime.join("lib"), runtime.join("bin")];
    if !runtime.as_os_str().is_empty() {
        paths.push(runtime.to_owned());
    }
    if let (Some(root), Some(asset)) = (
        runtime.parent(),
        runtime
            .file_name()
            .and_then(OsStr::to_str)
            .and_then(asset_receipt),
    ) {
        for id in &asset.dependencies {
            let Some(dependency) = asset_receipt(id) else {
                continue;
            };
            for file in &dependency.required {
                let path = root.join(id).join(file);
                if let Some(directory) = path.parent().filter(|dir| !paths.iter().any(|p| p == dir))
                {
                    paths.push(directory.to_owned());
                }
            }
        }
    }
    paths
}

pub fn configure_worker_libraries(
    command: &mut std::process::Command,
    runtime: &Path,
) -> Result<()> {
    if runtime.join(".verified").is_file() {
        if let (Some(root), Some(id)) = (
            runtime.parent(),
            runtime.file_name().and_then(OsStr::to_str),
        ) {
            if asset_receipt(id).is_some_and(|asset| !asset.dependencies.is_empty()) {
                anyhow::ensure!(
                    runtime_asset_installed(id, root),
                    "Runtime {id} or its shared libraries need updating. Download NVIDIA in Settings."
                );
            }
        }
    }
    let key = match std::env::consts::OS {
        "windows" => "PATH",
        "macos" => "DYLD_LIBRARY_PATH",
        _ => "LD_LIBRARY_PATH",
    };
    command.env(
        key,
        library_search_path(runtime, std::env::var_os(key).as_deref())?,
    );
    Ok(())
}

/// Prefer the chosen runtime and its pinned dependencies over inherited installs.
pub fn library_search_path(runtime: &Path, inherited: Option<&OsStr>) -> Result<OsString> {
    let mut paths = runtime_library_directories(runtime);
    if let Some(inherited) = inherited {
        for path in std::env::split_paths(inherited).filter(|path| !path.as_os_str().is_empty()) {
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    std::env::join_paths(paths).context("Constructing native runtime library search path")
}

/// Register the selected directories in a disposable inference worker only.
#[cfg(windows)]
pub fn configure_windows_dll_search(runtime: &Path, library: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use std::sync::Mutex;
    use windows_sys::Win32::System::LibraryLoader::{
        AddDllDirectory, SetDefaultDllDirectories, LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
    };

    anyhow::ensure!(
        unsafe { SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_DEFAULT_DIRS) } != 0,
        "Setting the Windows DLL search policy failed: {}",
        std::io::Error::last_os_error()
    );
    let directory = library
        .parent()
        .context("Native library has no directory")?;
    let paths = runtime_library_directories(runtime);
    static REGISTERED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
    let mut registered = REGISTERED.lock().expect("native DLL search");
    for path in std::iter::once(directory)
        .chain(
            paths
                .iter()
                .map(PathBuf::as_path)
                .filter(|path| *path != directory),
        )
        .filter(|path| path.is_dir())
    {
        let path = path.canonicalize()?;
        if registered.contains(&path) {
            continue;
        }
        let wide: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        anyhow::ensure!(
            !unsafe { AddDllDirectory(wide.as_ptr()) }.is_null(),
            "Registering Windows DLL directory {} failed: {}",
            path.display(),
            std::io::Error::last_os_error()
        );
        registered.push(path);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_nvidia_dependencies_precede_inherited_libraries() {
        let runtime = Path::new("assets/sherpa-cuda");
        let inherited =
            std::env::join_paths([Path::new("legacy/lib"), &runtime.join("lib")]).unwrap();
        let search = library_search_path(runtime, Some(&inherited)).unwrap();
        let paths = std::env::split_paths(&search).collect::<Vec<_>>();
        let mut expected = runtime_library_directories(runtime);
        expected.push(PathBuf::from("legacy/lib"));
        assert_eq!(paths, expected);
        assert!(paths.contains(&PathBuf::from("assets/cuda-cublas/nvidia/cublas/lib")));
        assert!(paths.contains(&PathBuf::from("assets/cuda-nvrtc/nvidia/cuda_nvrtc/lib")));
    }

    #[test]
    fn both_cuda_engines_share_the_same_runtime_and_blas_on_each_platform() {
        for (suffix, library_dir) in [("", "lib"), ("-windows", "bin")] {
            let parakeet =
                runtime_library_directories(Path::new(&format!("assets/parakeet-cuda{suffix}")));
            let sherpa =
                runtime_library_directories(Path::new(&format!("assets/sherpa-cuda{suffix}")));
            for (asset, wheel_dir) in [("cuda-runtime", "cuda_runtime"), ("cuda-cublas", "cublas")]
            {
                let path = PathBuf::from(format!(
                    "assets/{asset}{suffix}/nvidia/{wheel_dir}/{library_dir}"
                ));
                assert!(parakeet.contains(&path), "{path:?}");
                assert!(sherpa.contains(&path), "{path:?}");
            }
        }
    }

    #[test]
    fn processor_runtime_does_not_pick_up_nvidia_dependencies_or_current_directory() {
        let runtime = Path::new("assets/sherpa-cpu");
        let search = library_search_path(runtime, Some(OsStr::new(""))).unwrap();
        assert_eq!(
            std::env::split_paths(&search).collect::<Vec<_>>(),
            runtime_library_directories(runtime)
        );
    }
}
