// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use hidapi::{BusType, DeviceInfo, HidApi};
use std::{thread, time::Duration};
use tauri::{
    menu::{MenuBuilder, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.hide();
                #[cfg(target_os = "macos")]
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            }

            let show_i = MenuItem::with_id(app, "show", "Show window", true, None::<&str>)?;
            let menu = MenuBuilder::new(app).quit().items(&[&show_i]).build()?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("Dualsense battery checker")
                .menu(&menu)
                .on_tray_icon_event(|tray, event| match event {
                    TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } => {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    _ => {
                        println!("Unhandled event: {:?}", event);
                    }
                })
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        #[cfg(target_os = "macos")]
                        let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    _ => {
                        println!("Unhandled menu event: {:?}", event);
                    }
                })
                .build(app)?;

            start_controller_polling();

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn start_controller_polling() {
    thread::spawn(|| loop {
        let api = HidApi::new().expect("Failed to initialize HID API");
        let devices = api.device_list();
        for device in devices {
            if device.vendor_id() == 0x054C
                && (device.product_id() == 0x0CE6 || device.product_id() == 0x0DF2)
            {
                if let Ok(controller) = api.open(device.vendor_id(), device.product_id()) {
                    let mut buf = [0u8; 64];

                    if let Ok(len) = controller.read_timeout(&mut buf, 500) {
                        if len > 30 {
                            let battery_report = parse_battery(&buf, &device);
                            println!(
                                "Battery: {}% ({})",
                                battery_report.battery_capacity,
                                if battery_report.charging_status {
                                    "Charging"
                                } else {
                                    "Not Charging"
                                }
                            );
                        }
                    }
                }
            }
        }
        thread::sleep(Duration::from_secs(10));
    });
}

struct BatteryReport {
    battery_capacity: u8,
    charging_status: bool,
    is_healthy: bool,
}

fn parse_battery(report: &[u8], device: &DeviceInfo) -> BatteryReport {
    if report.len() < 55 {
        return BatteryReport {
            battery_capacity: 0,
            charging_status: false,
            is_healthy: false,
        }; // Avoid out-of-bounds access
    }

    // USB reports start at index 1, Bluetooth at index 2
    let is_bluetooth = matches!(device.bus_type(), BusType::Bluetooth);
    let status_byte = if is_bluetooth { report[54] } else { report[53] };

    let battery_data = status_byte & 0x0F; // Extracts the battery level (0-10)
    let charging_status = (status_byte & 0xF0) >> 4; // Extracts charging bits

    let battery_capacity = (battery_data * 10 + 5).min(100); // Convert 0-10 scale to 0-100%

    let (charging_status, is_healthy) = match charging_status {
        0x0 => (false, true),
        0x1 => (true, true),
        0x2 => (false, true),
        0xA | 0xB => (false, true),
        0xF => (false, false),
        _ => (false, false),
    };

    BatteryReport {
        battery_capacity,
        charging_status,
        is_healthy,
    }
}
