use windows::{
    Win32::{
        Foundation::{HMODULE, HWND},
        Graphics::{
            Direct2D::{
                D2D1_DEBUG_LEVEL_INFORMATION, D2D1_DEBUG_LEVEL_NONE,
                D2D1_DEVICE_CONTEXT_OPTIONS_NONE, D2D1_FACTORY_OPTIONS,
                D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1CreateFactory, ID2D1DeviceContext,
                ID2D1Factory1,
            },
            Direct3D::D3D_DRIVER_TYPE_HARDWARE,
            Direct3D11::{
                D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice,
                ID3D11Device,
            },
            DirectComposition::{
                DCompositionCreateDevice, IDCompositionAnimation, IDCompositionDevice,
                IDCompositionEffectGroup, IDCompositionTarget,
            },
            DirectWrite::{
                DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
                DWRITE_TEXT_ALIGNMENT_CENTER, DWriteCreateFactory, IDWriteFactory,
                IDWriteTextFormat,
            },
            Dxgi::{
                Common::{
                    DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC,
                },
                DXGI_SCALING_STRETCH, DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
                DXGI_USAGE_RENDER_TARGET_OUTPUT, IDXGIDevice, IDXGIFactory2, IDXGISwapChain1,
            },
        },
    },
    core::w,
};

use windows::core::Interface;

pub fn create_d3d_device() -> (ID3D11Device, IDXGIDevice) {
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

pub fn create_dcomp_device(dxgi_device: &IDXGIDevice) -> IDCompositionDevice {
    unsafe { DCompositionCreateDevice(dxgi_device).expect("Failed to create DComp device") }
}

pub fn create_dcomp_target(dcomp_device: &IDCompositionDevice, hwnd: HWND) -> IDCompositionTarget {
    unsafe {
        dcomp_device
            .CreateTargetForHwnd(hwnd, true)
            .expect("Failed to create dcomp target")
    }
}

pub fn create_d2d_factory() -> ID2D1Factory1 {
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

pub fn create_d2d_device_context(
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

pub fn get_dxgi_factory(d3d_device: &ID3D11Device) -> IDXGIFactory2 {
    let dxgi_device: IDXGIDevice = d3d_device.cast().expect("Failed to cast to IDXGIDevice");
    let dxgi_adapter = unsafe { dxgi_device.GetAdapter().expect("Failed to get adapter") };
    unsafe {
        dxgi_adapter
            .GetParent()
            .expect("Failed to get dxgi adapter parent")
    }
}

pub fn create_composition_swapchain(
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

pub fn create_dwrite_factory() -> IDWriteFactory {
    unsafe {
        DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).expect("Failed to create dwrite factory")
    }
}

pub fn create_text_format(dwrite_factory: &IDWriteFactory) -> IDWriteTextFormat {
    unsafe {
        let tx_fmt = dwrite_factory
            .CreateTextFormat(
                w!("Segoe UI"),
                None,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                22.0,
                w!("en-us"),
            )
            .expect("Failed to create text format");

        tx_fmt
            .SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)
            .expect("Failed to set text alignment");
        tx_fmt
            .SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)
            .expect("Failed to set paragraph alignment");

        tx_fmt
    }
}

pub fn create_opacity_animation(
    dcomp_device: &IDCompositionDevice,
    duration_sec: f64,
    start_opacity: f32,
    end_opacity: f32,
) -> IDCompositionAnimation {
    if duration_sec <= 0.0 {
        eprintln!("Invalid duration for opacity animation: {}", duration_sec);
    }

    let linear_coefficient = (end_opacity - start_opacity) / (duration_sec as f32);

    unsafe {
        let animation = dcomp_device
            .CreateAnimation()
            .expect("Failed to create animation");

        animation
            .AddCubic(0.0, start_opacity, linear_coefficient, 0.0, 0.0)
            .expect("Failed to add cubic");

        animation
    }
}

pub fn apply_opacity(
    dcomp_device: &IDCompositionDevice,
    effect_group: &IDCompositionEffectGroup,
    opacity: f32,
) -> () {
    unsafe {
        let static_animation = dcomp_device
            .CreateAnimation()
            .expect("Failed to create animation");

        static_animation
            .AddCubic(0.0, opacity, 0.0, 0.0, 0.0)
            .expect("Failed to add cubic");

        effect_group
            .SetOpacity(Some(&static_animation))
            .expect("Failed to set visual opacity");
    }
}

pub fn apply_opacity_animation(
    effect_group: &IDCompositionEffectGroup,
    animation: &IDCompositionAnimation,
) -> () {
    unsafe {
        effect_group
            .SetOpacity(Some(animation))
            .expect("Failed to set opacity")
    }
}
