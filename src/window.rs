use windows::{
    Win32::{
        Foundation::{GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
        Graphics::Gdi::{BeginPaint, EndPaint, HBRUSH, PAINTSTRUCT},
        UI::WindowsAndMessaging::{
            self, CS_HREDRAW, CS_OWNDC, CS_VREDRAW, CreateWindowExW, DefWindowProcW, HICON,
            HWND_TOPMOST, IDC_ARROW, LoadCursorW, PostQuitMessage, RegisterClassExW, SW_SHOW,
            SWP_NOMOVE, SWP_NOSIZE, SetWindowPos, ShowWindow, WM_DESTROY, WM_PAINT, WNDCLASSEXW,
            WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
        },
    },
    core::{HRESULT, PCWSTR, w},
};

use crate::{WINDOW_HEIGHT, WINDOW_WIDTH};

pub fn create_overlay_window(hinstance: HINSTANCE) -> Result<HWND, windows::core::Error> {
    let class_name = w!("overlay_window_class");
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW | CS_OWNDC,
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinstance,
        hIcon: HICON::default(),
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW).expect("Failed to load cursor") },
        hbrBackground: HBRUSH::default(),
        lpszMenuName: PCWSTR::null(),
        lpszClassName: class_name,
        hIconSm: HICON::default(),
        lpfnWndProc: Some(wndproc),
    };

    let atom = unsafe { RegisterClassExW(&wc) };
    if atom == 0 {
        let error = unsafe { GetLastError() };
        return Err(windows::core::Error::new(
            HRESULT::from(error),
            "Failed to register window class.",
        ));
    }

    let screen_width =
        unsafe { WindowsAndMessaging::GetSystemMetrics(WindowsAndMessaging::SM_CXSCREEN) };
    let screen_height =
        unsafe { WindowsAndMessaging::GetSystemMetrics(WindowsAndMessaging::SM_CYSCREEN) };

    // Centered X
    let x = (screen_width - WINDOW_WIDTH) / 2;

    //  80% down the screen
    let y = (screen_height * 4) / 5 - (WINDOW_HEIGHT / 2);

    // Ensure position is non-negative (important for CreateWindowExW)
    let x = if x < 0 { 0 } else { x };
    let y = if y < 0 { 0 } else { y };

    let hwnd: HWND = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class_name,
            w!("DS Battery overlay"),
            WS_POPUP,
            x,
            y,
            WINDOW_WIDTH,
            WINDOW_HEIGHT,
            None,
            None,
            Some(hinstance),
            None,
        )
        .expect("Failed to create window")
    };

    if hwnd.is_invalid() {
        let error = unsafe { GetLastError() };
        return Err(windows::core::Error::new(
            HRESULT::from(error),
            "Failed to create window",
        ));
    }

    println!("HWND created: {:?}", hwnd);

    Ok(hwnd)
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            // Basic paint handling: just validate the region
            // More complex drawing would happen here
            println!("WM_PAINT received");
            let mut ps = PAINTSTRUCT::default();
            let _hdc = unsafe { BeginPaint(hwnd, &mut ps) };
            // FillRect(hdc, &ps.rcPaint, HBRUSH((COLOR_WINDOW.0 + 1) as isize)); // Example drawing
            unsafe {
                let _ = EndPaint(hwnd, &ps);
            };
            LRESULT(0) // Indicate message was handled
        }
        WM_DESTROY => {
            // This message is sent when the window is being destroyed (e.g., user clicks close button)
            println!("WM_DESTROY received");
            // Post a WM_QUIT message to the message queue to signal the message loop to exit
            unsafe { PostQuitMessage(0) };
            LRESULT(0) // Indicate message was handled
        }
        _ => {
            // For messages we don't handle explicitly, pass them to the default window procedure
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
    }
}

pub fn show_and_set_topmost(hwnd: &HWND) {
    unsafe {
        let _ = ShowWindow(*hwnd, SW_SHOW);

        let _ = SetWindowPos(
            *hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE,
        );
    }
}
