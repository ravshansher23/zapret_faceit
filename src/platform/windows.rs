use std::path::Path;

use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE, HANDLE};
use windows_sys::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows_sys::Win32::System::Console::{SetConsoleCP, SetConsoleOutputCP};
use windows_sys::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard};
use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Memory::{GlobalLock, GlobalUnlock};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, TerminateProcess,
    PROCESS_QUERY_INFORMATION, PROCESS_TERMINATE, PROCESS_VM_READ,
};
use windows_sys::Win32::UI::Shell::ShellExecuteW;

use crate::server::steam_library_paths;

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

pub fn prepare_console() {
    unsafe {
        SetConsoleOutputCP(65001);
        SetConsoleCP(65001);
    }
}

pub fn pause() {
    eprintln!();
    eprintln!("Enter — закрыть окно.");
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
}

pub fn read_clipboard() -> Result<String, String> {
    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return Err("буфер обмена занят".into());
        }
        let _guard = ClipboardGuard;
        let data = GetClipboardData(13);
        if data.is_null() {
            return Err("в буфере нет текста".into());
        }
        let ptr = GlobalLock(data) as *const u16;
        if ptr.is_null() {
            return Err("не удалось прочитать буфер обмена".into());
        }
        let mut len = 0usize;
        while *ptr.add(len) != 0 {
            len += 1;
            if len > 1_000_000 {
                break;
            }
        }
        let text = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len));
        GlobalUnlock(data);
        if text.trim().is_empty() {
            return Err("буфер обмена пуст".into());
        }
        Ok(text)
    }
}

pub fn cs2_command_lines() -> Vec<String> {
    process_ids("cs2.exe")
        .into_iter()
        .filter_map(command_line)
        .collect()
}

pub fn console_log_text() -> Option<String> {
    let paths = console_log_paths();
    let mut newest: Option<(std::time::SystemTime, std::path::PathBuf)> = None;
    for path in paths {
        let Ok(meta) = path.metadata() else { continue };
        let Ok(modified) = meta.modified() else { continue };
        let replace = newest.as_ref().map(|(time, _)| modified >= *time).unwrap_or(true);
        if replace {
            newest = Some((modified, path));
        }
    }
    let (_, path) = newest?;
    read_tail(&path, 65_536)
}

pub fn is_elevated() -> bool {
    unsafe {
        let mut token = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let _token = OwnedHandle(token);
        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut returned = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut elevation as *mut TOKEN_ELEVATION as *mut _,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        );
        ok != 0 && elevation.TokenIsElevated != 0
    }
}

pub fn relaunch_elevated(args: &[String]) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|err| err.to_string())?;
    let directory = exe.parent().unwrap_or_else(|| Path::new("."));
    let operation = wide("runas");
    let file = wide(&exe.display().to_string());
    let params = wide(&args.join(" "));
    let cwd = wide(&directory.display().to_string());
    let code = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            file.as_ptr(),
            params.as_ptr(),
            cwd.as_ptr(),
            1,
        )
    };
    if (code as isize) <= 32 {
        return Err("запрос администратора отменён".into());
    }
    Ok(())
}

pub fn stop_winws() -> Result<u32, String> {
    let mut stopped = 0u32;
    for pid in process_ids("winws.exe") {
        let Some(command) = command_line(pid) else {
            continue;
        };
        if !command.contains("list-faceit.txt") {
            continue;
        }
        unsafe {
            let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
            if handle.is_null() {
                continue;
            }
            let _guard = OwnedHandle(handle);
            if TerminateProcess(handle, 0) != 0 {
                stopped += 1;
            }
        }
    }
    Ok(stopped)
}

pub fn exclude_from_defender(folder: &Path) {
    let folder = folder.display().to_string().replace('\'', "''");
    let script = format!(
        "$p='{folder}'; if (Get-Command Add-MpPreference -ErrorAction SilentlyContinue) {{ Add-MpPreference -ExclusionPath $p -ErrorAction SilentlyContinue; Add-MpPreference -ExclusionProcess 'winws.exe' -ErrorAction SilentlyContinue; Add-MpPreference -ExclusionProcess 'faceit-bypass.exe' -ErrorAction SilentlyContinue }}"
    );
    let _ = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .status();
}

pub fn spawn_detached(exe: &Path, args: &[String], cwd: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    std::process::Command::new(exe)
        .args(args)
        .current_dir(cwd)
        .creation_flags(CREATE_NEW_CONSOLE | CREATE_NEW_PROCESS_GROUP)
        .spawn()
        .map(|_| ())
        .map_err(|err| format!("не удалось запустить winws.exe: {err}"))
}

struct ClipboardGuard;

impl Drop for ClipboardGuard {
    fn drop(&mut self) {
        unsafe {
            CloseClipboard();
        }
    }
}

