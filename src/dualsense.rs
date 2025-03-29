use hidapi::{BusType, DeviceInfo, HidApi};
use std::{
    ffi::CString,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};

#[allow(dead_code)]
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

#[derive(Debug)]
pub enum ControllerEvent {
    BatteryUpdate(BatteryReport),
    MuteBussonPressed,
}

struct ConnectedControllerState {
    device: hidapi::HidDevice,
    is_bluetooth: bool,
    previous_mute_state: bool,
    last_battery_poll: std::time::Instant,
    has_polled_for_first_time: bool,
}

pub fn setup_controller_polling() -> Result<Receiver<ControllerEvent>, String> {
    let (sender, receiver) = mpsc::channel::<ControllerEvent>();
    spawn_polling_thread(sender)?;
    Ok(receiver)
}

pub fn spawn_polling_thread(event_sender: Sender<ControllerEvent>) -> Result<(), String> {
    let builder = thread::Builder::new()
        .name("dualsense_poll".to_string())
        .spawn(move || {
            let api = HidApi::new().expect("Failed to initialize HID API");

            let mut current_device_state: Option<ConnectedControllerState> = None;

            loop {
                if current_device_state.is_none() {
                    if let Some((path, info)) = find_dualsense_device(&api) {
                        match api.open_path(&path) {
                            Ok(device) => {
                                let is_bluetooth = matches!(info.bus_type(), BusType::Bluetooth);
                                current_device_state = Some(ConnectedControllerState {
                                    device,
                                    is_bluetooth,
                                    previous_mute_state: false,
                                    last_battery_poll: std::time::Instant::now(),
                                    has_polled_for_first_time: false,
                                });
                            }
                            Err(e) => {
                                eprintln!("Failed to open device: {}", e);
                                thread::sleep(Duration::from_secs(2));
                            }
                        }
                    } else {
                        println!("No device found, waiting 5 seconds before retrying...");
                        thread::sleep(Duration::from_secs(5));
                        continue;
                    }
                }

                if let Some(mut state) = current_device_state.take() {
                    match poll_connected_device(&mut state, &event_sender) {
                        Ok(true) => {
                            current_device_state = Some(state);
                        }
                        Ok(false) => {
                            println!("Device disconnected");
                            current_device_state = None;
                        }
                        Err(PollError::ChannelClosed) => {
                            println!("Event channel closed, exiting polling thread");
                            break;
                        }
                    }
                }

                thread::sleep(Duration::from_millis(20));
            }
        })
        .map_err(|e| e.to_string());

    if builder.is_err() {
        return Err("Failed to spawn polling thread".to_string());
    }

    println!("Controller polling thread finished.");

    Ok(())
}

#[derive(Debug)]
enum PollError {
    ChannelClosed,
}

fn poll_connected_device(
    state: &mut ConnectedControllerState,
    event_sender: &Sender<ControllerEvent>,
) -> Result<bool, PollError> {
    let mut buf = [0u8; 64];
    match state.device.read_timeout(&mut buf, 20) {
        Ok(len) if len > 0 => {
            if !state.has_polled_for_first_time
                || state.last_battery_poll.elapsed() >= Duration::from_secs(10)
            {
                let battery_report = parse_battery(&buf, state.is_bluetooth);

                if event_sender
                    .send(ControllerEvent::BatteryUpdate(battery_report))
                    .is_err()
                {
                    eprintln!("Failed to send battery update event");
                    return Err(PollError::ChannelClosed);
                }

                state.last_battery_poll = std::time::Instant::now();
                state.has_polled_for_first_time = true;
            }

            let current_mute_state = mute_button_pressed(&buf, state.is_bluetooth);
            if current_mute_state && !state.previous_mute_state {
                println!("Mute button pressed");
                if event_sender
                    .send(ControllerEvent::MuteBussonPressed)
                    .is_err()
                {
                    eprintln!("Failed to send mute button event");
                    return Err(PollError::ChannelClosed);
                }
            }
            state.previous_mute_state = current_mute_state;
        }
        Ok(_) => {}
        Err(e) => {
            eprintln!("Failed to read from controller: {}", e);
            return Ok(false);
        }
    }

    Ok(true)
}

pub(crate) fn parse_battery(report: &[u8], is_bluetooth: bool) -> BatteryReport {
    if report.len() < 55 {
        return BatteryReport::new(0, false, false);
        // Avoid out-of-bounds access
    }
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

pub(crate) fn mute_button_pressed(report: &[u8], is_bluetooth: bool) -> bool {
    let mic_byte = if is_bluetooth { report[11] } else { report[10] };
    let mic_button_pressed = (mic_byte & 0x04) != 0;

    mic_button_pressed
}

fn find_dualsense_device(api: &HidApi) -> Option<(CString, DeviceInfo)> {
    for device_info in api.device_list() {
        if device_info.vendor_id() == 0x054C
            && (device_info.product_id() == 0x0CE6 || device_info.product_id() == 0x0DF2)
        {
            return Some((device_info.path().to_owned(), device_info.clone()));
        }
    }
    None
}
