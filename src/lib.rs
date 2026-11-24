#![no_std]

extern crate alloc;

mod allocator;
mod font;
mod hal;
mod kernel;

use allocator::BumpAllocator;
use core::cell::UnsafeCell;
use core::panic::PanicInfo;
use hal::Hardware;
use kernel::{Kernel, FB_HEIGHT, FB_WIDTH};

#[global_allocator]
static ALLOCATOR: BumpAllocator = BumpAllocator;

extern "C" {
    fn js_now_ms() -> f64;
    fn js_console_log(ptr: *const u8, len: usize);
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