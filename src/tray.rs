use windows::{
    Win32::{
        Foundation::{GetLastError, HWND, LPARAM, POINT, WPARAM},
        UI::{
            Shell::{
                NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_SETVERSION,
                NOTIFYICONDATAW, Shell_NotifyIconW,
            },
            WindowsAndMessaging::{
                AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, HICON, HMENU, MF_STRING,
                PostMessageW, SetForegroundWindow, TPM_LEFTALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON,
                TrackPopupMenu, WM_COMMAND,
            },
        },
    },
    core::w,
};

use crate::{IDM_CONFIGURE, IDM_EXIT};

const TRAY_ICON_ID: u32 = 1;
const TRAY_TOOLTIP: &str = "DualSense Battery Overlay";

pub fn add_tray_icon(
    hwnd: HWND,
    h_icon: HICON,
    callback_message_id: u32,
) -> Result<(), windows::core::Error> {
    let mut nid = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ICON_ID,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: callback_message_id,
        hIcon: h_icon,
        szTip: [0; 128],
        ..Default::default()
    };

    let mut wide_chars = TRAY_TOOLTIP.encode_utf16().collect::<Vec<_>>();
    wide_chars.push(0);
    let len_to_copy = std::cmp::min(wide_chars.len() - 1, nid.szTip.len() - 1);
    nid.szTip[..len_to_copy].copy_from_slice(&wide_chars[..len_to_copy]);
    nid.szTip[len_to_copy] = 0; // Null terminate

    if !unsafe { Shell_NotifyIconW(NIM_ADD, &nid).as_bool() } {
        return Err(windows::core::Error::from_win32());
    }

    if !unsafe { Shell_NotifyIconW(NIM_SETVERSION, &nid).as_bool() } {
        let add_error = windows::core::Error::from_win32();

        let _ = remove_tray_icon(hwnd);
        return Err(add_error);
    }
    Ok(())
}

pub fn remove_tray_icon(hwnd: HWND) -> Result<(), ()> {
    let nid = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ICON_ID,
        ..Default::default()
    };

    unsafe {
        if !Shell_NotifyIconW(NIM_DELETE, &nid).as_bool() {
            eprintln!("Shell_NotifyIconW(NIM_DELETE) failed: {:?}", GetLastError());
        }
    }
    Ok(())
}

pub fn show_context_menu(hwnd: HWND) -> Result<(), ()> {
    unsafe {
        let hmenu = CreatePopupMenu().unwrap();

        struct MenuGuard(HMENU);
        impl Drop for MenuGuard {
            fn drop(&mut self) {
                if !self.0.is_invalid() {
                    let _ = unsafe { DestroyMenu(self.0) };
                }
            }
        }
        let _guard = MenuGuard(hmenu);

        AppendMenuW(hmenu, MF_STRING, IDM_CONFIGURE as usize, w!("Configure")).unwrap();
        AppendMenuW(hmenu, MF_STRING, IDM_EXIT as usize, w!("Exit")).unwrap();

        let mut point = POINT::default();
        GetCursorPos(&mut point).unwrap();

        SetForegroundWindow(hwnd).unwrap();

        let selected_cmd = TrackPopupMenu(
            hmenu,
            TPM_LEFTALIGN | TPM_RIGHTBUTTON | TPM_RETURNCMD,
            point.x,
            point.y,
            Some(0),
            hwnd,
            None,
        );

        if !selected_cmd.as_bool() {
            PostMessageW(
                Some(hwnd),
                WM_COMMAND,
                WPARAM(selected_cmd.as_bool() as usize),
                LPARAM(0),
            )
            .unwrap();
        }
    }
    Ok(())
}
