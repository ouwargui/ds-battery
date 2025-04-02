use std::os::windows::ffi::OsStrExt;
use std::{ffi::OsStr, slice};

use windows::Win32::System::Registry::RegDeleteValueW;
use windows::Win32::UI::WindowsAndMessaging::{MF_CHECKED, MF_UNCHECKED};
use windows::{
    Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, GetLastError, HWND, POINT},
        System::Registry::{
            HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ,
            RegCloseKey, RegCreateKeyExW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
        },
        UI::{
            Shell::{
                NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_SETVERSION,
                NOTIFYICONDATAW, Shell_NotifyIconW,
            },
            WindowsAndMessaging::{
                AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, HICON, HMENU, MF_STRING,
                SetForegroundWindow, TPM_LEFTALIGN, TPM_RIGHTBUTTON, TrackPopupMenu,
            },
        },
    },
    core::{HSTRING, PCWSTR, w},
};

use crate::{APP_REGISTRY_KEY_NAME, IDM_CONFIGURE, IDM_EXIT, IDM_RUN_ON_STARTUP};

const TRAY_ICON_ID: u32 = 1;
const TRAY_TOOLTIP: &str = "DualSense Battery Overlay";

const RUN_KEY_PATH: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";

const REG_QUERY_SUCCESS: u32 = ERROR_SUCCESS.0 as u32;
const REG_QUERY_ERROR: u32 = ERROR_FILE_NOT_FOUND.0 as u32;

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

pub fn show_context_menu(hwnd: HWND) -> Result<(), windows::core::Error> {
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

        let is_startup_enabled = is_run_on_startup_enabled().unwrap_or(false);
        let startup_flags = if is_startup_enabled {
            MF_STRING | MF_CHECKED
        } else {
            MF_STRING | MF_UNCHECKED
        };

        AppendMenuW(hmenu, MF_STRING, IDM_CONFIGURE as usize, w!("Configure")).unwrap();
        AppendMenuW(
            hmenu,
            startup_flags,
            IDM_RUN_ON_STARTUP as usize,
            w!("Run on Startup"),
        )
        .unwrap();
        AppendMenuW(hmenu, MF_STRING, IDM_EXIT as usize, w!("Exit")).unwrap();

        let mut point = POINT::default();
        GetCursorPos(&mut point).unwrap();
        SetForegroundWindow(hwnd).unwrap();

        let success = TrackPopupMenu(
            hmenu,
            TPM_LEFTALIGN | TPM_RIGHTBUTTON,
            point.x,
            point.y,
            Some(0),
            hwnd,
            None,
        );

        if !success.as_bool() {
            eprintln!(
                "TrackPopupMenu failed to display menu. GetLastError: {:?}",
                GetLastError()
            );
            // Return an error or handle appropriately
            return Err(windows::core::Error::from_win32());
        } else {
            println!("TrackPopupMenu displayed successfully. Waiting for WM_COMMAND...");
        }
    }
    Ok(())
}

pub fn is_run_on_startup_enabled() -> Result<bool, windows::core::Error> {
    unsafe {
        let mut hkey = HKEY::default();
        let status = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            &HSTRING::from(RUN_KEY_PATH),
            Some(0),
            KEY_READ,
            &mut hkey,
        );

        if status != ERROR_SUCCESS {
            return if status.0 as u32 == ERROR_FILE_NOT_FOUND.0 {
                Ok(false) // Key does not exist, so run on startup is disabled
            } else {
                println!("RegOpenKeyExW failed with error: {:?}", status);
                Err(windows::core::Error::from_win32())
            };
        }

        struct RegKeyGuard(HKEY);
        impl Drop for RegKeyGuard {
            fn drop(&mut self) {
                if self.0 != HKEY::default() {
                    let _ = unsafe { RegCloseKey(self.0) };
                }
            }
        }

        let _guard = RegKeyGuard(hkey);

        let value_name = HSTRING::from(APP_REGISTRY_KEY_NAME);
        let status_query = RegQueryValueExW(hkey, &value_name, None, None, None, None);

        match status_query.0 as u32 {
            REG_QUERY_SUCCESS => {
                println!("Run on startup is enabled");
                Ok(true)
            }
            REG_QUERY_ERROR => {
                println!("Run on startup is disabled.");
                Ok(false)
            }
            _ => {
                println!("RegQueryValueExW failed with error: {:?}", status_query);
                Err(windows::core::Error::from_win32())
            }
        }
    }
}

pub fn set_run_on_startup(enable: bool) -> Result<(), windows::core::Error> {
    unsafe {
        let mut hkey = HKEY::default();

        let status = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            &HSTRING::from(RUN_KEY_PATH),
            Some(0),
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut hkey,
            None,
        );

        if status != ERROR_SUCCESS {
            println!("RegCreateKeyExW failed with error: {:?}", status);
            return Err(windows::core::Error::from(status));
        }

        struct RegKeyGuard(HKEY);
        impl Drop for RegKeyGuard {
            fn drop(&mut self) {
                if self.0 != HKEY::default() {
                    unsafe {
                        let _ = RegCloseKey(self.0);
                    }
                }
            }
        }
        let _guard = RegKeyGuard(hkey);

        let value_name = HSTRING::from(APP_REGISTRY_KEY_NAME);

        if enable {
            let exe_path_buf = std::env::current_exe().map_err(|e| {
                windows::core::Error::new(
                    windows::core::HRESULT(0),
                    format!("Failed to get current exe path: {}", e),
                )
            });
            let exe_path_quoted = format!("\"{}\"", exe_path_buf.unwrap().display());
            println!(
                "Setting Run key value '{}' to '{}'",
                APP_REGISTRY_KEY_NAME, exe_path_quoted
            );

            let wide_path: Vec<u16> = OsStr::new(&exe_path_quoted)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();

            let data_size_bytes = wide_path.len() * std::mem::size_of::<u16>();
            let data_ptr = wide_path.as_ptr();
            let data_slice: &[u8] = slice::from_raw_parts(data_ptr as *const u8, data_size_bytes);

            let status_set = RegSetValueExW(hkey, &value_name, Some(0), REG_SZ, Some(data_slice));

            if status_set != ERROR_SUCCESS {
                println!("RegSetValueExW failed: {:?}", status_set);
                return Err(windows::core::Error::from(status_set));
            }
        } else {
            let status_del = RegDeleteValueW(hkey, &value_name);
            if status_del != ERROR_SUCCESS && status_del.0 as u32 != ERROR_FILE_NOT_FOUND.0 {
                return Err(windows::core::Error::from(status_del));
            }
        }
    }
    Ok(())
}