fn console_log_paths() -> Vec<std::path::PathBuf> {
    let mut roots = Vec::new();
    let program_files = std::env::var("ProgramFiles(x86)")
        .unwrap_or_else(|_| r"C:\Program Files (x86)".to_string());
    let vdf = Path::new(&program_files)
        .join("Steam")
        .join("steamapps")
        .join("libraryfolders.vdf");
    if let Ok(text) = std::fs::read_to_string(&vdf) {
        for path in steam_library_paths(&text) {
            roots.push(std::path::PathBuf::from(path));
        }
    }
    roots.push(std::path::PathBuf::from(&program_files).join("Steam"));
    let mut logs = Vec::new();
    for root in roots {
        let log = root
            .join("steamapps")
            .join("common")
            .join("Counter-Strike Global Offensive")
            .join("game")
            .join("csgo")
            .join("console.log");
        if !logs.contains(&log) {
            logs.push(log);
        }
    }
    logs
}

fn read_tail(path: &Path, max: u64) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    if len > max {
        file.seek(SeekFrom::End(-(max as i64))).ok()?;
    }
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

struct Proc {
    pid: u32,
    name: String,
}

fn process_ids(image: &str) -> Vec<u32> {
    snapshot()
        .unwrap_or_default()
        .into_iter()
        .filter(|entry| entry.name.eq_ignore_ascii_case(image))
        .map(|entry| entry.pid)
        .collect()
}

fn snapshot() -> Result<Vec<Proc>, ()> {
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return Err(());
        }
        let _guard = OwnedHandle(snap);
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        if Process32FirstW(snap, &mut entry) == 0 {
            return Ok(Vec::new());
        }
        let mut list = Vec::new();
        loop {
            list.push(Proc {
                pid: entry.th32ProcessID,
                name: exe_name(&entry),
            });
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            if Process32NextW(snap, &mut entry) == 0 {
                break;
            }
        }
        Ok(list)
    }
}

fn exe_name(entry: &PROCESSENTRY32W) -> String {
    let len = entry
        .szExeFile
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(entry.szExeFile.len());
    String::from_utf16_lossy(&entry.szExeFile[..len])
}

#[cfg(target_pointer_width = "64")]
fn command_line(pid: u32) -> Option<String> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid);
        if handle.is_null() {
            return None;
        }
        let _guard = OwnedHandle(handle);
        let mut info = ProcessBasicInformation {
            reserved1: std::ptr::null_mut(),
            peb: std::ptr::null_mut(),
            reserved2: [std::ptr::null_mut(); 2],
            pid: 0,
            parent_pid: 0,
        };
        let status = NtQueryInformationProcess(
            handle,
            0,
            &mut info as *mut ProcessBasicInformation as *mut _,
            std::mem::size_of::<ProcessBasicInformation>() as u32,
            std::ptr::null_mut(),
        );
        if status != 0 || info.peb.is_null() {
            return None;
        }
        let params = read_pod::<*mut std::ffi::c_void>(handle, info.peb.byte_add(0x20))?;
        if params.is_null() {
            return None;
        }
        let command = read_pod::<UnicodeString>(handle, params.byte_add(0x70))?;
        if command.length == 0 || command.buffer.is_null() {
            return None;
        }
        let words = (command.length as usize) / 2;
        if words > 32_768 {
            return None;
        }
        let mut buffer = vec![0u16; words];
        let mut read = 0usize;
        let ok = ReadProcessMemory(
            handle,
            command.buffer as *const _,
            buffer.as_mut_ptr() as *mut _,
            command.length as usize,
            &mut read,
        );
        if ok == 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buffer[..read / 2]))
    }
}

#[cfg(not(target_pointer_width = "64"))]
fn command_line(_pid: u32) -> Option<String> {
    None
}

#[cfg(target_pointer_width = "64")]
fn read_pod<T: Copy>(process: HANDLE, addr: *const std::ffi::c_void) -> Option<T> {
    unsafe {
        let mut value = std::mem::MaybeUninit::<T>::uninit();
        let mut read = 0usize;
        let ok = ReadProcessMemory(
            process,
            addr,
            value.as_mut_ptr() as *mut _,
            std::mem::size_of::<T>(),
            &mut read,
        );
        if ok == 0 || read != std::mem::size_of::<T>() {
            return None;
        }
        Some(value.assume_init())
    }
}

#[cfg(target_pointer_width = "64")]
#[repr(C)]
struct ProcessBasicInformation {
    reserved1: *mut std::ffi::c_void,
    peb: *mut std::ffi::c_void,
    reserved2: [*mut std::ffi::c_void; 2],
    pid: usize,
    parent_pid: usize,
}

#[cfg(target_pointer_width = "64")]
#[repr(C)]
#[derive(Clone, Copy)]
struct UnicodeString {
    length: u16,
    maximum_length: u16,
    buffer: *const u16,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(std::mem::size_of::<UnicodeString>() == 16);
#[cfg(target_pointer_width = "64")]
const _: () = assert!(std::mem::offset_of!(UnicodeString, buffer) == 8);

#[cfg(target_pointer_width = "64")]
#[link(name = "ntdll")]
extern "system" {
    fn NtQueryInformationProcess(
        process: HANDLE,
        class: u32,
        info: *mut std::ffi::c_void,
        length: u32,
        returned: *mut u32,
    ) -> i32;
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}
