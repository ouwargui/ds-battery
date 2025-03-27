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
        Foundation::{GetLastError, HMODULE, HWND, LPARAM, LRESULT, WPARAM},
        Graphics::{
            Direct3D::D3D_DRIVER_TYPE_HARDWARE,
            Direct3D11::{D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice},
            DirectComposition::{
                DCompositionCreateDevice, IDCompositionDevice, IDCompositionTarget,
            },
            Dxgi::IDXGIDevice,
            Gdi::{BeginPaint, COLOR_WINDOW, EndPaint, PAINTSTRUCT},
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            CS_HREDRAW, CS_OWNDC, CS_VREDRAW, CW_USEDEFAULT, CreateWindowExW, DefWindowProcW,
            DispatchMessageW, GetMessageW, HICON, IDC_ARROW, LoadCursorW, MSG, PostQuitMessage,
            RegisterClassExW, SW_SHOWDEFAULT, ShowWindow, TranslateMessage, WINDOW_EX_STYLE,
            WM_DESTROY, WM_PAINT, WNDCLASSEXW, WS_OVERLAPPEDWINDOW,
        },
    },
    core::{BOOL, HRESULT, PCWSTR, w},
};

use windows::core::Interface;

fn main() {
    let dcomp_device = create_dcomp_device();
    println!("DComp device: {:?}", dcomp_device);

    let dcomp_target = create_dcomp_target(&dcomp_device);
    println!("DComp target: {:?}", dcomp_target);

    let msg = MSG::default();
    loop {
        let should_break = run_window(msg).expect("Failed to run window");
        if should_break {
            break;
        }
    }
}

fn create_dcomp_device() -> IDCompositionDevice {
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

    unsafe { DCompositionCreateDevice(&dxgi_device).expect("Failed to create DComp device") }
}

fn create_dcomp_target(dcomp_device: &IDCompositionDevice) -> IDCompositionTarget {
    let hwnd = create_window().expect("Failed to create window");

    unsafe {
        dcomp_device
            .CreateTargetForHwnd(hwnd, true)
            .expect("Failed to create dcomp target")
    }
}

fn create_window() -> Result<HWND, windows::core::Error> {
    let hinstance = unsafe { GetModuleHandleW(None).expect("Failed to create module handle") };
    let class_name = w!("sample_window_class");
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW | CS_OWNDC,
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinstance.into(),
        hIcon: HICON::default(),
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW).expect("Failed to load cursor") },
        hbrBackground: unsafe { windows::Win32::Graphics::Gdi::GetSysColorBrush(COLOR_WINDOW) },
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

    let hwnd: HWND = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class_name,
            w!("My first window"),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            Some(HWND::default()),
            None,
            Some(hinstance.into()),
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

    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWDEFAULT);
    };

    Ok(hwnd)
}

fn run_window(mut msg: MSG) -> Result<bool, windows::core::Error> {
    // GetMessageW waits for a message
    let result: BOOL = unsafe { GetMessageW(&mut msg, Some(HWND::default()), 0, 0) };

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
                let _ = TranslateMessage(&msg); // Translates virtual-key messages
                DispatchMessageW(&msg); // Dispatches message to the window procedure (wndproc)
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
