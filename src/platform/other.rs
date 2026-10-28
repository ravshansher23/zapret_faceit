use std::path::Path;

pub fn prepare_console() {}

pub fn pause() {}

pub fn read_clipboard() -> Result<String, String> {
    Err("буфер обмена читается в сборке для Windows".into())
}

pub fn cs2_command_lines() -> Vec<String> {
    Vec::new()
}

pub fn console_log_text() -> Option<String> {
    None
}

pub fn stop_winws() -> Result<u32, String> {
    Err("остановка winws доступна в сборке для Windows".into())
}

#[allow(dead_code)]
pub fn is_elevated() -> bool {
    false
}

#[allow(dead_code)]
pub fn relaunch_elevated(_args: &[String]) -> Result<(), String> {
    Err("запуск winws доступен в сборке для Windows".into())
}

#[allow(dead_code)]
pub fn exclude_from_defender(_folder: &Path) {}

#[allow(dead_code)]
pub fn spawn_detached(_exe: &Path, _args: &[String], _cwd: &Path) -> Result<(), String> {
    Err("winws запускается на Windows".into())
}
