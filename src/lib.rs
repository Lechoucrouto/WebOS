#![no_std]

extern crate alloc;

mod allocator;
mod exec;
mod font;
mod fs;
mod gui;
mod hal;
mod kernel;

use alloc::vec;
use alloc::vec::Vec;
use allocator::LinkedListAllocator;
use core::cell::UnsafeCell;
use core::panic::PanicInfo;
use gui::{FB_HEIGHT, FB_WIDTH};
use hal::Hardware;
use kernel::Kernel;

#[global_allocator]
static ALLOCATOR: LinkedListAllocator = LinkedListAllocator::new();

#[link(wasm_import_module = "env")]
extern "C" {
    fn js_now_ms() -> f64;
    fn js_console_log(ptr: *const u8, len: usize);
    fn js_storage_save(ptr: *const u8, len: usize);
    fn js_storage_len() -> usize;
    fn js_storage_read(ptr: *mut u8, max_len: usize) -> usize;
    fn js_factory_reset();
}

struct BrowserHal;

impl Hardware for BrowserHal {
    fn uptime_ms(&self) -> u64 {
        unsafe { js_now_ms() as u64 }
    }

    fn serial_log(&self, msg: &str) {
        unsafe {
            js_console_log(msg.as_ptr(), msg.len());
        }
    }

    fn storage_save(&self, data: &[u8]) {
        unsafe {
            js_storage_save(data.as_ptr(), data.len());
        }
    }

    fn storage_load(&self) -> Option<Vec<u8>> {
        unsafe {
            let len = js_storage_len();
            if len == 0 {
                return None;
            }
            let mut buf = vec![0u8; len];
            let read = js_storage_read(buf.as_mut_ptr(), len);
            buf.truncate(read);
            Some(buf)
        }
    }

    fn factory_reset(&self) {
        unsafe {
            js_factory_reset();
        }
    }
}

struct KernelCell(UnsafeCell<Option<Kernel>>);
unsafe impl Sync for KernelCell {}

static KERNEL: KernelCell = KernelCell(UnsafeCell::new(None));

fn with_kernel<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut Kernel) -> R,
{
    unsafe { (*KERNEL.0.get()).as_mut().map(f) }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let msg = alloc::format!("KERNEL PANIC: {}", info);
    BrowserHal.serial_log(&msg);
    core::arch::wasm32::unreachable()
}

#[no_mangle]
pub extern "C" fn kernel_boot() {
    let k = Kernel::boot(&BrowserHal);
    unsafe {
        *KERNEL.0.get() = Some(k);
    }
}

#[no_mangle]
pub extern "C" fn kernel_key_input(code: u32) {
    with_kernel(|k| k.on_key(code, &BrowserHal));
}

#[no_mangle]
pub extern "C" fn kernel_mouse_move(x: i32, y: i32) {
    with_kernel(|k| k.on_mouse_move(x, y));
}

#[no_mangle]
pub extern "C" fn kernel_mouse_down(x: i32, y: i32, button: u32) {
    with_kernel(|k| k.on_mouse_down(x, y, button, &BrowserHal));
}

#[no_mangle]
pub extern "C" fn kernel_mouse_up(x: i32, y: i32, button: u32) {
    with_kernel(|k| k.on_mouse_up(x, y, button));
}

#[no_mangle]
pub extern "C" fn kernel_tick() {
    with_kernel(|k| k.render(&BrowserHal));
}

#[no_mangle]
pub extern "C" fn fb_ptr() -> *const u32 {
    with_kernel(|k| k.framebuffer_ptr()).unwrap_or(core::ptr::null())
}

#[no_mangle]
pub extern "C" fn fb_width() -> usize {
    FB_WIDTH
}

#[no_mangle]
pub extern "C" fn fb_height() -> usize {
    FB_HEIGHT
}
