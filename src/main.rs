mod dualsense;
mod graphics;
mod renderer;
mod window;

use std::{sync::mpsc, thread, time::Duration};

use dualsense::BatteryReport;
use windows::Win32::{
    Foundation::{HINSTANCE, HWND},
    Graphics::{
        Direct2D::{ID2D1DeviceContext, ID2D1Factory1},
        Direct3D11::ID3D11Device,
        DirectComposition::{IDCompositionDevice, IDCompositionTarget, IDCompositionVisual},
        DirectWrite::{IDWriteFactory, IDWriteTextFormat},
        Dxgi::{IDXGIDevice, IDXGIFactory2, IDXGISwapChain1},
    },
    System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::{
        DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, TranslateMessage, WM_QUIT,
    },
};

const WINDOW_WIDTH: i32 = 200;
const WINDOW_HEIGHT: i32 = 150;
const CORNER_RADIUS: f32 = 10.0;

struct AppState {
    hwnd: HWND,
    d3d_device: ID3D11Device,
    dxgi_device: IDXGIDevice,
    dxgi_factory: IDXGIFactory2,
    dcomp_device: IDCompositionDevice,
    dcomp_target: IDCompositionTarget,
    dcomp_visual: IDCompositionVisual,
    swap_chain: IDXGISwapChain1,
    d2d_factory: ID2D1Factory1,
    d2d_device_context: ID2D1DeviceContext,
    dwrite_factory: IDWriteFactory,
    text_format: IDWriteTextFormat,

    battery_receiver: mpsc::Receiver<dualsense::BatteryReport>,
    last_battery_report: Option<dualsense::BatteryReport>,
}

fn main() -> Result<(), ()> {
    let (battery_sender, battery_receiver) = mpsc::channel::<dualsense::BatteryReport>();
    dualsense::poll_controller_battery(battery_sender);

    let hinstance: HINSTANCE = unsafe {
        GetModuleHandleW(None)
            .expect("Failed to create module handle")
            .into()
    };

    let hwnd = window::create_overlay_window(hinstance).expect("Failed to create window");
    let (d3d_device, dxgi_device) = graphics::create_d3d_device();
    let dxgi_factory = graphics::get_dxgi_factory(&d3d_device);
    let dcomp_device = graphics::create_dcomp_device(&dxgi_device);
    let dcomp_target = graphics::create_dcomp_target(&dcomp_device, hwnd);
    let d2d_factory = graphics::create_d2d_factory();
    let d2d_device_context = graphics::create_d2d_device_context(&d2d_factory, &dxgi_device);
    let dwrite_factory = graphics::create_dwrite_factory();
    let text_format = graphics::create_text_format(&dwrite_factory);

    let dcomp_visual = unsafe {
        dcomp_device
            .CreateVisual()
            .expect("Failed to create visual")
    };

    let swap_chain = graphics::create_composition_swapchain(
        &dxgi_factory,
        &d3d_device,
        WINDOW_WIDTH as u32,
        WINDOW_HEIGHT as u32,
    );

    unsafe {
        dcomp_visual
            .SetContent(&swap_chain)
            .expect("Failed to set visual content");

        dcomp_target
            .SetRoot(&dcomp_visual)
            .expect("Failed to set root");

        dcomp_device.Commit().expect("Failed to commit");
    }

    let mut app_state = AppState {
        hwnd,
        d3d_device,
        dxgi_device,
        dxgi_factory,
        dcomp_device,
        dcomp_target,
        dcomp_visual,
        swap_chain,
        d2d_device_context,
        d2d_factory,
        dwrite_factory,
        text_format,
        battery_receiver,
        last_battery_report: None,
    };

    window::show_and_set_topmost(&app_state.hwnd);

    renderer::draw_content(
        &app_state.d2d_device_context,
        &app_state.dwrite_factory, // Pass DWrite factory
        &app_state.text_format,    // Pass text format
        &app_state.swap_chain,
        CORNER_RADIUS,
        BatteryReport {
            battery_capacity: 0,
            charging_status: false,
            is_healthy: false,
        }, // inital percentage
    );

    let mut msg = MSG::default();
    loop {
        while unsafe { PeekMessageW(&mut msg, Some(HWND::default()), 0, 0, PM_REMOVE) }.as_bool() {
            if msg.message == WM_QUIT {
                return Ok(());
            }

            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        match app_state.battery_receiver.try_recv() {
            Ok(new_report) => {
                let needs_redraw = match &app_state.last_battery_report {
                    Some(last_report) => {
                        new_report.battery_capacity != last_report.battery_capacity
                    }
                    None => true,
                };

                if needs_redraw {
                    renderer::draw_content(
                        &app_state.d2d_device_context,
                        &app_state.dwrite_factory,
                        &app_state.text_format,
                        &app_state.swap_chain,
                        CORNER_RADIUS,
                        new_report.clone(),
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
        thread::sleep(Duration::from_millis(100));
    }
}
