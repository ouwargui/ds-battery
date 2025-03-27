mod dualsense;
mod graphics;
mod renderer;
mod window;

use std::{sync::mpsc, thread, time::Duration};

use windows::Win32::{
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
        DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, TranslateMessage, WM_QUIT,
    },
};

pub const WINDOW_WIDTH: i32 = 200;
pub const WINDOW_HEIGHT: i32 = 150;
const CORNER_RADIUS: f32 = 10.0;
const HOTKEY_ID_TOGGLE: i32 = 1;
const TIMER_ID_FADEOUT: usize = 1;
const SHOW_DURATION_MS: u32 = 3000;

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
    battery_receiver: mpsc::Receiver<dualsense::BatteryReport>,
    last_battery_report: Option<dualsense::BatteryReport>,
    visibility_state: VisibilityState,
    fadeout_timer_id: Option<usize>,
    fade_out_animation: Option<IDCompositionAnimation>,
}

fn main() -> Result<(), ()> {
    let battery_receiver = dualsense::setup_controller_polling().unwrap();

    let hinstance: HINSTANCE = unsafe { GetModuleHandleW(None).unwrap().into() };

    let hwnd = window::create_overlay_window(hinstance).unwrap();

    let graphics_resources =
        graphics::initialize_graphics(hwnd, WINDOW_WIDTH as u32, WINDOW_HEIGHT as u32).unwrap();

    let mut app_state = AppState {
        hwnd,
        battery_receiver,
        last_battery_report: None,
        visibility_state: VisibilityState::Hidden,
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
    };

    window::associate_appstate_with_hwnd(app_state.hwnd, &mut app_state);
    window::register_app_hotkey(app_state.hwnd).unwrap();

    unsafe { app_state.dcomp_device.Commit().unwrap() };
    println!("Initial dcomp commit succesful");

    let mut msg = MSG::default();
    loop {
        while unsafe { PeekMessageW(&mut msg, Some(HWND::default()), 0, 0, PM_REMOVE) }.as_bool() {
            if msg.message == WM_QUIT {
                println!("Received WM_QUIT");
                window::unregister_app_hotkey(app_state.hwnd).unwrap();
                return Ok(());
            }

            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        if app_state.visibility_state != VisibilityState::Hidden {
            match app_state.battery_receiver.try_recv() {
                Ok(new_report) => {
                    let needs_redraw =
                        app_state.last_battery_report.as_ref().map_or(true, |last| {
                            new_report.battery_capacity != last.battery_capacity
                        });

                    if needs_redraw {
                        renderer::draw_content(
                            &app_state.d2d_device_context,
                            &app_state.dwrite_factory,
                            &app_state.text_format,
                            &app_state.swap_chain,
                            CORNER_RADIUS,
                            new_report.battery_capacity,
                            new_report.charging_status,
                        );

                        app_state.last_battery_report = Some(new_report);
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    eprintln!("Battery receiver disconnected");
                    break Err(());
                }
                _ => {}
            }
        }

        thread::sleep(Duration::from_millis(50));
    }
}
