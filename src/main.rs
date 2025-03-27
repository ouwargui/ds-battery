// use ds_battery::{direct_composition::Overlay, dualsense_manager::start_controller_polling};
// use std::{thread, time::Duration};

// fn main() {
//     // start_controller_polling();
//     let overlay = Overlay::new().expect("Failed to initialize DirectComposition overlay");
//     overlay.run_message_loop();

//     // loop {
//     //     thread::sleep(Duration::from_secs(1));
//     // }
// }

mod graphics;
mod renderer;
mod window;

use windows::{
    Win32::{
        Foundation::{GetLastError, HINSTANCE, HWND},
        Graphics::{
            Direct2D::{ID2D1DeviceContext, ID2D1Factory1},
            Direct3D11::ID3D11Device,
            DirectComposition::{IDCompositionDevice, IDCompositionTarget, IDCompositionVisual},
            DirectWrite::{IDWriteFactory, IDWriteTextFormat},
            Dxgi::{IDXGIDevice, IDXGIFactory2, IDXGISwapChain1},
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{DispatchMessageW, GetMessageW, MSG, TranslateMessage},
    },
    core::{BOOL, HRESULT},
};

const WINDOW_WIDTH: i32 = 200;
const WINDOW_HEIGHT: i32 = 150;

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
}

fn main() {
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

    let app_state = AppState {
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
    };

    window::show_and_set_topmost(&app_state.hwnd);

    let corner_radius = 10.0;
    let battery_percentage = 25;
    renderer::draw_content(
        &app_state.d2d_device_context,
        &app_state.dwrite_factory, // Pass DWrite factory
        &app_state.text_format,    // Pass text format
        &app_state.swap_chain,
        corner_radius,
        battery_percentage,
    );

    let mut msg = MSG::default();
    loop {
        let should_break = run_window(&mut msg).expect("Failed to run window");
        if should_break {
            break;
        }
    }
}

fn run_window(msg: &mut MSG) -> Result<bool, windows::core::Error> {
    // GetMessageW waits for a message
    let result: BOOL = unsafe { GetMessageW(msg, Some(HWND::default()), 0, 0) };

    match result.0 {
        -1 => {
            // Error occurred
            let error = unsafe { GetLastError() };
            return Err(windows::core::Error::new(
                HRESULT::from(error),
                "GetMessageW error",
            ));
        }
        0 => Ok(true),
        _ => {
            // Message received, process it
            unsafe {
                let _ = TranslateMessage(msg); // Translates virtual-key messages
                DispatchMessageW(msg); // Dispatches message to the window procedure (wndproc)
            };
            Ok(false)
        }
    }
}
