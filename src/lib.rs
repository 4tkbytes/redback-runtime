#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn android_main() {
    #[cfg(debug_assertions)]
    {
        unsafe { std::env::set_var("RUST_BACKTRACE", "full"); }
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