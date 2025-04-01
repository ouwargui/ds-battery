//! Handles HID device discovery, polling loop, and event generation.

use crate::dualsense::{
    ConnectedControllerState, ControllerEvent, PRODUCT_ID_DUALSENSE, PRODUCT_ID_DUALSENSE_EDGE,
    VENDOR_ID_SONY, c_str_to_string, mute_button_pressed, parse_battery,
};
use hidapi::{BusType, HidApi, HidError};
use std::{
    collections::{HashMap, HashSet},
    ffi::{CStr, CString},
    sync::mpsc::{SendError, Sender},
    thread,
    time::{Duration, Instant},
};

// --- Constants ---
const DEVICE_SCAN_INTERVAL: Duration = Duration::from_secs(3);
const BATTERY_POLL_INTERVAL: Duration = Duration::from_secs(10);
const DEVICE_READ_TIMEOUT_MS: i32 = 20; // Short timeout for non-blocking reads
const POLLING_THREAD_SLEEP_MS: u64 = 20; // Main loop sleep duration

// --- Error Type ---

#[derive(Debug)]
enum PollError {
    Hid(HidError),
    Send(SendError<ControllerEvent>),
    ApiInitFailed,
    ThreadSpawnFailed,
}

impl From<HidError> for PollError {
    fn from(err: HidError) -> Self {
        PollError::Hid(err)
    }
}

impl From<SendError<ControllerEvent>> for PollError {
    fn from(err: SendError<ControllerEvent>) -> Self {
        PollError::Send(err)
    }
}

struct ControllerPollingManager {
    hid_api: HidApi,
    event_sender: Sender<ControllerEvent>,
    connected_devices: HashMap<CString, ConnectedControllerState>,
    last_scan_time: Instant,
}

impl ControllerPollingManager {
    fn new(event_sender: Sender<ControllerEvent>) -> Result<Self, PollError> {
        let hid_api = HidApi::new().map_err(|_| PollError::ApiInitFailed)?;
        Ok(Self {
            hid_api,
            event_sender,
            connected_devices: HashMap::new(),
            // Start scan immediately
            last_scan_time: Instant::now() - DEVICE_SCAN_INTERVAL,
        })
    }

    fn run_polling_loop(mut self) {
        loop {
            let now = Instant::now();

            if now.duration_since(self.last_scan_time) >= DEVICE_SCAN_INTERVAL {
                if let Err(e) = self.scan_for_device_changes() {
                    match e {
                        PollError::Send(_) => {
                            eprintln!(
                                "Polling Thread: Event channel closed during device scan. Exiting."
                            );
                            break;
                        }
                        PollError::Hid(err) => {
                            eprintln!("Polling Thread: HID error during device scan: {}", err);
                        }
                        _ => {}
                    }
                }
                self.last_scan_time = now;
            }

            if let Err(e) = self.poll_connected_devices() {
                match e {
                    PollError::Send(_) => {
                        eprintln!(
                            "Polling Thread: Event channel closed during device poll. Exiting."
                        );
                        break;
                    }
                    PollError::Hid(err) => {
                        eprintln!(
                            "Polling Thread: Unhandled HID error during poll loop: {}",
                            err
                        );
                    }
                    _ => {}
                }
            }

            thread::sleep(Duration::from_millis(POLLING_THREAD_SLEEP_MS));
        }
        println!("Controller polling thread finished.");
    }

    fn scan_for_device_changes(&mut self) -> Result<(), PollError> {
        self.hid_api.refresh_devices()?;
        let current_system_paths = self.find_dualsense_device_paths();

        let mut disconnected_paths = Vec::new();
        self.connected_devices.retain(|path: &CString, _state| {
            let still_connected = current_system_paths.contains(path);
            if !still_connected {
                disconnected_paths.push(path.clone());
            }
            still_connected
        });

        for path in disconnected_paths {
            let path_str = c_str_to_string(&path);
            println!("Polling Thread: Device disconnected: {}", path_str);
            self.event_sender
                .send(ControllerEvent::DeviceDisconnected(path_str))?;
        }

        for path in current_system_paths {
            if !self.connected_devices.contains_key(&path) {
                self.handle_new_connection(path)?;
            }
        }

        Ok(())
    }

    fn find_dualsense_device_paths(&self) -> HashSet<CString> {
        self.hid_api
            .device_list()
            .filter(|dev| {
                dev.vendor_id() == VENDOR_ID_SONY
                    && (dev.product_id() == PRODUCT_ID_DUALSENSE
                        || dev.product_id() == PRODUCT_ID_DUALSENSE_EDGE)
            })
            .map(|dev| dev.path().to_owned())
            .collect()
    }

