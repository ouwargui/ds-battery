use windows_numerics::Vector2;

use windows::Win32::Graphics::{
    Direct2D::{
        Common::{D2D_RECT_F, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT},
        D2D1_FEATURE_LEVEL_DEFAULT, D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT,
        D2D1_RENDER_TARGET_USAGE_NONE, D2D1_ROUNDED_RECT, ID2D1DeviceContext, ID2D1SolidColorBrush,
    },
    DirectWrite::{IDWriteFactory, IDWriteTextFormat, IDWriteTextLayout},
    Dxgi::{Common::DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_PRESENT, IDXGISwapChain1},
};

use crate::{WINDOW_HEIGHT, WINDOW_WIDTH, dualsense::BatteryReport};

pub fn draw_content(
    context: &ID2D1DeviceContext,
    dwrite_factory: &IDWriteFactory,
    text_format: &IDWriteTextFormat,
    swap_chain: &IDXGISwapChain1,
    corner_radius: f32,
    battery_status: BatteryReport, // battery percentage (0-100)
) {
    // --- Get Render Target ---
    let surface: windows::Win32::Graphics::Dxgi::IDXGISurface =
        unsafe { swap_chain.GetBuffer(0).expect("Failed to get buffer") };
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
    }; // Transparent
    let background_color = D2D1_COLOR_F {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.7,
    }; // Opaque Black bg
    let outline_color = D2D1_COLOR_F {
        r: 0.8,
        g: 0.8,
        b: 0.8,
        a: 1.0,
    }; // Light gray outline/text
    let fill_color = match battery_status.battery_capacity {
        0..=20 => rgba_to_d2d1_color_f(242, 27, 63, 255),
        21..=50 => rgba_to_d2d1_color_f(255, 198, 10, 255),
        _ => rgba_to_d2d1_color_f(43, 192, 22, 255),
    };

    // --- Create Brushes ---
    let bg_brush: ID2D1SolidColorBrush = unsafe {
        render_target
            .CreateSolidColorBrush(&background_color, None)
            .expect("Failed to create bg brush")
    };
    let outline_brush: ID2D1SolidColorBrush = unsafe {
        render_target
            .CreateSolidColorBrush(&outline_color, None)
            .expect("Failed to create outline brush")
    };
    let fill_brush: ID2D1SolidColorBrush = unsafe {
        render_target
            .CreateSolidColorBrush(&fill_color, None)
            .expect("Failed to create fill brush")
    };

    // --- Define Geometry ---
    let target_width = WINDOW_WIDTH as f32;
    let target_height = WINDOW_HEIGHT as f32;

    // Background Rounded Rect
    let bg_rect = D2D_RECT_F {
        left: 0.0,
        top: 0.0,
        right: target_width,
        bottom: target_height,
    };
    let bg_rounded_rect = D2D1_ROUNDED_RECT {
        rect: bg_rect,
        radiusX: corner_radius,
        radiusY: corner_radius,
    };

    let icon_height = target_height * 0.4; // Icon takes 40% of overlay height
    let icon_width = icon_height * 1.8;
    let icon_center_x = target_width / 2.0;
    let icon_top_y = target_height * 0.15; // Position icon 15% from the top
    let icon_bottom_y = icon_top_y + icon_height;
    let outline_thickness = 5.0;
    let battery_corner_radius = 4.0;

    // Battery Body
    let body_rect = D2D_RECT_F {
        left: icon_center_x - icon_width / 2.0,
        top: icon_top_y,
        right: icon_center_x + icon_width / 2.0,
        bottom: icon_bottom_y,
    };

    let body_rounded_rect = D2D1_ROUNDED_RECT {
        rect: body_rect,
        radiusX: battery_corner_radius,
        radiusY: battery_corner_radius,
    };

    // Battery Terminal
    let terminal_height = icon_height * 0.4;
    let terminal_width = icon_width * 0.1;
    let terminal_rect = D2D_RECT_F {
        left: body_rect.right,
        top: icon_top_y + (icon_height - terminal_height) / 2.0,
        right: body_rect.right + terminal_width,
        bottom: icon_top_y + (icon_height + terminal_height) / 2.0,
    };

    // Battery Fill Area
    // Inset by half the outline thickness to align with the inside edge of the stroke
    let inset = outline_thickness / 2.0;
    let fill_area_rect = D2D_RECT_F {
        left: body_rect.left + inset,
        top: body_rect.top + inset,
        right: body_rect.right - inset,
        bottom: body_rect.bottom - inset,
    };
    let max_fill_width = fill_area_rect.right - fill_area_rect.left;
    let fill_width = max_fill_width * (battery_status.battery_capacity as f32 / 100.0);
    // The actual rectangle to fill
    let fill_rect = D2D_RECT_F {
        left: fill_area_rect.left,
        top: fill_area_rect.top,
        right: fill_area_rect.left + fill_width, // Calculate end based on percentage
        bottom: fill_area_rect.bottom,
    };

    // Text Layout Area (below the icon)
    let text_top_y = icon_bottom_y + 5.0; // Space below icon
    let text_layout_rect = D2D_RECT_F {
        left: 0.0, // Allow text to center across the whole width
        top: text_top_y,
        right: target_width,
        bottom: target_height - 5.0, // Space at bottom
    };

    // --- Draw Commands ---
    unsafe {
        render_target.BeginDraw();
        render_target.Clear(Some(&clear_color)); // Clear transparent

        // 1. Draw Background
        render_target.FillRoundedRectangle(&bg_rounded_rect, &bg_brush);

        // Fill
        if fill_width > 0.0 {
            render_target.FillRectangle(&fill_rect, &fill_brush);
        }

        // 2. Draw Battery Icon
        // Outline
        render_target.DrawRoundedRectangle(
            &body_rounded_rect, // Use the rounded rect definition
            &outline_brush,
            outline_thickness,
            None, // No stroke style needed
        );
        render_target.FillRectangle(&terminal_rect, &outline_brush); // Solid terminal

        // 3. Draw Text
        let text = format!("{}%", battery_status.battery_capacity);
        let text_pcwstr = text.encode_utf16().collect::<Vec<u16>>(); // Convert to UTF-16

        // Create Text Layout
        let text_layout: IDWriteTextLayout = dwrite_factory
            .CreateTextLayout(
                &text_pcwstr,
                &*text_format,
                text_layout_rect.right - text_layout_rect.left, // Max width
                text_layout_rect.bottom - text_layout_rect.top, // Max height
            )
            .expect("Failed to create text layout");

        // Draw the layout using the outline brush color
        render_target.DrawTextLayout(
            Vector2 {
                X: text_layout_rect.left,
                Y: text_layout_rect.top,
            }, // Origin point
            &text_layout,
            &outline_brush, // Use outline brush for text color
            windows::Win32::Graphics::Direct2D::D2D1_DRAW_TEXT_OPTIONS_NONE,
        );

        render_target
            .EndDraw(None, None)
            .expect("Failed to end draw");

        let _ = swap_chain.Present(1, DXGI_PRESENT::default());
    }
}

#[inline]
pub fn rgba_to_d2d1_color_f(r: u8, g: u8, b: u8, a: u8) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: (r as f32) / 255.0,
        g: (g as f32) / 255.0,
        b: (b as f32) / 255.0,
        a: (a as f32) / 255.0,
    }
}
