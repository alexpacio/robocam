//! Modeless notices keep head control usable while help/errors are displayed.
use super::*;

const CLASS: &str = "robocam.notice";

struct Notice {
    font: HFONT,
    release_url: Option<String>,
}

pub(super) unsafe fn show(owner: HWND, content: &str) {
    show_notice(owner, content, None);
}

pub(super) unsafe fn show_releases(owner: HWND, content: &str, url: &str) {
    show_notice(
        owner,
        &format!("{content}\n\nGitHub Releases: alexpacio/robocam"),
        Some(url),
    );
}

unsafe fn show_notice(owner: HWND, content: &str, release_url: Option<&str>) {
    let class_name = wide_raw(CLASS);
    let instance = GetModuleHandleW(null());
    let class = WNDCLASSW {
        lpfnWndProc: Some(notice_proc),
        hInstance: instance,
        lpszClassName: class_name.as_ptr(),
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        hbrBackground: (COLOR_BTNFACE + 1) as HBRUSH,
        ..zeroed()
    };
    RegisterClassW(&class);
    let dpi = GetDpiForWindow(owner).max(96);
    let height = if content.len() > 700 { 510 } else { 290 };
    let window = CreateWindowExW(
        WS_EX_CONTROLPARENT,
        class_name.as_ptr(),
        wide_raw("robocam").as_ptr(),
        WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        scale(570, dpi),
        scale(height, dpi),
        owner,
        null_mut(),
        instance,
        null(),
    );
    if window.is_null() {
        return;
    }
    let field = CreateWindowExW(
        0,
        wide_raw("EDIT").as_ptr(),
        wide(content).as_ptr(),
        WS_CHILD | WS_VISIBLE | WS_VSCROLL | ES_MULTILINE as u32 | ES_READONLY as u32,
        scale(16, dpi),
        scale(14, dpi),
        scale(522, dpi),
        scale(height - 100, dpi),
        window,
        100usize as HMENU,
        instance,
        null(),
    );
    let button = CreateWindowExW(
        0,
        wide_raw("BUTTON").as_ptr(),
        wide_raw("OK").as_ptr(),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_DEFPUSHBUTTON as u32,
        scale(438, dpi),
        scale(height - 76, dpi),
        scale(100, dpi),
        scale(28, dpi),
        window,
        1usize as HMENU,
        instance,
        null(),
    );
    let release_button = release_url.map(|_| {
        CreateWindowExW(
            0,
            wide_raw("BUTTON").as_ptr(),
            wide("Open GitHub Releases").as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP,
            scale(16, dpi),
            scale(height - 76, dpi),
            scale(270, dpi),
            scale(28, dpi),
            window,
            3usize as HMENU,
            instance,
            null(),
        )
    });
    let font = CreateFontW(
        -scale(14, dpi),
        0,
        0,
        0,
        400,
        0,
        0,
        0,
        DEFAULT_CHARSET as u32,
        OUT_DEFAULT_PRECIS as u32,
        CLIP_DEFAULT_PRECIS as u32,
        CLEARTYPE_QUALITY as u32,
        DEFAULT_PITCH as u32,
        wide_raw("Segoe UI").as_ptr(),
    );
    let notice = Box::new(Notice {
        font,
        release_url: release_url.map(str::to_owned),
    });
    SetWindowLongPtrW(window, GWLP_USERDATA, Box::into_raw(notice) as isize);
    SendMessageW(field, WM_SETFONT, font as usize, 1);
    SendMessageW(button, WM_SETFONT, font as usize, 1);
    if let Some(button) = release_button {
        SendMessageW(button, WM_SETFONT, font as usize, 1);
    }
    ShowWindow(window, SW_SHOW);
    SetForegroundWindow(window);
    SetFocus(button);
}

pub(super) unsafe fn route(msg: &MSG) -> bool {
    let root = GetAncestor(msg.hwnd, GA_ROOT);
    let mut name = [0u16; 64];
    let count = GetClassNameW(root, name.as_mut_ptr(), name.len() as i32);
    if String::from_utf16_lossy(&name[..count.max(0) as usize]) == CLASS {
        return IsDialogMessageW(root, msg) != 0;
    }
    false
}

unsafe extern "system" fn notice_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND if wp == 3 => {
            // Copy the URL before ShellExecute, which can pump native messages.
            // The user may close this modeless notice while the browser opens.
            let notice = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Notice;
            let url = notice
                .as_ref()
                .and_then(|notice| notice.release_url.as_deref())
                .map(wide_raw);
            if let Some(url) = url {
                let result = ShellExecuteW(
                    hwnd,
                    wide_raw("open").as_ptr(),
                    url.as_ptr(),
                    null(),
                    null(),
                    SW_SHOWNORMAL,
                );
                if result as usize <= 32 && IsWindow(hwnd) != 0 {
                    set_text(
                        GetDlgItem(hwnd, 100),
                        "Could not open your browser. Visit https://github.com/alexpacio/robocam/releases",
                    );
                }
            }
            0
        }
        WM_COMMAND if matches!(wp & 0xffff, 1 | 2) && (wp >> 16) == 0 => {
            DestroyWindow(hwnd);
            0
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_NCDESTROY => {
            let notice = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Notice;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            if !notice.is_null() {
                let notice = Box::from_raw(notice);
                if !notice.font.is_null() {
                    DeleteObject(notice.font);
                }
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
