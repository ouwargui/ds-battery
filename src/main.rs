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

use windows::{
    Win32::{
        Foundation::{GetLastError, HINSTANCE, HMODULE, HWND, LPARAM, LRESULT, WPARAM},
        Graphics::{
            Direct2D::{
                Common::{
                    D2D_RECT_F, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT,
                },
                D2D1_DEBUG_LEVEL_INFORMATION, D2D1_DEBUG_LEVEL_NONE,
                D2D1_DEVICE_CONTEXT_OPTIONS_NONE, D2D1_FACTORY_OPTIONS,
                D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT,
                D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT,
                D2D1_RENDER_TARGET_USAGE_NONE, D2D1_ROUNDED_RECT, D2D1CreateFactory,
                ID2D1DeviceContext, ID2D1Factory1,
            },
            Direct3D::D3D_DRIVER_TYPE_HARDWARE,
            Direct3D11::{
                D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice,
                ID3D11Device,
            },
            DirectComposition::{
                DCompositionCreateDevice, IDCompositionDevice, IDCompositionTarget,
            },
            Dxgi::{
                Common::{
                    DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC,
                },
                DXGI_PRESENT, DXGI_SCALING_STRETCH, DXGI_SWAP_CHAIN_DESC1,
                DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL, DXGI_USAGE_RENDER_TARGET_OUTPUT, IDXGIDevice,
                IDXGIFactory2, IDXGISurface, IDXGISwapChain1,
            },
            Gdi::{BeginPaint, EndPaint, HBRUSH, PAINTSTRUCT},
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            self, CS_HREDRAW, CS_OWNDC, CS_VREDRAW, CreateWindowExW, DefWindowProcW,
            DispatchMessageW, GetMessageW, HICON, IDC_ARROW, LoadCursorW, MSG, PostQuitMessage,
            RegisterClassExW, SW_SHOW, ShowWindow, TranslateMessage, WM_DESTROY, WM_PAINT,
            WNDCLASSEXW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
        },
    },
    core::{BOOL, HRESULT, PCWSTR, w},
};

use windows::core::Interface;

const WINDOW_WIDTH: i32 = 200;
const WINDOW_HEIGHT: i32 = 150;

fn main() {
    let hinstance: HINSTANCE = unsafe {
        GetModuleHandleW(None)
            .expect("Failed to create module handle")
            .into()
    };

    let hwnd = create_overlay_window(hinstance).expect("Failed to create window");
    let (d3d_device, dxgi_device) = create_d3d_device();
    let dcomp_device = create_dcomp_device(&dxgi_device);
    let dcomp_target = create_dcomp_target(&dcomp_device, hwnd);
    let d2d_factory = create_d2d_factory();
    let d2d_device_context = create_d2d_device_context(&d2d_factory, &dxgi_device);

    let visual = unsafe {
        dcomp_device
            .CreateVisual()
            .expect("Failed to create visual")
    };

    let dxgi_factory = get_dxgi_factory(&d3d_device);
    let swap_chain = create_composition_swapchain(
        &dxgi_factory,
        &d3d_device,
        WINDOW_WIDTH as u32,
        WINDOW_HEIGHT as u32,
    );

    unsafe {
        visual
            .SetContent(&swap_chain)
            .expect("Failed to set visual content");

        dcomp_target.SetRoot(&visual).expect("Failed to set root");

        dcomp_device.Commit().expect("Failed to commit");

        let _ = ShowWindow(hwnd, SW_SHOW);
    }

    draw_background(&d2d_device_context, &swap_chain, 0.0, 0.0, 0.0, 0.7, 10.0);

    let mut msg = MSG::default();
    loop {
        let should_break = run_window(&mut msg).expect("Failed to run window");
        if should_break {
            break;
        }
    }
}

fn create_d3d_device() -> (ID3D11Device, IDXGIDevice) {
    let mut d3d_device = None;
    unsafe {
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut d3d_device),
            None,
            None,
        )
    }
    .expect("Failed to create D3D11 device");
    let d3d_device = d3d_device.unwrap();
    let dxgi_device: IDXGIDevice = d3d_device.cast().expect("Failed to cast to IDXGIDevice");
    (d3d_device, dxgi_device)
}

fn create_dcomp_device(dxgi_device: &IDXGIDevice) -> IDCompositionDevice {
    unsafe { DCompositionCreateDevice(dxgi_device).expect("Failed to create DComp device") }
}

