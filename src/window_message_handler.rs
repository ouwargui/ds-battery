use crate::{
    AppState, HOTKEY_ID_TOGGLE, IDM_CONFIGURE, IDM_EXIT, SHOW_DURATION_MS, TIMER_ID_FADEOUT,
    VisibilityState, WM_APP_TRAYMSG, graphics, renderer, tray,
};
use windows::Win32::{
    Foundation::{GetLastError, HWND, LPARAM, LRESULT, WPARAM},
    Graphics::Gdi::{BeginPaint, EndPaint, PAINTSTRUCT},
    UI::WindowsAndMessaging::{
        DestroyWindow, KillTimer, PostQuitMessage, SW_HIDE, SetTimer, ShowWindow, WM_COMMAND,
        WM_DESTROY, WM_HOTKEY, WM_PAINT, WM_RBUTTONUP, WM_TIMER,
    },
};

pub fn handle_message(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    app_state: &mut AppState,
) -> Option<LRESULT> {
    match msg {
        WM_HOTKEY => handle_hotkey_message(hwnd, wparam, app_state),
        WM_TIMER => handle_timer_message(hwnd, wparam, app_state),
        WM_COMMAND => handle_command_message(hwnd, wparam),
        WM_APP_TRAYMSG => handle_tray_message(hwnd, lparam),
        WM_PAINT => handle_paint_message(hwnd),
        WM_DESTROY => handle_destroy_message(hwnd),
        _ => None,
    }
}

fn handle_hotkey_message(_hwnd: HWND, wparam: WPARAM, app_state: &mut AppState) -> Option<LRESULT> {
    if wparam.0 as i32 == HOTKEY_ID_TOGGLE {
        toggle_window_visibility(app_state);
        Some(LRESULT(0))
    } else {
        None
    }
}

fn handle_timer_message(_hwnd: HWND, wparam: WPARAM, app_state: &mut AppState) -> Option<LRESULT> {
    if wparam.0 == TIMER_ID_FADEOUT {
        start_fade_out(app_state);
        Some(LRESULT(0))
    } else {
        None
    }
}

fn handle_command_message(hwnd: HWND, wparam: WPARAM) -> Option<LRESULT> {
    let menu_id = (wparam.0 & 0xFFFF) as u16;
    match menu_id {
        IDM_CONFIGURE => {
            println!("Configure menu item clicked");
            Some(LRESULT(0))
        }
        IDM_EXIT => {
            println!("Exit menu item clicked");
            unsafe {
                let _ = DestroyWindow(hwnd);
            };
            Some(LRESULT(0))
        }
        _ => None,
    }
}

fn handle_tray_message(hwnd: HWND, lparam: LPARAM) -> Option<LRESULT> {
    let mouse_msg = (lparam.0 & 0xFFFF) as u32;
    if mouse_msg == WM_RBUTTONUP {
        println!("Tray icon right-clicked");
        tray::show_context_menu(hwnd).unwrap_or_else(|e| {
            eprintln!("Failed to show context menu: {}", e);
        });
        Some(LRESULT(0))
    } else {
        None
    }
}

fn handle_paint_message(hwnd: HWND) -> Option<LRESULT> {
    println!("WM_PAINT received");
    unsafe {
        let mut ps = PAINTSTRUCT::default();
        let _hdc = BeginPaint(hwnd, &mut ps);
        let _ = EndPaint(hwnd, &ps);
    };
    Some(LRESULT(0))
}

fn handle_destroy_message(hwnd: HWND) -> Option<LRESULT> {
    println!("WM_DESTROY received");
    unsafe {
        let _ = KillTimer(Some(hwnd), TIMER_ID_FADEOUT);
        PostQuitMessage(0);
    };
    Some(LRESULT(0))
}

// --- Visibility and Timer Logic ---

