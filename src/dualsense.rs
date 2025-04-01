use hidapi::{BusType, HidApi};
use std::{
    collections::{HashMap, HashSet},
    ffi::{CStr, CString},
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant},
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
    BatteryUpdate(String, BatteryReport),
    MuteBussonPressed(String),
    DeviceConnected(String),
    DeviceDisconnected(String),
}

struct ConnectedControllerState {
    device: hidapi::HidDevice,
    is_bluetooth: bool,
    previous_mute_state: bool,
    last_battery_poll: std::time::Instant,
    has_polled_for_first_time: bool,
}

#[derive(Debug)]
enum PollError {
    ChannelClosed,
    HidError(hidapi::HidError),
}

impl From<hidapi::HidError> for PollError {
    fn from(err: hidapi::HidError) -> Self {
        PollError::HidError(err)
    }
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
            let mut api = HidApi::new().expect("Failed to initialize HID API");

            let mut connected_devices: HashMap<CString, ConnectedControllerState> = HashMap::new();
            let mut last_scan = Instant::now();
            let mut needs_first_scan = true;
            let scan_interval = Duration::from_secs(3);

            loop {
                let now = Instant::now();
                let mut channel_closed = false;

                if needs_first_scan || now.duration_since(last_scan) >= scan_interval {
                    needs_first_scan = false;
                    let current_system_paths = find_dualsense_devices(&mut api);

                    connected_devices.retain(|path, _state| {
                        let still_connected = current_system_paths.contains(path);
                        if !still_connected {
                            if let Ok(path_str) = path.to_str().map(String::from) {
                                if event_sender
                                    .send(ControllerEvent::DeviceDisconnected(path_str))
                                    .is_err()
                                {
                                    channel_closed = true;
                                }
                            }
                        }
                        still_connected
                    });

                    if channel_closed {
                        eprintln!("Polling Thread: Channel closed, exiting.");
                        break;
                    }

                    for path in &current_system_paths {
                        if !connected_devices.contains_key(path) {
                            match api.open_path(path) {
                                Ok(device) => {
                                    let info = device.get_device_info().unwrap();
                                    let is_bluetooth =
                                        matches!(info.bus_type(), BusType::Bluetooth);
                                    let mut state = ConnectedControllerState {
                                        device,
                                        is_bluetooth,
                                        previous_mute_state: false,
                                        last_battery_poll: Instant::now(),
                                        has_polled_for_first_time: false,
                                    };

                                    let mut buf = [0u8; 64];
                                    match state.device.read_timeout(&mut buf, 20) {
                                        Ok(len) if len > 0 => {
                                            if let Err(e) = poll_device_battery(&buf, &mut state, &event_sender, path) {
                                                match e {
                                                    PollError::ChannelClosed => {
                                                        channel_closed = true;
                                                        eprintln!("Polling Thread: Channel closed, exiting.");
                                                        break;
                                                    }
                                                    PollError::HidError(e) => {
                                                        eprintln!("Polling Thread: Failed to read from device {:?}: {:?}", path, e);
                                                    }
                                                }
                                            }
                                        }
                                        Ok(_) => {}
                                        Err(e) => {
                                            eprintln!("Failed to poll for the first time - controller: {}", e);
                                        }
                                    }

                                    connected_devices.insert(path.clone(), state);

                                    if let Ok(path_str) = path.to_str().map(String::from) {
                                        if event_sender
                                            .send(ControllerEvent::DeviceConnected(path_str))
                                            .is_err()
                                        {
                                            channel_closed = true;
                                            eprintln!("Polling Thread: Channel closed, exiting.");
                                            break;
                                        }
                                    }
                                }
                                Err(e) => eprintln!(
                                    "Polling Thread: Failed to open new device {:?}: {}",
                                    path, e
                                ),
                            }
                        }
                    }
                    last_scan = now;
                }

                if channel_closed {
                    eprintln!("Polling Thread: Channel closed, exiting.");
                    break;
                }

                let mut poll_failed_paths = HashSet::new();

                for (path, state) in connected_devices.iter_mut() {
                    let mut device_poll_failed = false;

                    let mut buf = [0u8; 64];
                    match state.device.read_timeout(&mut buf, 20) {
                        Ok(len) if len > 0 => {
                            if let Err(e) = poll_device_battery(&buf, state, &event_sender, path) {
                                match e {
                                    PollError::ChannelClosed => {
                                        channel_closed = true;
                                        eprintln!("Polling Thread: Channel closed, exiting.");
                                        break;
                                    }
                                    PollError::HidError(e) => {
                                        eprintln!("Polling Thread: Failed to read from device {:?}: {:?}", path, e);
                                        device_poll_failed = true;
                                    }
                                }
                            }

                            if !device_poll_failed && !channel_closed {
                                if let Err(e) = poll_device_buttons(&buf, state, &event_sender, path) {
                                    match e {
                                        PollError::ChannelClosed => {
                                            channel_closed = true;
                                            eprintln!("Polling Thread: Channel closed, exiting.");
                                            break;
                                        }
                                        PollError::HidError(e) => {
                                            eprintln!("Polling Thread: Failed to read from device {:?}: {:?}", path, e);
                                            device_poll_failed = true;
                                        }
                                    }
                                }
                            }

                            if device_poll_failed {
                                poll_failed_paths.insert(path.clone());
                            }
                        }
                        Ok(_) => {}
                        Err(e) => {
                            eprintln!("Polling Thread: Failed to read from device {:?}: {}", path, e);
                        }
                    }
                }

                if channel_closed {
                    eprintln!("Polling Thread: Channel closed, exiting.");
                    break;
                }

                if !poll_failed_paths.is_empty() {
                    connected_devices.retain(|path, _state| {
                        let failed = poll_failed_paths.contains(path);
                        if failed {
                            println!("Polling Thread: Device disconnected (detected by poll failure): {:?}", path);
                            if let Ok(path_str) = path.to_str().map(String::from) {
                                if event_sender.send(ControllerEvent::DeviceDisconnected(path_str)).is_err() {
                                    channel_closed = true;
                                }
                            }
                        }
                        !failed 
                    });
                }

                if channel_closed {
                    eprintln!("Polling Thread: Channel closed, exiting.");
                    break;
                }

                thread::sleep(Duration::from_millis(20));
            }
        });

    if builder.is_err() {
        return Err("Failed to spawn polling thread".to_string());
    }

    println!("Controller polling thread finished.");

    Ok(())
}

