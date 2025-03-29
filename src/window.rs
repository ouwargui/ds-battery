use windows::{
    Win32::{
        Foundation::{GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
        Graphics::Gdi::{BeginPaint, EndPaint, HBRUSH, PAINTSTRUCT},
        UI::{
            Input::KeyboardAndMouse::{
                MOD_ALT, MOD_CONTROL, RegisterHotKey, UnregisterHotKey, VK_B,
            },
            WindowsAndMessaging::{
                self, CS_HREDRAW, CS_OWNDC, CS_VREDRAW, CreateWindowExW, DefWindowProcW,
                DestroyWindow, GWLP_USERDATA, GetWindowLongPtrW, HICON, HWND_TOPMOST, IDC_ARROW,
                KillTimer, LoadCursorW, PostQuitMessage, RegisterClassExW, SW_HIDE, SW_SHOW,
                SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetTimer, SetWindowLongPtrW, SetWindowPos,
                ShowWindow, WM_COMMAND, WM_DESTROY, WM_HOTKEY, WM_PAINT, WM_RBUTTONUP, WM_TIMER,
                WNDCLASSEXW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
            },
        },
    },
    core::{HRESULT, PCWSTR, w},
};

use crate::{
    AppState, HOTKEY_ID_TOGGLE, IDM_CONFIGURE, IDM_EXIT, SHOW_DURATION_MS, TIMER_ID_FADEOUT,
    VisibilityState, WINDOW_HEIGHT, WINDOW_WIDTH, WM_APP_TRAYMSG, graphics, renderer, tray,
};

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
            WS_EX_TOPMOST | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW,
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

pub fn handle_hotkey(app_state: &mut AppState) {
    match app_state.visibility_state {
        VisibilityState::Hidden | VisibilityState::FadingOut => {
            println!("Showing window");
            app_state.visibility_state = VisibilityState::Visible;

            if let Some(old_timer_id) = app_state.fadeout_timer_id.take() {
                println!("Killing previous timer {}", old_timer_id);
                unsafe {
                    KillTimer(Some(app_state.hwnd), old_timer_id).expect("Failed to kill timer")
                };
            }

            graphics::apply_opacity(&app_state.dcomp_device, &app_state.dcomp_effect_group, 1.0);
            match unsafe { app_state.dcomp_device.Commit() } {
                Ok(_) => println!("Committed DComp changes successfully in handle_hotkey."),
                Err(e) => eprintln!("!!! FAILED to commit DComp changes in handle_hotkey: {}", e),
            };

            show_and_set_topmost(&app_state.hwnd);

            let percent_to_draw = app_state
                .last_battery_report
                .as_ref()
                .map_or(0, |r| r.battery_capacity);
            let charging_to_draw = app_state
                .last_battery_report
                .as_ref()
                .map_or(false, |r| r.charging_status);
            renderer::draw_content(
                &app_state.d2d_device_context,
                &app_state.dwrite_factory,
                &app_state.text_format,
                &app_state.swap_chain,
                crate::CORNER_RADIUS,
                percent_to_draw,
                charging_to_draw,
            );

            let new_timer_id = unsafe {
                SetTimer(
                    Some(app_state.hwnd),
                    TIMER_ID_FADEOUT,
                    SHOW_DURATION_MS,
                    None,
                )
            };
            if new_timer_id == 0 {
                eprintln!(
                    "!!! FAILED to set initial fadeout timer! Error: {:?}",
                    unsafe { GetLastError() }
                );
                app_state.fadeout_timer_id = None; // Ensure state is consistent
            } else {
                println!(
                    "Set initial timer. ID: {}, Duration: {}ms",
                    new_timer_id, SHOW_DURATION_MS
                );
                app_state.fadeout_timer_id = Some(new_timer_id); // Store the NEW ID
            }
        }
        VisibilityState::Visible => {
            println!("Window already visible, resetting timer");
            if let Some(timer_id) = app_state.fadeout_timer_id {
                println!("Killing existing timer {}", timer_id);
                unsafe { KillTimer(Some(app_state.hwnd), timer_id).expect("Failed to kill timer") };
                let new_timer_id = unsafe {
                    SetTimer(
                        Some(app_state.hwnd),
                        TIMER_ID_FADEOUT,
                        SHOW_DURATION_MS,
                        None,
                    )
                };
                if new_timer_id == 0 {
                    eprintln!("!!! FAILED to set fadeout timer! Error: {:?}", unsafe {
                        GetLastError()
                    });
                    app_state.fadeout_timer_id = None;
                } else {
                    println!(
                        "SetTimer succeeded. Timer ID: {}, Duration: {}ms",
                        timer_id, SHOW_DURATION_MS
                    );
                    app_state.fadeout_timer_id = Some(new_timer_id);
                }
            } else {
                eprintln!("Warning: State is visible but no timer ID found. Setting new timer.");
                let new_timer_id = unsafe {
                    SetTimer(
                        Some(app_state.hwnd),
                        TIMER_ID_FADEOUT,
                        SHOW_DURATION_MS,
                        None,
                    )
                };
                if new_timer_id == 0 {
                    eprintln!("!!! FAILED to set fadeout timer! Error: {:?}", unsafe {
                        GetLastError()
                    });
                    app_state.fadeout_timer_id = None;
                } else {
                    println!(
                        "SetTimer succeeded. Timer ID: {}, Duration: {}ms",
                        new_timer_id, SHOW_DURATION_MS
                    );
                    app_state.fadeout_timer_id = Some(new_timer_id);
                }
            }
        }
    }
}

unsafe fn handle_fadeout_timer(app_state: &mut AppState) {
    println!(
        "handle_fadeout_timer called. Current state: {:?}",
        app_state.visibility_state
    );

    if app_state.visibility_state == VisibilityState::Visible {
        println!("Fading out window");
        app_state.visibility_state = VisibilityState::FadingOut;

        if let Some(timer_id) = app_state.fadeout_timer_id.take() {
            unsafe { KillTimer(Some(app_state.hwnd), timer_id).expect("Failed to kill timer") };
        } else {
            println!("Warning: Fadeout timer ID was already None.");
        }

        if let Some(anim) = &app_state.fade_out_animation {
            println!("apply_opacity_animation called successfully.");
            graphics::apply_opacity_animation(&app_state.dcomp_effect_group, anim);
        } else {
            eprintln!("!!! Error: fade_out_animation object is None!");

            app_state.visibility_state = VisibilityState::Hidden;
            graphics::apply_opacity(&app_state.dcomp_device, &app_state.dcomp_effect_group, 0.0);
            unsafe {
                ShowWindow(app_state.hwnd, SW_HIDE).expect("Failed to show window");
            };
        }

        unsafe { app_state.dcomp_device.Commit().expect("Failed to commit") };
    } else {
        println!(
            "handle_fadeout_timer called but state was not Visible ({:?}). Ignoring.",
            app_state.visibility_state
        );
        graphics::apply_opacity(&app_state.dcomp_device, &app_state.dcomp_effect_group, 0.0);
        unsafe {
            ShowWindow(app_state.hwnd, SW_HIDE).expect("Failed to show window");
        };
        if let Some(timer_id) = app_state.fadeout_timer_id.take() {
            println!("Killing unexpected timer {}", timer_id);
            unsafe { KillTimer(Some(app_state.hwnd), timer_id).expect("Failed to kill timer") };
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
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}

pub fn associate_appstate_with_hwnd(hwnd: HWND, app_state: &mut AppState) {
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, app_state as *mut _ as isize);
    }
}

pub fn register_app_hotkey(hwnd: HWND) -> Result<(), ()> {
    let modifiers = MOD_CONTROL | MOD_ALT;
    let vk = VK_B.0 as u32;
    unsafe { RegisterHotKey(Some(hwnd), HOTKEY_ID_TOGGLE, modifiers, vk).unwrap() };
    Ok(())
}

pub fn unregister_app_hotkey(hwnd: HWND) -> Result<(), ()> {
    unsafe {
        UnregisterHotKey(Some(hwnd), HOTKEY_ID_TOGGLE).unwrap();
    }
    Ok(())
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let app_state_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };

    if msg == WM_APP_TRAYMSG {
        let mouse_msg = (lparam.0 & 0xFFFF) as u32;
        match mouse_msg {
            WM_RBUTTONUP => {
                println!("Tray icon right-clicked");
                tray::show_context_menu(hwnd).unwrap_or_else(|_| {
                    eprintln!("Failed to show context menu");
                });
                return LRESULT(0);
            }
            _ => {}
        }
    }

    if app_state_ptr != 0 {
        let app_state = unsafe { &mut *(app_state_ptr as *mut AppState) };

        match msg {
            WM_HOTKEY => {
                if wparam.0 as i32 == HOTKEY_ID_TOGGLE {
                    println!("WM_HOTKEY received");
                    handle_hotkey(app_state);
                    return LRESULT(0);
                }
            }
            WM_TIMER => {
                if wparam.0 == TIMER_ID_FADEOUT {
                    println!("WM_TIMER received");
                    unsafe { handle_fadeout_timer(app_state) };
                    return LRESULT(0);
                }
            }
            WM_COMMAND => {
                let menu_id = (wparam.0 & 0xFFFF) as u16;
                match menu_id {
                    IDM_CONFIGURE => {
                        println!("Configure menu item clicked");
                        return LRESULT(0);
                    }
                    IDM_EXIT => {
                        println!("Exit menu item clicked");
                        let _ = unsafe { DestroyWindow(hwnd) };
                        return LRESULT(0);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    match msg {
        WM_PAINT => {
            println!("WM_PAINT received");
            let mut ps = PAINTSTRUCT::default();
            let _hdc = unsafe { BeginPaint(hwnd, &mut ps) };
            unsafe {
                let _ = EndPaint(hwnd, &ps);
            };
            LRESULT(0)
        }
        WM_DESTROY => {
            // This message is sent when the window is being destroyed (e.g., user clicks close button)
            println!("WM_DESTROY received");
            // Post a WM_QUIT message to the message queue to signal the message loop to exit
            unsafe {
                KillTimer(Some(hwnd), TIMER_ID_FADEOUT).expect("Failed to kill timer");
                PostQuitMessage(0)
            };
            LRESULT(0)
        }
        _ => {
            // For messages we don't handle explicitly, pass them to the default window procedure
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
    }
}
