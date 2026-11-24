use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicUsize, Ordering};

const HEAP_SIZE: usize = 16 * 1024 * 1024; // 4 MiB kernel heap

#[repr(align(16))]
struct HeapStorage(UnsafeCell<[u8; HEAP_SIZE]>);
unsafe impl Sync for HeapStorage {}

static HEAP: HeapStorage = HeapStorage(UnsafeCell::new([0; HEAP_SIZE]));
static OFFSET: AtomicUsize = AtomicUsize::new(0);

pub struct BumpAllocator;

unsafe impl GlobalAlloc for BumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let align = layout.align();
        let size = layout.size();
        let mut current = OFFSET.load(Ordering::Relaxed);

        loop {
            let aligned = (current + align - 1) & !(align - 1);
            let next = match aligned.checked_add(size) {
                Some(n) if n <= HEAP_SIZE => n,
                _ => return core::ptr::null_mut(),
            };

            if OFFSET
                .compare_exchange_weak(current, next, Ordering::SeqCst, Ordering::Relaxed)
                .is_ok()
            {
                let base = HEAP.0.get() as *mut u8;
                return base.add(aligned);
            }
            current = OFFSET.load(Ordering::Relaxed);
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // Bump allocator does not reclaim individual allocations.
        // Swap with `talc` or `linked_list_allocator` for full deallocation.
    }
}

pub fn heap_stats() -> (usize, usize) {
    (OFFSET.load(Ordering::Relaxed), HEAP_SIZE)
}