// This is just for android, as it uses a dynamic library to load itself. On top of that, it uses
// logcat for logging instead of env_logger (as in PC). There might be some useful stuff in this
// library later on, but for now, its useless...

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn android_main() {
    #[cfg(debug_assertions)]
    {
        unsafe {
            std::env::set_var("RUST_BACKTRACE", "full");
        }
        android_logger::init_once(
            android_logger::Config::default().with_max_level(log::Level::Trace.to_level_filter()),
        );
    }

    std::thread::spawn(|| {
        if let Err(e) = run() {
            log::error!("Runtime failed: {:?}", e);
        }
    });
}