fn poll_device_buttons(
    buf: &[u8; 64],
    state: &mut ConnectedControllerState,
    event_sender: &Sender<ControllerEvent>,
    device_path: &CStr,
) -> Result<(), PollError> {
    let current_mute_state = mute_button_pressed(buf, state.is_bluetooth);
    let path_str = device_path.to_str().unwrap_or("invalid_path").to_string();
    if current_mute_state && !state.previous_mute_state {
        println!("Mute button pressed");
        if event_sender
            .send(ControllerEvent::MuteBussonPressed(path_str))
            .is_err()
        {
            eprintln!("Failed to send mute button event");
            return Err(PollError::ChannelClosed);
        }
    }
    state.previous_mute_state = current_mute_state;
    Ok(())
}

fn poll_device_battery(
    buf: &[u8; 64],
    state: &mut ConnectedControllerState,
    event_sender: &Sender<ControllerEvent>,
    device_path: &CStr,
) -> Result<(), PollError> {
    if !state.has_polled_for_first_time
        || state.last_battery_poll.elapsed() >= Duration::from_secs(10)
    {
        let battery_report = parse_battery(buf, state.is_bluetooth);
        let path_str = device_path.to_str().unwrap_or("invalid_path").to_string();

        if event_sender
            .send(ControllerEvent::BatteryUpdate(path_str, battery_report))
            .is_err()
        {
            eprintln!("Failed to send battery update event");
            return Err(PollError::ChannelClosed);
        }

        state.last_battery_poll = Instant::now();
        state.has_polled_for_first_time = true;
    }

    Ok(())
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

fn find_dualsense_devices(api: &mut HidApi) -> HashSet<CString> {
    api.refresh_devices().unwrap();

    api.device_list()
        .filter(|device| {
            device.vendor_id() == 0x054C
                && (device.product_id() == 0x0CE6 || device.product_id() == 0x0DF2)
        })
        .map(|device| device.path().to_owned())
        .collect::<HashSet<CString>>()
}
