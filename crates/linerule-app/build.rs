//! Build script: embed Windows manifest (DPI v2, longPathAware) into the
//! resulting `linerule.exe` on Windows targets. No-op on other targets so
//! cross-checks under Linux still build cleanly.

fn main() {
    #[cfg(target_os = "windows")]
    {
        let _ = embed_resource::compile("app.manifest", embed_resource::NONE);
    }
}
