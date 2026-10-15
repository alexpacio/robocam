use super::*;
use windows_sys::Win32::Security::{Authorization::ConvertSidToStringSidW, *};

pub(super) unsafe fn ui_access() -> Option<bool> {
    let mut token = null_mut();
    if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
        return None;
    }
    let mut enabled = 0u32;
    let mut returned = 0;
    let success = GetTokenInformation(
        token,
        TokenUIAccess,
        &mut enabled as *mut _ as *mut c_void,
        size_of::<u32>() as u32,
        &mut returned,
    ) != 0;
    CloseHandle(token);
    success.then_some(enabled != 0)
}

pub(super) unsafe fn elevated() -> bool {
    let mut token = null_mut();
    if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
        return false;
    }
    let mut elevation: TOKEN_ELEVATION = zeroed();
    let mut returned = 0;
    let success = GetTokenInformation(
        token,
        TokenElevation,
        &mut elevation as *mut _ as *mut c_void,
        size_of::<TOKEN_ELEVATION>() as u32,
        &mut returned,
    ) != 0;
    CloseHandle(token);
    success && elevation.TokenIsElevated != 0
}

pub(super) unsafe fn current_sid() -> Result<String, String> {
    let mut token = null_mut();
    if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let mut length = 0;
    GetTokenInformation(token, TokenUser, null_mut(), 0, &mut length);
    let mut buffer = vec![0usize; (length as usize).div_ceil(size_of::<usize>())];
    if GetTokenInformation(
        token,
        TokenUser,
        buffer.as_mut_ptr().cast(),
        length,
        &mut length,
    ) == 0
    {
        CloseHandle(token);
        return Err(std::io::Error::last_os_error().to_string());
    }
    let user = &*(buffer.as_ptr() as *const TOKEN_USER);
    let mut string = null_mut();
    let success = ConvertSidToStringSidW(user.User.Sid, &mut string) != 0;
    CloseHandle(token);
    if !success {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let mut size = 0;
    while *string.add(size) != 0 {
        size += 1;
    }
    let value = String::from_utf16_lossy(std::slice::from_raw_parts(string, size));
    LocalFree(string.cast());
    Ok(value)
}

pub(super) unsafe fn ensure_realtime() -> Result<(), String> {
    if GetPriorityClass(GetCurrentProcess()) == REALTIME_PRIORITY_CLASS {
        return Ok(());
    }
    let mut token = null_mut();
    if OpenProcessToken(
        GetCurrentProcess(),
        TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
        &mut token,
    ) != 0
    {
        let mut privileges: TOKEN_PRIVILEGES = zeroed();
        privileges.PrivilegeCount = 1;
        if LookupPrivilegeValueW(
            null(),
            SE_INC_BASE_PRIORITY_NAME,
            &mut privileges.Privileges[0].Luid,
        ) != 0
        {
            privileges.Privileges[0].Attributes = SE_PRIVILEGE_ENABLED;
            AdjustTokenPrivileges(token, 0, &privileges, 0, null_mut(), null_mut());
        }
        CloseHandle(token);
    }
    if SetPriorityClass(GetCurrentProcess(), REALTIME_PRIORITY_CLASS) == 0
        || GetPriorityClass(GetCurrentProcess()) != REALTIME_PRIORITY_CLASS
    {
        Err(format!(
            "Could not set Realtime priority: {}",
            std::io::Error::last_os_error()
        ))
    } else {
        Ok(())
    }
}

fn quote_argument(value: &str) -> String {
    let mut output = String::from("\"");
    let mut slashes = 0;
    for ch in value.chars() {
        if ch == '\\' {
            slashes += 1;
            continue;
        }
        if ch == '"' {
            output.extend(std::iter::repeat_n('\\', slashes * 2 + 1));
            output.push(ch);
        } else {
            output.extend(std::iter::repeat_n('\\', slashes));
            output.push(ch);
        }
        slashes = 0;
    }
    output.extend(std::iter::repeat_n('\\', slashes * 2));
    output.push('"');
    output
}

pub(super) unsafe fn relaunch_elevated(args: &[String]) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let parameters = args
        .iter()
        .skip(1)
        .map(|arg| quote_argument(arg))
        .collect::<Vec<_>>()
        .join(" ");
    let result = ShellExecuteW(
        null_mut(),
        wide_raw("runas").as_ptr(),
        wide_raw(&exe.to_string_lossy()).as_ptr(),
        wide_raw(&parameters).as_ptr(),
        null(),
        SW_SHOWNORMAL,
    );
    if result as usize <= 32 {
        Err("Administrator permission is required to control elevated windows.".into())
    } else {
        Ok(())
    }
}
