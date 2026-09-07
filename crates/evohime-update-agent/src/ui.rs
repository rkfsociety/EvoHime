use evohime_update_agent::UpdateCandidate;
use std::{path::Path, ptr::null_mut};
use windows_sys::Win32::{
    System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, KillTimer,
        LoadCursorW, PostQuitMessage, RegisterClassW, SetTimer, ShowWindow, TranslateMessage,
        CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, IDC_ARROW, MSG, SW_SHOW, WM_CLOSE, WM_COMMAND,
        WM_CREATE, WM_DESTROY, WM_TIMER, WNDCLASSW, WS_CAPTION, WS_CHILD, WS_EX_DLGMODALFRAME,
        WS_OVERLAPPED, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
    },
};

const TIMER_ID: usize = 1;
const RUN_BUTTON: usize = 10;
const UPDATE_BUTTON: usize = 11;
const CLASS_NAME: &[u16] = &[
    69, 118, 111, 72, 105, 109, 101, 85, 112, 100, 97, 116, 101, 114, 0,
];

thread_local! { static PREFLIGHT_BODY: std::cell::RefCell<Vec<u16>> = const { std::cell::RefCell::new(Vec::new()) }; }
thread_local! { static PREFLIGHT_HAS_UPDATES: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
thread_local! { static PREFLIGHT_ACTION: std::cell::Cell<UiAction> = const { std::cell::Cell::new(UiAction::Launch) }; }

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum UiAction {
    Launch,
    Update,
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn run_preflight_window(
    _install_dir: &Path,
    updates: &[UpdateCandidate],
    remote_error: Option<&str>,
) -> Result<UiAction, String> {
    let text = if updates.is_empty() {
        match remote_error {
            Some(error) => format!(
                "Manifest, размеры и SHA-256 подтверждены.\nНе удалось проверить releases: {error}"
            ),
            None => "Manifest, размеры и SHA-256 подтверждены.\nВсе модули актуальны.".to_owned(),
        }
    } else {
        format!(
            "Найдены обновления:\n{}",
            updates
                .iter()
                .map(|item| format!("{}  {} → {}", item.module, item.installed, item.available))
                .collect::<Vec<_>>()
                .join("\n")
        )
    };
    PREFLIGHT_BODY.with(|body| *body.borrow_mut() = wide(&text));
    PREFLIGHT_HAS_UPDATES.with(|value| value.set(!updates.is_empty()));
    PREFLIGHT_ACTION.with(|value| value.set(UiAction::Launch));
    let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
    if instance.is_null() {
        return Err("updater: не удалось получить дескриптор окна".into());
    }
    let class = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(window_proc),
        hInstance: instance,
        hCursor: unsafe { LoadCursorW(null_mut(), IDC_ARROW) },
        lpszClassName: CLASS_NAME.as_ptr(),
        ..unsafe { std::mem::zeroed() }
    };
    if unsafe { RegisterClassW(&class) } == 0 {
        return Err("updater: не удалось зарегистрировать окно".into());
    }
    let title = wide("EvoHime Updater");
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_DLGMODALFRAME,
            CLASS_NAME.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            440,
            260,
            null_mut(),
            null_mut(),
            instance,
            null_mut(),
        )
    };
    if hwnd.is_null() {
        return Err("updater: не удалось создать окно".into());
    }
    unsafe {
        if !PREFLIGHT_HAS_UPDATES.with(std::cell::Cell::get) {
            SetTimer(hwnd, TIMER_ID, 900, None);
        }
        ShowWindow(hwnd, SW_SHOW);
    }
    let mut message: MSG = unsafe { std::mem::zeroed() };
    loop {
        let result = unsafe { GetMessageW(&mut message, null_mut(), 0, 0) };
        if result <= 0 {
            break;
        }
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    Ok(PREFLIGHT_ACTION.with(std::cell::Cell::get))
}

unsafe extern "system" fn window_proc(
    hwnd: windows_sys::Win32::Foundation::HWND,
    message: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    match message {
        WM_CREATE => {
            let static_class = wide("STATIC");
            let title = wide("Проверка модулей завершена");
            let body = PREFLIGHT_BODY.with(|value| value.borrow().clone());
            let button_class = wide("BUTTON");
            let run = wide("Запустить текущую версию");
            CreateWindowExW(
                0,
                static_class.as_ptr(),
                title.as_ptr(),
                WS_CHILD | WS_VISIBLE,
                28,
                26,
                370,
                32,
                hwnd,
                null_mut(),
                null_mut(),
                null_mut(),
            );
            CreateWindowExW(
                0,
                static_class.as_ptr(),
                body.as_ptr(),
                WS_CHILD | WS_VISIBLE,
                28,
                70,
                370,
                60,
                hwnd,
                null_mut(),
                null_mut(),
                null_mut(),
            );
            CreateWindowExW(
                0,
                button_class.as_ptr(),
                run.as_ptr(),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP,
                28,
                160,
                220,
                32,
                hwnd,
                RUN_BUTTON as *mut _,
                null_mut(),
                null_mut(),
            );
            if PREFLIGHT_HAS_UPDATES.with(std::cell::Cell::get) {
                let update = wide("Обновить модули");
                CreateWindowExW(
                    0,
                    button_class.as_ptr(),
                    update.as_ptr(),
                    WS_CHILD | WS_VISIBLE | WS_TABSTOP,
                    260,
                    160,
                    140,
                    32,
                    hwnd,
                    UPDATE_BUTTON as *mut _,
                    null_mut(),
                    null_mut(),
                );
            }
            0
        }
        WM_TIMER => {
            KillTimer(hwnd, TIMER_ID);
            request_launch(hwnd);
            0
        }
        WM_COMMAND if (wparam & 0xffff) == RUN_BUTTON => {
            KillTimer(hwnd, TIMER_ID);
            request_launch(hwnd);
            0
        }
        WM_COMMAND if (wparam & 0xffff) == UPDATE_BUTTON => {
            KillTimer(hwnd, TIMER_ID);
            PREFLIGHT_ACTION.with(|value| value.set(UiAction::Update));
            DestroyWindow(hwnd);
            0
        }
        WM_CLOSE => {
            KillTimer(hwnd, TIMER_ID);
            request_launch(hwnd);
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe fn request_launch(hwnd: windows_sys::Win32::Foundation::HWND) {
    DestroyWindow(hwnd);
}
