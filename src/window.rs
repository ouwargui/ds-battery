use crate::{AppState, HOTKEY_ID_TOGGLE, window_creator::WindowCreator, window_message_handler};
use windows::{
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
        UI::{
            Input::KeyboardAndMouse::{
                MOD_ALT, MOD_CONTROL, RegisterHotKey, UnregisterHotKey, VK_B,
            },
            WindowsAndMessaging::{
                DefWindowProcW, GWLP_USERDATA, GetWindowLongPtrW, HWND_TOPMOST, SW_SHOW,
                SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos, ShowWindow,
            },
        },
    },
    core::Result,
};

pub fn create_overlay_window(hinstance: HINSTANCE) -> Result<(HWND, WindowCreator)> {
    let window_creator = WindowCreator::new(hinstance);
    let hwnd = window_creator.create_overlay_window()?;
    Ok((hwnd, window_creator))
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
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}

pub fn register_app_hotkey(hwnd: HWND) -> Result<()> {
    let modifiers = MOD_CONTROL | MOD_ALT;
    let vk = VK_B.0 as u32;
    unsafe { RegisterHotKey(Some(hwnd), HOTKEY_ID_TOGGLE, modifiers, vk) }
}

pub fn unregister_app_hotkey(hwnd: HWND) -> Result<()> {
    unsafe { UnregisterHotKey(Some(hwnd), HOTKEY_ID_TOGGLE) }
}

pub unsafe extern "system" fn wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let app_state_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };

    if app_state_ptr != 0 {
        let app_state = unsafe { &mut *(app_state_ptr as *mut AppState) };

        // Delegate to the message handler module
        if let Some(result) =
            window_message_handler::handle_message(hwnd, msg, wparam, lparam, app_state)
        {
            return result;
        }
    }

    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}
