// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let mut arguments = std::env::args_os().skip(1);
    if arguments.next().as_deref() == Some(std::ffi::OsStr::new("--scerite-session-manager")) {
        let Some(app_data_dir) = arguments.next() else {
            std::process::exit(2);
        };
        if scerite_lib::run_session_manager(app_data_dir.into()).is_err() {
            std::process::exit(1);
        }
        return;
    }

    scerite_lib::run()
}