pub fn toggle_window_visibility(app_state: &mut AppState) {
    match app_state.visibility_state {
        VisibilityState::Hidden | VisibilityState::FadingOut => {
            show_window_and_start_timer(app_state);
        }
        VisibilityState::Visible => {
            reset_window_timer(app_state);
        }
    }
}

fn show_window_and_start_timer(app_state: &mut AppState) {
    println!("Showing window");
    app_state.visibility_state = VisibilityState::Visible;

    kill_existing_timer(app_state);
    apply_full_opacity(app_state);
    commit_dcomp_changes(app_state); // Commit opacity change
    crate::window::show_and_set_topmost(&app_state.hwnd);
    renderer::draw_content(app_state);
    start_new_timer(app_state);
    commit_dcomp_changes(app_state); // Commit potential draw changes? Maybe redundant
}

fn reset_window_timer(app_state: &mut AppState) {
    println!("Window already visible, resetting timer");
    renderer::draw_content(app_state);
    kill_existing_timer(app_state);
    start_new_timer(app_state);
}

fn start_fade_out(app_state: &mut AppState) {
    println!(
        "Fadeout timer triggered. Current state: {:?}",
        app_state.visibility_state
    );

    if app_state.visibility_state != VisibilityState::Visible {
        println!("State is not Visible, hiding immediately.");
        hide_window_immediately(app_state);
        return;
    }

    println!("Fading out window");
    app_state.visibility_state = VisibilityState::FadingOut;
    kill_existing_timer(app_state); // Should already be killed by WM_TIMER, but belt-and-suspenders

    if let Some(anim) = &app_state.fade_out_animation {
        graphics::apply_opacity_animation(&app_state.dcomp_effect_group, anim);
        commit_dcomp_changes(app_state);
    } else {
        eprintln!("Error: fade_out_animation object is None! Hiding immediately.");
        hide_window_immediately(app_state);
    }
}

fn hide_window_immediately(app_state: &mut AppState) {
    app_state.visibility_state = VisibilityState::Hidden;
    kill_existing_timer(app_state); // Ensure timer is gone
    apply_zero_opacity(app_state);
    commit_dcomp_changes(app_state);
    unsafe {
        let _ = ShowWindow(app_state.hwnd, SW_HIDE);
    };
}

fn kill_existing_timer(app_state: &mut AppState) {
    if let Some(old_timer_id) = app_state.fadeout_timer_id.take() {
        println!("Killing timer {}", old_timer_id);
        unsafe {
            if KillTimer(Some(app_state.hwnd), old_timer_id).is_err() {
                // Log error only if KillTimer fails, GetLastError is less reliable here
                eprintln!("Failed to kill timer {}", old_timer_id);
            }
        };
    }
}

fn start_new_timer(app_state: &mut AppState) {
    let new_timer_id = unsafe {
        SetTimer(
            Some(app_state.hwnd),
            TIMER_ID_FADEOUT,
            SHOW_DURATION_MS,
            None,
        )
    };
    if new_timer_id == 0 {
        eprintln!("Failed to set fadeout timer! Error: {:?}", unsafe {
            GetLastError()
        });
        app_state.fadeout_timer_id = None;
    } else {
        println!(
            "Set timer. ID: {}, Duration: {}ms",
            new_timer_id, SHOW_DURATION_MS
        );
        app_state.fadeout_timer_id = Some(new_timer_id);
    }
}

fn apply_full_opacity(app_state: &AppState) {
    graphics::apply_opacity(&app_state.dcomp_device, &app_state.dcomp_effect_group, 1.0);
}

fn apply_zero_opacity(app_state: &AppState) {
    graphics::apply_opacity(&app_state.dcomp_device, &app_state.dcomp_effect_group, 0.0);
}

fn commit_dcomp_changes(app_state: &AppState) {
    match unsafe { app_state.dcomp_device.Commit() } {
        Ok(_) => {} // println!("Committed DComp changes successfully."),
        Err(e) => eprintln!("Failed to commit DComp changes: {}", e),
    };
}
