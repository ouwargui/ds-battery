use std::ffi::CStr;
use std::time::Instant;

pub(crate) const VENDOR_ID_SONY: u16 = 0x054C;
pub(crate) const PRODUCT_ID_DUALSENSE: u16 = 0x0CE6;
pub(crate) const PRODUCT_ID_DUALSENSE_EDGE: u16 = 0x0DF2;

const _USB_INPUT_REPORT_ID: u8 = 0x01;
const _BLUETOOTH_INPUT_REPORT_ID: u8 = 0x31;

const USB_MIC_MUTE_BYTE_INDEX: usize = 10;
const BLUETOOTH_MIC_MUTE_BYTE_INDEX: usize = 11;
const MIC_MUTE_BUTTON_MASK: u8 = 0x04;

const BLUETOOTH_BATTERY_BYTE_INDEX: usize = 54;
const USB_BATTERY_BYTE_INDEX: usize = 53;
const BATTERY_LEVEL_MASK: u8 = 0x0F;
const BATTERY_STATUS_MASK: u8 = 0xF0;
const BATTERY_STATUS_SHIFT: u8 = 4;

const BATTERY_STATUS_DISCHARGING: u8 = 0x00;
const BATTERY_STATUS_CHARGING: u8 = 0x01;
const BATTERY_STATUS_FULL: u8 = 0x02;
const BATTERY_STATUS_VOLTAGE_ERROR: u8 = 0x0A;
const BATTERY_STATUS_TEMPERATURE_ERROR: u8 = 0x0B;
const _BATTERY_STATUS_CHARGING_ERROR: u8 = 0x0F;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BatteryStatus {
    Discharging,
    Charging,
    Full,
    ChargingError,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BatteryReport {
    pub battery_capacity: u8, // Battery level (0-100)
    pub battery_status: BatteryStatus,
}

impl BatteryReport {
    pub fn new(battery_capacity: u8, battery_status: BatteryStatus) -> Self {
        Self {
            battery_capacity,
            battery_status,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ControllerEvent {
    DeviceConnected(String),
    DeviceDisconnected(String),
    BatteryUpdate(String, BatteryReport),
    MuteButtonPressed(String),
}

pub(crate) struct ConnectedControllerState {
    pub device: hidapi::HidDevice,
    pub is_bluetooth: bool,
    pub previous_mute_state: bool,
    pub last_battery_poll: Instant,
    pub last_battery_report: Option<BatteryReport>,
}

impl ConnectedControllerState {
    pub fn new(device: hidapi::HidDevice, is_bluetooth: bool) -> Self {
        Self {
            device,
            is_bluetooth,
            previous_mute_state: false,
            last_battery_poll: Instant::now() - std::time::Duration::from_secs(1000),
            last_battery_report: None,
        }
    }
}

pub(crate) fn parse_battery(report: &[u8], is_bluetooth: bool) -> Option<BatteryReport> {
    let battery_byte_index = if is_bluetooth {
        BLUETOOTH_BATTERY_BYTE_INDEX
    } else {
        USB_BATTERY_BYTE_INDEX
    };

    if report.len() <= battery_byte_index {
        return None; // Report too short
    }

    let status_byte = report[battery_byte_index];
    let battery_level_raw = status_byte & BATTERY_LEVEL_MASK; // 0-10
    let charging_status_raw = (status_byte & BATTERY_STATUS_MASK) >> BATTERY_STATUS_SHIFT;

    let (battery_capacity, battery_status) = match charging_status_raw {
        BATTERY_STATUS_DISCHARGING => (
            (battery_level_raw * 10).saturating_add(5).min(100),
            BatteryStatus::Discharging,
        ),
        BATTERY_STATUS_CHARGING => (
            (battery_level_raw * 10).saturating_add(5).min(100),
            BatteryStatus::Charging,
        ),
        BATTERY_STATUS_FULL => (100, BatteryStatus::Full),
        BATTERY_STATUS_VOLTAGE_ERROR..=BATTERY_STATUS_TEMPERATURE_ERROR => {
            (0, BatteryStatus::ChargingError)
        }
        _ => (0, BatteryStatus::Unknown),
    };

    Some(BatteryReport::new(battery_capacity, battery_status))
}

/// Checks if the mute button is currently pressed based on an HID input report.
pub(crate) fn mute_button_pressed(report: &[u8], is_bluetooth: bool) -> Option<bool> {
    let mic_byte_index = if is_bluetooth {
        BLUETOOTH_MIC_MUTE_BYTE_INDEX
    } else {
        USB_MIC_MUTE_BYTE_INDEX
    };

    if report.len() <= mic_byte_index {
        return None; // Report too short
    }

    let is_pressed = (report[mic_byte_index] & MIC_MUTE_BUTTON_MASK) != 0;

    Some(is_pressed)
}

/// Helper to convert CStr to String, handling potential errors.
pub(crate) fn c_str_to_string(c_str: &CStr) -> String {
    c_str.to_str().unwrap_or("<invalid UTF-8 path>").to_string()
}
