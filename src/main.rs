#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod dualsense;
mod graphics;
mod renderer;
mod tray;
mod window;

use std::{collections::HashMap, sync::mpsc, thread, time::Duration};

use windows::{
    Win32::{
        Foundation::{HINSTANCE, HWND},
        Graphics::{
            Direct2D::{ID2D1DeviceContext, ID2D1Factory1},
            Direct3D11::ID3D11Device,
            DirectComposition::{
                IDCompositionAnimation, IDCompositionDevice, IDCompositionEffectGroup,
                IDCompositionTarget, IDCompositionVisual,
            },
            DirectWrite::{IDWriteFactory, IDWriteTextFormat},
            Dxgi::{IDXGIDevice, IDXGIFactory2, IDXGISwapChain1},
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            DispatchMessageW, HICON, IMAGE_ICON, LR_DEFAULTSIZE, LR_LOADFROMFILE, LoadImageW, MSG,
            PM_REMOVE, PeekMessageW, TranslateMessage, WM_QUIT, WM_USER,
        },
    },
    core::w,
};

pub const WINDOW_WIDTH: i32 = 200;
pub const WINDOW_HEIGHT: i32 = 150;
const CORNER_RADIUS: f32 = 10.0;
const HOTKEY_ID_TOGGLE: i32 = 1;
const TIMER_ID_FADEOUT: usize = 1;
const SHOW_DURATION_MS: u32 = 3000;

pub const WM_APP_TRAYMSG: u32 = WM_USER + 1;
pub const IDM_CONFIGURE: u16 = 1001;
pub const IDM_EXIT: u16 = 1002;

#[derive(Debug, PartialEq, Clone, Copy)]
enum VisibilityState {
    Visible,
    Hidden,
    FadingOut,
}

#[allow(dead_code)]
struct AppState {
    hwnd: HWND,
    d3d_device: ID3D11Device,
    dxgi_device: IDXGIDevice,
    dxgi_factory: IDXGIFactory2,
    dcomp_device: IDCompositionDevice,
    dcomp_target: IDCompositionTarget,
    dcomp_visual: IDCompositionVisual,
    dcomp_effect_group: IDCompositionEffectGroup,
    swap_chain: IDXGISwapChain1,
    d2d_factory: ID2D1Factory1,
    d2d_device_context: ID2D1DeviceContext,
    dwrite_factory: IDWriteFactory,
    text_format: IDWriteTextFormat,
    dualsense_receiver: mpsc::Receiver<dualsense::ControllerEvent>,
    battery_status_map: HashMap<String, dualsense::BatteryReport>,
    triggering_controller_path: Option<String>,
    visibility_state: VisibilityState,
    fadeout_timer_id: Option<usize>,
    fade_out_animation: Option<IDCompositionAnimation>,
    h_icon: Option<HICON>,
}

fn main() -> Result<(), ()> {
    let dualsense_receiver = dualsense::setup_controller_polling().unwrap();

    let hinstance: HINSTANCE = unsafe { GetModuleHandleW(None).unwrap().into() };
    let hwnd = window::create_overlay_window(hinstance).unwrap();

    let icon_path = w!("app_icon.ico");
    let h_icon = unsafe {
        LoadImageW(
            None,
            icon_path,
            IMAGE_ICON,
            0,
            0,
            LR_LOADFROMFILE | LR_DEFAULTSIZE,
        )
        .map(|handle| HICON(handle.0))
        .map_err(|e| {
            eprintln!("Failed to load icon: {:?}", e);
            e
        })
        .and_then(|hicon| {
            if hicon.is_invalid() {
                eprintln!("Invalid icon handle");
                Err(windows::core::Error::from_win32())
            } else {
                Ok(hicon)
            }
        })
        .ok()
    };

    let graphics_resources =
        graphics::initialize_graphics(hwnd, WINDOW_WIDTH as u32, WINDOW_HEIGHT as u32).unwrap();

    let mut app_state = AppState {
        hwnd,
        dualsense_receiver,
        visibility_state: VisibilityState::Hidden,
        battery_status_map: HashMap::new(),
        triggering_controller_path: None,
        fadeout_timer_id: None,
        d3d_device: graphics_resources.d3d_device,
        dxgi_device: graphics_resources.dxgi_device,
        dxgi_factory: graphics_resources.dxgi_factory,
        dcomp_device: graphics_resources.dcomp_device,
        dcomp_target: graphics_resources.dcomp_target,
        dcomp_visual: graphics_resources.dcomp_visual,
        dcomp_effect_group: graphics_resources.dcomp_effect_group,
        swap_chain: graphics_resources.swap_chain,
        d2d_factory: graphics_resources.d2d_factory,
        d2d_device_context: graphics_resources.d2d_device_context,
        dwrite_factory: graphics_resources.dwrite_factory,
        text_format: graphics_resources.text_format,
        fade_out_animation: graphics_resources.fade_out_animation,
        h_icon,
    };

    window::associate_appstate_with_hwnd(app_state.hwnd, &mut app_state);
    if let Some(icon) = app_state.h_icon {
        tray::add_tray_icon(app_state.hwnd, icon, WM_APP_TRAYMSG).unwrap();
    } else {
        eprintln!("Failed to load icon, not adding to tray");
    }
    window::register_app_hotkey(app_state.hwnd).unwrap();

    unsafe { app_state.dcomp_device.Commit().unwrap() };
    println!("Initial dcomp commit succesful");

    let mut msg = MSG::default();
    loop {
        while unsafe { PeekMessageW(&mut msg, Some(HWND::default()), 0, 0, PM_REMOVE) }.as_bool() {
            if msg.message == WM_QUIT {
                println!("Received WM_QUIT");
                window::unregister_app_hotkey(app_state.hwnd).unwrap();
                tray::remove_tray_icon(app_state.hwnd).unwrap_or_else(|_| {
                    eprintln!("Failed to remove tray icon");
                });
                return Ok(());
            }

            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        match app_state.dualsense_receiver.try_recv() {
            Ok(event) => match event {
                dualsense::ControllerEvent::BatteryUpdate(path, report) => {
                    app_state
                        .battery_status_map
                        .insert(path.clone(), report.clone());

                    if Some(&path) == app_state.triggering_controller_path.as_ref()
                        && app_state.visibility_state != VisibilityState::Hidden
                    {
                        renderer::draw_content(&app_state);
                    }
                }
                dualsense::ControllerEvent::MuteBussonPressed(path) => {
                    println!("Main: Mute button pressed on {}", path);
                    app_state.triggering_controller_path = Some(path.clone());
                    window::handle_hotkey(&mut app_state);
                }
                dualsense::ControllerEvent::DeviceConnected(path) => {
                    println!("Main: Device connected: {}", path);
                }
                dualsense::ControllerEvent::DeviceDisconnected(path) => {
                    println!("Main: Device disconnected: {}", path);
                    app_state.battery_status_map.remove(&path);
                    if Some(&path) == app_state.triggering_controller_path.as_ref() {
                        app_state.triggering_controller_path = None;
                        app_state.visibility_state = VisibilityState::Hidden;
                    }
                }
            },
            Err(mpsc::TryRecvError::Disconnected) => {
                eprintln!("Battery receiver disconnected");
                break Err(());
            }
            _ => {}
        }

        thread::sleep(Duration::from_millis(50));
    }
}
