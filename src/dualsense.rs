use hidapi::{BusType, DeviceInfo, HidApi};
use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};

#[derive(Clone, Debug)]
pub struct BatteryReport {
    pub battery_capacity: u8,
    pub charging_status: bool,
    pub is_healthy: bool,
}

impl BatteryReport {
    pub fn new(battery_capacity: u8, charging_status: bool, is_healthy: bool) -> Self {
        Self {
            battery_capacity,
            charging_status,
            is_healthy,
        }
    }
}

pub fn setup_controller_polling() -> Result<Receiver<BatteryReport>, String> {
    let (sender, receiver) = mpsc::channel::<BatteryReport>();
    spawn_polling_thread(sender)?;
    Ok(receiver)
}

pub fn spawn_polling_thread(battery_sender: Sender<BatteryReport>) -> Result<(), String> {
    let builder = thread::Builder::new()
        .name("dualsense_poll".to_string())
        .spawn(move || {
            let mut api = HidApi::new().expect("Failed to initialize HID API");

            loop {
                api.refresh_devices().expect("Failed to refresh devices");
                let devices = api.device_list();

                for device in devices {
                    if device.vendor_id() == 0x054C
                        && (device.product_id() == 0x0CE6 || device.product_id() == 0x0DF2)
                    {
                        if let Ok(controller) = api.open(device.vendor_id(), device.product_id()) {
                            let mut buf = [0u8; 64];

                            if let Ok(len) = controller.read_timeout(&mut buf, 500) {
                                if len > 30 {
                                    if battery_sender.send(parse_battery(&buf, &device)).is_err() {
                                        eprintln!("Failed to send battery report");
                                        break;
                                    };
                                }
                            }
                        }
                    }
                }
                thread::sleep(Duration::from_secs(10));
            }
        })
        .map_err(|e| e.to_string());

    if builder.is_err() {
        return Err("Failed to spawn polling thread".to_string());
    }

    Ok(())
}

pub(crate) fn parse_battery(report: &[u8], device: &DeviceInfo) -> BatteryReport {
    if report.len() < 55 {
        return BatteryReport::new(0, false, false);
        // Avoid out-of-bounds access
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

    BatteryReport::new(battery_capacity, charging_status, is_healthy)
}
