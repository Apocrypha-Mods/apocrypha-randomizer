//! `apoc_native.dll`: the randomizer's own module inside the game, loaded by
//! me3 as a native alongside the mod folder. It is where options that depend
//! on what the player does during play live, starting with auto-upgrade
//! (docs/spec/items.md §2.7).
//!
//! Built for Windows (`cargo build -p native --target
//! x86_64-pc-windows-gnu`); on other targets it is an empty library, so the
//! workspace still builds and tests everywhere.
//!
//! For now it only proves it is loaded: on attach it appends a line to the
//! file `APOC_NATIVE_LOG` names (a Windows path), from its own thread, as
//! file work under the loader lock is unsafe.

#[cfg(windows)]
mod windows {
    use std::ffi::c_void;
    use std::io::Write;

    const DLL_PROCESS_ATTACH: u32 = 1;

    fn note(line: &str) {
        let Ok(path) = std::env::var("APOC_NATIVE_LOG") else {
            return;
        };
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(f, "{line}");
        }
    }

    /// Called by Windows as the module is loaded and unloaded.
    #[unsafe(no_mangle)]
    #[allow(non_snake_case)]
    pub extern "system" fn DllMain(
        _module: *mut c_void,
        reason: u32,
        _reserved: *mut c_void,
    ) -> i32 {
        if reason == DLL_PROCESS_ATTACH {
            std::thread::spawn(|| {
                note(&format!(
                    "apoc_native {} loaded into process {}",
                    env!("CARGO_PKG_VERSION"),
                    std::process::id()
                ))
            });
        }
        1
    }
}
