//! Modeless notices keep head control usable while help/errors are displayed.
use super::*;

const CLASS: &str = "robocam.notice";

thread_local! {
    static PAUSE_WINDOW: std::cell::Cell<HWND> = const { std::cell::Cell::new(null_mut()) };
    static PAUSE_ANSWER: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

pub(super) fn pause_window() -> HWND {
    PAUSE_WINDOW.with(|window| window.get())
}

pub(super) fn take_pause_answer() -> Option<bool> {
    PAUSE_ANSWER.with(|answer| answer.take())
}

pub(super) unsafe fn cancel_pause_confirmation() {
    let window = pause_window();
    if !window.is_null() {
        DestroyWindow(window);
    }
    take_pause_answer();
}

pub(super) unsafe fn confirm_pause(owner: HWND, target: bool) {
    let existing = pause_window();
    if !existing.is_null() {
        SetForegroundWindow(existing);
        return;
    }
    take_pause_answer();
    show_notice(
        owner,
        if target {
            "Pause webcam movement and clicks?\n\nHead control stays active until you confirm. To resume afterwards, use the full-pause hotkey or a physical mouse."
        } else {
            "Resume webcam movement and clicks?\n\nThe previous click-pause setting will be retained."
        },
        Some(target),
        true,
        "robocam",
    );
}

struct Notice {
    font: HFONT,
    pause_target: Option<bool>,
}

pub(super) unsafe fn show(owner: HWND, content: &str) {
    show_notice(owner, content, None, true, "robocam");
}

pub(super) unsafe fn show_verbatim(owner: HWND, title: &str, content: &str) {
    show_notice(owner, content, None, false, title);
}

unsafe fn show_notice(
    owner: HWND,
    content: &str,
    pause_target: Option<bool>,
    translate: bool,
    title: &str,
) {
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
    let mut owner_bounds: RECT = zeroed();
    GetWindowRect(owner, &mut owner_bounds);
    let window = CreateWindowExW(
        WS_EX_CONTROLPARENT,
        class_name.as_ptr(),
        wide(title).as_ptr(),
        WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU,
        owner_bounds.left + (owner_bounds.right - owner_bounds.left - scale(570, dpi)) / 2,
        owner_bounds.top + (owner_bounds.bottom - owner_bounds.top - scale(height, dpi)) / 2,
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
    // Native multiline EDIT controls require CRLF, including translated text.
    let content = if translate {
        crate::i18n::translate(content)
    } else {
        std::borrow::Cow::Borrowed(content)
    }
    .replace("\r\n", "\n")
    .replace('\n', "\r\n");
    let field = CreateWindowExW(
        0,
        wide_raw("EDIT").as_ptr(),
        wide_raw(&content).as_ptr(),
        WS_CHILD
            | WS_VISIBLE
            | WS_VSCROLL
            | ES_MULTILINE as u32
            | ES_READONLY as u32
            | if translate { 0 } else { WS_TABSTOP },
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
        wide(if pause_target.is_some() {
            "Confirm"
        } else {
            "OK"
        })
        .as_ptr(),
        WS_CHILD
            | WS_VISIBLE
            | WS_TABSTOP
            | if pause_target.is_some() {
                0
            } else {
                BS_DEFPUSHBUTTON as u32
            },
        scale(438, dpi),
        scale(height - 76, dpi),
        scale(100, dpi),
        scale(28, dpi),
        window,
        1usize as HMENU,
        instance,
        null(),
    );
    let cancel = pause_target.map(|_| {
        CreateWindowExW(
            0,
            wide_raw("BUTTON").as_ptr(),
            wide("Cancel").as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_DEFPUSHBUTTON as u32,
            scale(320, dpi),
            scale(height - 76, dpi),
            scale(100, dpi),
            scale(28, dpi),
            window,
            2usize as HMENU,
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
    let notice = Box::new(Notice { font, pause_target });
    SetWindowLongPtrW(window, GWLP_USERDATA, Box::into_raw(notice) as isize);
    SendMessageW(field, WM_SETFONT, font as usize, 1);
    SendMessageW(button, WM_SETFONT, font as usize, 1);
    if let Some(cancel) = cancel {
        SendMessageW(cancel, WM_SETFONT, font as usize, 1);
        PAUSE_WINDOW.with(|handle| handle.set(window));
    }
    placement::ensure(window);
    ShowWindow(window, SW_SHOW);
    SetForegroundWindow(window);
    SetFocus(cancel.unwrap_or(button));
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
        WM_COMMAND if matches!(wp & 0xffff, 1 | 2) && (wp >> 16) == 0 => {
            if wp == 1 {
                let notice = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Notice;
                if let Some(target) = notice.as_ref().and_then(|notice| notice.pause_target) {
                    PAUSE_ANSWER.with(|answer| answer.set(Some(target)));
                }
            }
            DestroyWindow(hwnd);
            0
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_NCDESTROY => {
            PAUSE_WINDOW.with(|window| {
                if window.get() == hwnd {
                    window.set(null_mut());
                }
            });
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