    fn handle_new_connection(&mut self, path: CString) -> Result<(), PollError> {
        match self.hid_api.open_path(&path) {
            Ok(device) => {
                let path_str = c_str_to_string(&path);
                println!("Polling Thread: Device connected: {}", path_str);

                let info = device.get_device_info()?;
                let is_bluetooth = matches!(info.bus_type(), BusType::Bluetooth);

                device.set_blocking_mode(false)?;

                let state = ConnectedControllerState::new(device, is_bluetooth);
                self.connected_devices.insert(path.clone(), state);

                // Send connected event *after* adding to map
                self.event_sender
                    .send(ControllerEvent::DeviceConnected(path_str))?;

                // Perform an initial poll immediately if possible (best effort)
                if let Some(state_mut) = self.connected_devices.get_mut(&path) {
                    if let Err(e) = poll_single_device(&self.event_sender, &path, state_mut) {
                        eprintln!(
                            "Polling Thread: Error during initial poll for {}: {:?}",
                            c_str_to_string(&path),
                            e
                        );
                        // Don't remove the device yet, maybe it's temporary
                    }
                }
            }
            Err(e) => {
                eprintln!(
                    "Polling Thread: Failed to open new device {:?}: {}",
                    path, e
                );
                // Don't return error, just log and skip this device for now
            }
        }
        Ok(())
    }

    fn poll_connected_devices(&mut self) -> Result<(), PollError> {
        let mut failed_paths = HashSet::new();

        let sender = self.event_sender.clone();

        // Iterate mutably to update state
        for (path, state) in self.connected_devices.iter_mut() {
            if let Err(e) = poll_single_device(&sender, path, state) {
                match e {
                    PollError::Hid(HidError::HidApiError { message })
                        if message == "No data read from device" =>
                    {
                        // This is expected in non-blocking mode, ignore.
                    }
                    PollError::Hid(_) => {
                        // Treat other HID errors as potential disconnections
                        let path_str = c_str_to_string(path);
                        eprintln!(
                            "Polling Thread: HID error polling device {}: {:?}. Marking for removal.",
                            path_str, e
                        );
                        failed_paths.insert(path.clone());
                    }
                    PollError::Send(send_err) => {
                        // Channel closed, propagate the error up
                        return Err(PollError::Send(send_err));
                    }
                    _ => {
                        // Other error types shouldn't originate from poll_single_device
                        eprintln!(
                            "Polling Thread: Unexpected error polling device {}: {:?}",
                            c_str_to_string(path),
                            e
                        );
                    }
                }
            }
        }

        if !failed_paths.is_empty() {
            for path in failed_paths {
                if self.connected_devices.remove(&path).is_some() {
                    let path_str = c_str_to_string(&path);
                    println!(
                        "Polling Thread: Device disconnected (detected by poll failure): {}",
                        path_str
                    );
                    self.event_sender
                        .send(ControllerEvent::DeviceDisconnected(path_str))?;
                }
            }
        }

        Ok(())
    }
}

fn poll_single_device(
    sender: &Sender<ControllerEvent>,
    path: &CStr,
    state: &mut ConnectedControllerState,
) -> Result<(), PollError> {
    let mut buf = [0u8; 64]; // Standard HID report size for DualSense
    let bytes_read = state
        .device
        .read_timeout(&mut buf, DEVICE_READ_TIMEOUT_MS)?;

    if bytes_read == 0 {
        return Ok(()); // No new data available (expected in non-blocking)
    }

    let report = &buf[..bytes_read];
    let path_str = c_str_to_string(path);

    let now = Instant::now();

    if now.duration_since(state.last_battery_poll) >= BATTERY_POLL_INTERVAL {
        if let Some(battery_report) = parse_battery(report, state.is_bluetooth) {
            let changed = state.last_battery_report.as_ref() != Some(&battery_report);
            if changed {
                sender.send(ControllerEvent::BatteryUpdate(
                    path_str.clone(),
                    battery_report.clone(),
                ))?;
                state.last_battery_report = Some(battery_report);
            }
            state.last_battery_poll = now;
        } else {
            eprintln!("Polling Thread: Failed to parse battery for {}", path_str);
        }
    }

    if let Some(current_mute_state) = mute_button_pressed(report, state.is_bluetooth) {
        if current_mute_state && !state.previous_mute_state {
            println!("Mute button pressed on {}", path_str);
            sender.send(ControllerEvent::MuteButtonPressed(path_str.clone()))?;
        }
        state.previous_mute_state = current_mute_state;
    } else {
        eprintln!(
            "Polling Thread: Failed to parse mute button for {}",
            path_str
        );
    }

    Ok(())
}

pub fn setup_controller_polling() -> Result<std::sync::mpsc::Receiver<ControllerEvent>, String> {
    let (sender, receiver) = std::sync::mpsc::channel::<ControllerEvent>();
    spawn_polling_thread(sender).map_err(|e| format!("{:?}", e))?;
    Ok(receiver)
}

fn spawn_polling_thread(event_sender: Sender<ControllerEvent>) -> Result<(), PollError> {
    let manager = ControllerPollingManager::new(event_sender)?;

    thread::Builder::new()
        .name("dualsense_poll".to_string())
        .spawn(move || {
            manager.run_polling_loop();
        })
        .map_err(|_| PollError::ThreadSpawnFailed)?;

    Ok(())
}
