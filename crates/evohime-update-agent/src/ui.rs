use evohime_update_agent::UpdateCandidate;
use std::{
    cell::{Cell, RefCell},
    path::Path,
    ptr::null_mut,
    sync::{mpsc, Arc},
    thread,
};
use windows_sys::Win32::{
    Foundation::HWND,
    Graphics::Gdi::{
        BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, EndPaint, FillRect,
        InvalidateRect, SetBkColor, SetTextColor, PAINTSTRUCT,
    },
    System::LibraryLoader::GetModuleHandleW,
    UI::Input::KeyboardAndMouse::EnableWindow,
    UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, KillTimer,
        LoadCursorW, PostQuitMessage, RegisterClassW, SendMessageW, SetTimer, SetWindowTextW,
        ShowWindow, TranslateMessage, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, IDC_ARROW, MSG,
        SW_SHOW, WM_CLOSE, WM_COMMAND, WM_CREATE, WM_CTLCOLORBTN, WM_CTLCOLORSTATIC, WM_DESTROY,
        WM_PAINT, WM_SETFONT, WM_TIMER, WNDCLASSW, WS_CAPTION, WS_CHILD, WS_EX_CLIENTEDGE,
        WS_EX_DLGMODALFRAME, WS_OVERLAPPED, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
    },
};

const TIMER_ID: usize = 1;
const RUN_BUTTON: usize = 10;
const UPDATE_BUTTON: usize = 11;
const CLASS_NAME: &[u16] = &[
    85, 112, 100, 97, 116, 101, 114, 80, 114, 101, 102, 108, 105, 103, 104, 116, 0,
];

thread_local! { static PREFLIGHT_BODY: RefCell<Vec<u16>> = const { RefCell::new(Vec::new()) }; }
thread_local! { static PREFLIGHT_STATUS: RefCell<Vec<u16>> = const { RefCell::new(Vec::new()) }; }
thread_local! { static PREFLIGHT_HAS_UPDATES: Cell<bool> = const { Cell::new(false) }; }
thread_local! { static PREFLIGHT_RUNNING: Cell<bool> = const { Cell::new(false) }; }
thread_local! { static PREFLIGHT_ACTION: Cell<UiAction> = const { Cell::new(UiAction::Launch) }; }
thread_local! { static PREFLIGHT_OPERATION: RefCell<Option<Arc<ApplyOperation>>> = const { RefCell::new(None) }; }
thread_local! { static PREFLIGHT_EVENTS: RefCell<Option<mpsc::Receiver<UiEvent>>> = const { RefCell::new(None) }; }
thread_local! { static PREFLIGHT_STATUS_HWND: Cell<HWND> = const { Cell::new(null_mut()) }; }
thread_local! { static PREFLIGHT_UPDATE_HWND: Cell<HWND> = const { Cell::new(null_mut()) }; }
thread_local! { static PREFLIGHT_RUN_HWND: Cell<HWND> = const { Cell::new(null_mut()) }; }
thread_local! { static PREFLIGHT_FONT: Cell<isize> = const { Cell::new(0) }; }
thread_local! { static PREFLIGHT_BODY_HWND: Cell<HWND> = const { Cell::new(null_mut()) }; }
thread_local! { static PREFLIGHT_CARD_BRUSH: Cell<isize> = const { Cell::new(0) }; }
thread_local! { static PREFLIGHT_STATUS_BRUSH: Cell<isize> = const { Cell::new(0) }; }
thread_local! { static PREFLIGHT_PROGRESS: Cell<u8> = const { Cell::new(100) }; }
thread_local! { static PREFLIGHT_HEADING: RefCell<Vec<u16>> = const { RefCell::new(Vec::new()) }; }

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum UiAction {
    Launch,
}

pub enum UiEvent {
    Progress { message: String, percent: u8 },
    Finished,
    Failed(String),
}

