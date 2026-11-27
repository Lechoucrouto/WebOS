pub mod exec;
pub mod fs;
pub mod gui;
pub mod hal;
pub mod kernel;

use hal::Hardware;
use kernel::Kernel;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use web_sys::window;

struct WebHardware;

impl Hardware for WebHardware {
    fn now_ms(&self) -> f64 {
        window()
            .and_then(|w| w.performance())
            .map(|p| p.now())
            .unwrap_or(0.0)
    }

    fn serial_log(&self, msg: &str) {
        web_sys::console::log_1(&JsValue::from_str(msg));
    }

    fn storage_save(&self, data: &str) {
        if let Some(Ok(Some(storage))) = window().map(|w| w.local_storage()) {
            let _ = storage.set_item("webos_fs_v1", data);
        }
    }

    fn storage_load(&self) -> Option<String> {
        window()
            .and_then(|w| w.local_storage().ok().flatten())
            .and_then(|s| s.get_item("webos_fs_v1").ok().flatten())
    }

    fn factory_reset(&self) {
        if let Some(win) = window() {
            if let Ok(Some(storage)) = win.local_storage() {
                let _ = storage.remove_item("webos_fs_v1");
            }
            let _ = win.location().reload();
        }
    }
}

#[wasm_bindgen]
pub struct WebOsHandle {
    kernel: Rc<RefCell<Kernel>>,
}

#[wasm_bindgen]
impl WebOsHandle {
    pub fn on_key(&self, code: u32) {
        let hw = WebHardware;
        self.kernel.borrow_mut().on_key(code, &hw);
    }

    pub fn on_mouse_move(&self, x: i32, y: i32) {
        self.kernel.borrow_mut().on_mouse_move(x, y);
    }

    pub fn on_mouse_down(&self, x: i32, y: i32, button: u32) {
        let hw = WebHardware;
        self.kernel.borrow_mut().on_mouse_down(x, y, button, &hw);
    }

    pub fn on_mouse_up(&self, x: i32, y: i32, button: u32) {
        self.kernel.borrow_mut().on_mouse_up(x, y, button);
    }

    pub fn on_wheel(&self, dx: i32, dy: i32) {
        self.kernel.borrow_mut().on_wheel(dx, dy);
    }

    pub fn tick(&self) {
        let hw = WebHardware;
        self.kernel.borrow_mut().tick(&hw);
    }

    pub fn framebuffer_ptr(&self) -> *const u32 {
        self.kernel.borrow().framebuffer().as_ptr()
    }

    pub fn framebuffer_len(&self) -> usize {
        self.kernel.borrow().framebuffer().len()
    }

    pub fn width(&self) -> usize {
        gui::FB_WIDTH
    }

    pub fn height(&self) -> usize {
        gui::FB_HEIGHT
    }
}

#[wasm_bindgen]
pub fn boot_webos() -> WebOsHandle {
    let hw = WebHardware;
    let kernel = Rc::new(RefCell::new(Kernel::boot(&hw)));
    WebOsHandle { kernel }
}