fn create_dcomp_target(dcomp_device: &IDCompositionDevice, hwnd: HWND) -> IDCompositionTarget {
    unsafe {
        dcomp_device
            .CreateTargetForHwnd(hwnd, true)
            .expect("Failed to create dcomp target")
    }
}

fn create_overlay_window(hinstance: HINSTANCE) -> Result<HWND, windows::core::Error> {
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

fn create_d2d_factory() -> ID2D1Factory1 {
    let options = D2D1_FACTORY_OPTIONS {
        debugLevel: if cfg!(debug_assertions) {
            D2D1_DEBUG_LEVEL_INFORMATION
        } else {
            D2D1_DEBUG_LEVEL_NONE
        },
    };

    unsafe {
        D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, Some(&options))
            .expect("Failed to create d2d factory")
    }
}

fn create_d2d_device_context(
    d2d_factory: &ID2D1Factory1,
    dxgi_device: &IDXGIDevice,
) -> ID2D1DeviceContext {
    let d2d_device = unsafe {
        d2d_factory
            .CreateDevice(dxgi_device)
            .expect("Failed to create d2d device")
    };
    unsafe {
        d2d_device
            .CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)
            .expect("Failed to create d2d device context")
    }
}

fn get_dxgi_factory(d3d_device: &ID3D11Device) -> IDXGIFactory2 {
    let dxgi_device: IDXGIDevice = d3d_device.cast().expect("Failed to cast to IDXGIDevice");
    let dxgi_adapter = unsafe { dxgi_device.GetAdapter().expect("Failed to get adapter") };
    unsafe {
        dxgi_adapter
            .GetParent()
            .expect("Failed to get dxgi adapter parent")
    }
}

fn create_composition_swapchain(
    dxgi_factory: &IDXGIFactory2,
    d3d_device: &ID3D11Device,
    width: u32,
    height: u32,
) -> IDXGISwapChain1 {
    let desc = DXGI_SWAP_CHAIN_DESC1 {
        Width: width,
        Height: height,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        Stereo: false.into(),
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
        BufferCount: 2,
        Scaling: DXGI_SCALING_STRETCH,
        SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
        AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
        Flags: 0,
    };

    unsafe {
        dxgi_factory
            .CreateSwapChainForComposition(d3d_device, &desc, None)
            .expect("Failed to create composition swap chain")
    }
}

fn draw_background(
    context: &ID2D1DeviceContext,
    swap_chain: &IDXGISwapChain1,
    r: f32,
    g: f32,
    b: f32,
    a: f32,
    corner_radius: f32,
) {
    let surface: IDXGISurface = unsafe {
        swap_chain
            .GetBuffer(0)
            .expect("Failed to get swap chain buffer")
    };

    let props = D2D1_RENDER_TARGET_PROPERTIES {
        r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
        pixelFormat: D2D1_PIXEL_FORMAT {
            format: DXGI_FORMAT_B8G8R8A8_UNORM,
            alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
        },
        dpiX: 0.0,
        dpiY: 0.0,
        usage: D2D1_RENDER_TARGET_USAGE_NONE,
        minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
    };

    let d2d_device = unsafe { context.GetDevice().expect("Failed to get d2d device") };
    let d2d_factory = unsafe { d2d_device.GetFactory().expect("Failed to get d2d factory") };

    let render_target = unsafe {
        d2d_factory
            .CreateDxgiSurfaceRenderTarget(&surface, &props)
            .expect("Failed to create render target")
    };

    let clear_color = D2D1_COLOR_F {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };
    let fill_color = D2D1_COLOR_F { r, g, b, a };

    let brush = unsafe {
        render_target
            .CreateSolidColorBrush(&fill_color, None)
            .expect("Failed to create brush")
    };

    let rect = D2D_RECT_F {
        left: 0.0,
        top: 0.0,
        right: WINDOW_WIDTH as f32,
        bottom: WINDOW_HEIGHT as f32,
    };

    let rounded_rect = D2D1_ROUNDED_RECT {
        rect,
        radiusX: corner_radius,
        radiusY: corner_radius,
    };

    unsafe {
        render_target.BeginDraw();
        render_target.Clear(Some(&clear_color));
        render_target.FillRoundedRectangle(&rounded_rect, &brush);
        let _ = render_target.EndDraw(None, None);

        let _ = swap_chain.Present(1, DXGI_PRESENT::default());
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
