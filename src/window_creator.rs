use crate::{AppState, WINDOW_HEIGHT, WINDOW_WIDTH, window::wndproc};
use windows::{
    Win32::{
        Foundation::{GetLastError, HINSTANCE, HWND},
        Graphics::Gdi::HBRUSH,
        UI::WindowsAndMessaging::{
            CS_HREDRAW, CS_OWNDC, CS_VREDRAW, CreateWindowExW, GWLP_USERDATA, GetSystemMetrics,
            HICON, IDC_ARROW, LoadCursorW, RegisterClassExW, SM_CXSCREEN, SM_CYSCREEN,
            SetWindowLongPtrW, WNDCLASSEXW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
            WS_POPUP,
        },
    },
    core::{Error, HRESULT, PCWSTR, w},
};

const OVERLAY_WINDOW_CLASS_NAME: PCWSTR = w!("overlay_window_class");

pub struct WindowCreator {
    hinstance: HINSTANCE,
}

impl WindowCreator {
    pub fn new(hinstance: HINSTANCE) -> Self {
        Self { hinstance }
    }

    pub fn create_overlay_window(&self) -> Result<HWND, Error> {
        self.register_window_class()?;
        let hwnd = self.create_window_instance()?;
        Ok(hwnd)
    }

    fn register_window_class(&self) -> Result<u16, Error> {
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW | CS_OWNDC,
            cbClsExtra: 0,
            cbWndExtra: std::mem::size_of::<*mut AppState>() as i32, // Reserve space
            hInstance: self.hinstance,
            hIcon: HICON::default(),
            hCursor: unsafe { LoadCursorW(None, IDC_ARROW)? },
            hbrBackground: HBRUSH::default(),
            lpszMenuName: PCWSTR::null(),
            lpszClassName: OVERLAY_WINDOW_CLASS_NAME,
            hIconSm: HICON::default(),
            lpfnWndProc: Some(wndproc),
        };

        let atom = unsafe { RegisterClassExW(&wc) };
        if atom == 0 {
            let error = unsafe { GetLastError() };
            Err(Error::new(
                HRESULT::from(error),
                "Failed to register window class.",
            ))
        } else {
            Ok(atom)
        }
    }

    fn calculate_window_position() -> (i32, i32) {
        let screen_width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
        let screen_height = unsafe { GetSystemMetrics(SM_CYSCREEN) };

        let x = (screen_width - WINDOW_WIDTH) / 2;
        let y = (screen_height * 4) / 5 - (WINDOW_HEIGHT / 2);

        (x.max(0), y.max(0))
    }

    fn create_window_instance(&self) -> Result<HWND, Error> {
        let (x, y) = Self::calculate_window_position();

        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW,
                OVERLAY_WINDOW_CLASS_NAME,
                w!("DS Battery overlay"),
                WS_POPUP,
                x,
                y,
                WINDOW_WIDTH,
                WINDOW_HEIGHT,
                None,
                None,
                Some(self.hinstance),
                None, // Pass AppState pointer here during creation? No, use SetWindowLongPtrW later
            )
        };

        if let Err(e) = hwnd {
            eprintln!("Failed to create window: {:?}", e);
            let error = unsafe { GetLastError() };
            Err(Error::new(HRESULT::from(error), "Failed to create window"))
        } else {
            Ok(hwnd.unwrap())
        }
    }

    pub fn associate_appstate_with_hwnd(&self, hwnd: HWND, app_state: &mut AppState) {
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, app_state as *mut _ as isize);
        }
    }
}