pub type ApplyOperation = dyn Fn(mpsc::Sender<UiEvent>) -> Result<(), String> + Send + Sync;

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn run_preflight_window(
    _install_dir: &Path,
    updates: &[UpdateCandidate],
    remote_error: Option<&str>,
    operation: Arc<ApplyOperation>,
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
                .map(|item| {
                    let summary = if item.summary.is_empty() {
                        "Описание доступно в манифесте релиза.".to_owned()
                    } else {
                        item.summary.clone()
                    };
                    format!(
                        "{}  {} → {}\n  {}",
                        item.module, item.installed, item.available, summary
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        )
    };
    PREFLIGHT_BODY.with(|body| *body.borrow_mut() = wide(&text));
    PREFLIGHT_STATUS.with(|status| {
        *status.borrow_mut() = wide(if remote_error.is_some() {
            "Проверка завершена с ошибкой."
        } else if updates.is_empty() {
            "Проверка завершена."
        } else {
            "Обновление ещё не запущено."
        })
    });
    PREFLIGHT_HEADING.with(|heading| {
        *heading.borrow_mut() = wide(if remote_error.is_some() {
            "Проверка требует внимания"
        } else if updates.is_empty() {
            "Модули проверены"
        } else {
            "Доступно обновление"
        });
    });
    PREFLIGHT_PROGRESS.with(|progress| progress.set(100));
    PREFLIGHT_HAS_UPDATES.with(|value| value.set(!updates.is_empty()));
    PREFLIGHT_RUNNING.with(|value| value.set(false));
    PREFLIGHT_ACTION.with(|value| value.set(UiAction::Launch));
    PREFLIGHT_OPERATION.with(|value| *value.borrow_mut() = Some(operation));
    PREFLIGHT_EVENTS.with(|value| *value.borrow_mut() = None);

    let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
    if instance.is_null() {
        return Err("updater: не удалось получить дескриптор окна".into());
    }
    let window_brush = unsafe { CreateSolidBrush(0x002A1811) };
    let class = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(window_proc),
        hInstance: instance,
        hCursor: unsafe { LoadCursorW(null_mut(), IDC_ARROW) },
        hbrBackground: window_brush,
        lpszClassName: CLASS_NAME.as_ptr(),
        ..unsafe { std::mem::zeroed() }
    };
    let _ = unsafe { RegisterClassW(&class) };
    let title = wide("EvoHime — обновление");
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_DLGMODALFRAME,
            CLASS_NAME.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            620,
            455,
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
        if !PREFLIGHT_HAS_UPDATES.with(Cell::get) && remote_error.is_none() {
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
    PREFLIGHT_OPERATION.with(|value| *value.borrow_mut() = None);
    PREFLIGHT_EVENTS.with(|value| *value.borrow_mut() = None);
    Ok(PREFLIGHT_ACTION.with(Cell::get))
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    match message {
        WM_CREATE => {
            let font = CreateFontW(
                -18,
                0,
                0,
                0,
                400,
                0,
                0,
                0,
                1,
                0,
                0,
                5,
                0,
                wide("Segoe UI").as_ptr(),
            );
            PREFLIGHT_FONT.with(|value| value.set(font as isize));
            let static_class = wide("STATIC");
            let rail_kicker = wide("НАДЁЖНЫЙ ЗАПУСК");
            let rail_heading = wide("Всё готово\nдля работы.");
            let rail_body = wide("Проверяем модули и целостность локальной установки.");
            let rail_icon = wide("✓");
            let title = PREFLIGHT_HEADING.with(|heading| heading.borrow().clone());
            let body = PREFLIGHT_BODY.with(|value| value.borrow().clone());
            let status = PREFLIGHT_STATUS.with(|value| value.borrow().clone());
            let button_class = wide("BUTTON");
            let run = wide("Запустить текущую версию");
            let update = wide("Обновить");
            let rail_kicker_hwnd = CreateWindowExW(
                0,
                static_class.as_ptr(),
                rail_kicker.as_ptr(),
                WS_CHILD | WS_VISIBLE,
                32,
                38,
                190,
                24,
                hwnd,
                null_mut(),
                null_mut(),
                null_mut(),
            );
            let rail_heading_hwnd = CreateWindowExW(
                0,
                static_class.as_ptr(),
                rail_heading.as_ptr(),
                WS_CHILD | WS_VISIBLE,
                32,
                70,
                190,
                64,
                hwnd,
                null_mut(),
                null_mut(),
                null_mut(),
            );
            let rail_body_hwnd = CreateWindowExW(
                0,
                static_class.as_ptr(),
                rail_body.as_ptr(),
                WS_CHILD | WS_VISIBLE,
                32,
                150,
                180,
                72,
                hwnd,
                null_mut(),
                null_mut(),
                null_mut(),
            );
            let rail_icon_hwnd = CreateWindowExW(
                0,
                static_class.as_ptr(),
                rail_icon.as_ptr(),
                WS_CHILD | WS_VISIBLE,
                100,
                270,
                55,
                55,
                hwnd,
                null_mut(),
                null_mut(),
                null_mut(),
            );
            let title_hwnd = CreateWindowExW(
                0,
                static_class.as_ptr(),
                title.as_ptr(),
                WS_CHILD | WS_VISIBLE,
                240,
                38,
                330,
                30,
                hwnd,
                null_mut(),
                null_mut(),
                null_mut(),
            );
            let body_hwnd = CreateWindowExW(
                WS_EX_CLIENTEDGE,
                static_class.as_ptr(),
                body.as_ptr(),
                WS_CHILD | WS_VISIBLE,
                240,
                110,
                330,
                175,
                hwnd,
                null_mut(),
                null_mut(),
                null_mut(),
            );
            PREFLIGHT_BODY_HWND.with(|value| value.set(body_hwnd));
            PREFLIGHT_CARD_BRUSH.with(|value| {
                value.set(CreateSolidBrush(0x00482D20) as isize);
            });
            PREFLIGHT_STATUS_BRUSH.with(|value| {
                value.set(CreateSolidBrush(0x00482D20) as isize);
            });
            let status_hwnd = CreateWindowExW(
                WS_EX_CLIENTEDGE,
                static_class.as_ptr(),
                status.as_ptr(),
                WS_CHILD | WS_VISIBLE,
                240,
                300,
                330,
                34,
                hwnd,
                null_mut(),
                null_mut(),
                null_mut(),
            );
            PREFLIGHT_STATUS_HWND.with(|value| value.set(status_hwnd));
            let font = PREFLIGHT_FONT.with(Cell::get) as usize;
            for child in [
                rail_kicker_hwnd,
                rail_heading_hwnd,
                rail_body_hwnd,
                rail_icon_hwnd,
                title_hwnd,
                body_hwnd,
                status_hwnd,
            ] {
                SendMessageW(child, WM_SETFONT, font, 1);
            }
            let run_hwnd = CreateWindowExW(
                0,
                button_class.as_ptr(),
                run.as_ptr(),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | 1u32,
                240,
                370,
                155,
                38,
                hwnd,
                RUN_BUTTON as *mut _,
                null_mut(),
                null_mut(),
            );
            PREFLIGHT_RUN_HWND.with(|value| value.set(run_hwnd));
            SendMessageW(run_hwnd, WM_SETFONT, font, 1);
            if PREFLIGHT_HAS_UPDATES.with(Cell::get) {
                let update_hwnd = CreateWindowExW(
                    0,
                    button_class.as_ptr(),
                    update.as_ptr(),
                    WS_CHILD | WS_VISIBLE | WS_TABSTOP | 1u32,
                    405,
                    370,
                    165,
                    38,
                    hwnd,
                    UPDATE_BUTTON as *mut _,
                    null_mut(),
                    null_mut(),
                );
                PREFLIGHT_UPDATE_HWND.with(|value| value.set(update_hwnd));
                SendMessageW(update_hwnd, WM_SETFONT, font, 1);
            }
            0
        }
        WM_TIMER if wparam == TIMER_ID => {
            if PREFLIGHT_RUNNING.with(Cell::get) {
                poll_events(hwnd);
            } else {
                KillTimer(hwnd, TIMER_ID);
                request_launch(hwnd);
            }
            0
        }
        WM_CTLCOLORSTATIC => {
            let dc = wparam as *mut _;
            let control = lparam as HWND;
            let body = PREFLIGHT_BODY_HWND.with(Cell::get);
            let status = PREFLIGHT_STATUS_HWND.with(Cell::get);
            let (brush, color, background) = if control == body {
                (PREFLIGHT_CARD_BRUSH.with(Cell::get), 0x00FCFAF8, 0x00482D20)
            } else if control == status {
                (
                    PREFLIGHT_STATUS_BRUSH.with(Cell::get),
                    0x007DE37D,
                    0x00482D20,
                )
            } else {
                (
                    PREFLIGHT_STATUS_BRUSH.with(Cell::get),
                    0x00FCFAF8,
                    0x002A1811,
                )
            };
            SetTextColor(dc, color);
            SetBkColor(dc, background);
            brush
        }
        WM_CTLCOLORBTN => {
            let dc = wparam as *mut _;
            SetTextColor(dc, 0x00FCFAF8);
            SetBkColor(dc, 0x00482D20);
            PREFLIGHT_STATUS_BRUSH.with(Cell::get)
        }
        WM_PAINT => {
            let mut paint: PAINTSTRUCT = std::mem::zeroed();
            let dc = BeginPaint(hwnd, &mut paint);
            let rail = CreateSolidBrush(0x00382218);
            let rail_rect = windows_sys::Win32::Foundation::RECT {
                left: 0,
                top: 0,
                right: 220,
                bottom: 430,
            };
            FillRect(dc, &rail_rect, rail);
            let track = CreateSolidBrush(0x006B4B35);
            let fill = CreateSolidBrush(0x00E5E37D);
            let track_rect = windows_sys::Win32::Foundation::RECT {
                left: 240,
                top: 350,
                right: 570,
                bottom: 357,
            };
            FillRect(dc, &track_rect, track);
            let progress = PREFLIGHT_PROGRESS.with(Cell::get) as i32;
            let progress_rect = windows_sys::Win32::Foundation::RECT {
                left: 240,
                top: 350,
                right: 240 + (330 * progress / 100),
                bottom: 357,
            };
            FillRect(dc, &progress_rect, fill);
            DeleteObject(rail as _);
            DeleteObject(track as _);
            DeleteObject(fill as _);
            EndPaint(hwnd, &paint);
            0
        }
        WM_COMMAND if (wparam & 0xffff) == RUN_BUTTON => {
            if !PREFLIGHT_RUNNING.with(Cell::get) {
                KillTimer(hwnd, TIMER_ID);
                request_launch(hwnd);
            }
            0
        }
        WM_COMMAND if (wparam & 0xffff) == UPDATE_BUTTON => {
            if !PREFLIGHT_RUNNING.with(Cell::get) {
                start_update(hwnd);
            }
            0
        }
        WM_CLOSE => {
            if !PREFLIGHT_RUNNING.with(Cell::get) {
                KillTimer(hwnd, TIMER_ID);
                request_launch(hwnd);
            }
            0
        }
        WM_DESTROY => {
            DeleteObject(PREFLIGHT_FONT.with(Cell::get) as _);
            DeleteObject(PREFLIGHT_CARD_BRUSH.with(Cell::get) as _);
            DeleteObject(PREFLIGHT_STATUS_BRUSH.with(Cell::get) as _);
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe fn start_update(hwnd: HWND) {
    let (sender, receiver) = mpsc::channel();
    let Some(operation) = PREFLIGHT_OPERATION.with(|value| value.borrow().clone()) else {
        set_status("Операция обновления недоступна.");
        return;
    };
    PREFLIGHT_EVENTS.with(|value| *value.borrow_mut() = Some(receiver));
    PREFLIGHT_RUNNING.with(|value| value.set(true));
    PREFLIGHT_PROGRESS.with(|progress| progress.set(0));
    InvalidateRect(hwnd, null_mut(), 0);
    let update_text = wide("Обновление…");
    let update_hwnd = PREFLIGHT_UPDATE_HWND.with(Cell::get);
    if !update_hwnd.is_null() {
        SetWindowTextW(update_hwnd, update_text.as_ptr());
        EnableWindow(update_hwnd, 0);
    }
    let run_hwnd = PREFLIGHT_RUN_HWND.with(Cell::get);
    if !run_hwnd.is_null() {
        EnableWindow(run_hwnd, 0);
    }
    SetTimer(hwnd, TIMER_ID, 200, None);
    thread::spawn(move || {
        let result = operation(sender.clone());
        let event = match result {
            Ok(()) => UiEvent::Finished,
            Err(error) => UiEvent::Failed(error),
        };
        let _ = sender.send(event);
    });
}

unsafe fn poll_events(hwnd: HWND) {
    let mut finished = false;
    let mut failed = None;
    PREFLIGHT_EVENTS.with(|events| {
        if let Some(receiver) = events.borrow().as_ref() {
            while let Ok(event) = receiver.try_recv() {
                match event {
                    UiEvent::Progress { message, percent } => {
                        PREFLIGHT_PROGRESS.with(|progress| progress.set(percent));
                        set_status(&format!("{message} — {percent}%"));
                        InvalidateRect(hwnd, null_mut(), 0);
                    }
                    UiEvent::Finished => finished = true,
                    UiEvent::Failed(error) => failed = Some(error),
                }
            }
        }
    });
    if finished {
        PREFLIGHT_RUNNING.with(|value| value.set(false));
        KillTimer(hwnd, TIMER_ID);
        set_status("Обновление завершено. Запускаю приложение…");
        DestroyWindow(hwnd);
    } else if let Some(error) = failed {
        PREFLIGHT_RUNNING.with(|value| value.set(false));
        KillTimer(hwnd, TIMER_ID);
        set_status(&format!("Ошибка обновления: {error}"));
        let retry = wide("Повторить");
        let update_hwnd = PREFLIGHT_UPDATE_HWND.with(Cell::get);
        if !update_hwnd.is_null() {
            SetWindowTextW(update_hwnd, retry.as_ptr());
            EnableWindow(update_hwnd, 1);
        }
        let run_hwnd = PREFLIGHT_RUN_HWND.with(Cell::get);
        if !run_hwnd.is_null() {
            EnableWindow(run_hwnd, 1);
        }
    }
}

fn set_status(value: &str) {
    let text = wide(value);
    PREFLIGHT_STATUS_HWND.with(|value_hwnd| {
        let hwnd = value_hwnd.get();
        if !hwnd.is_null() {
            unsafe { SetWindowTextW(hwnd, text.as_ptr()) };
        }
    });
}

unsafe fn request_launch(hwnd: HWND) {
    DestroyWindow(hwnd);
}
