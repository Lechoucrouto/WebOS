use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::ptr::{self, NonNull};
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

const HEAP_SIZE: usize = 16 * 1024 * 1024; // 16 MiB kernel heap

#[repr(align(16))]
struct HeapStorage(UnsafeCell<[u8; HEAP_SIZE]>);
unsafe impl Sync for HeapStorage {}

static HEAP: HeapStorage = HeapStorage(UnsafeCell::new([0; HEAP_SIZE]));
static USED_BYTES: AtomicUsize = AtomicUsize::new(0);

struct FreeNode {
    size: usize,
    next: Option<NonNull<FreeNode>>,
}

const fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}

struct AllocatorState {
    initialized: bool,
    head: Option<NonNull<FreeNode>>,
}

impl AllocatorState {
    const fn new() -> Self {
        Self {
            initialized: false,
            head: None,
        }
    }

    unsafe fn ensure_init(&mut self) {
        if !self.initialized {
            let base = HEAP.0.get() as *mut FreeNode;
            (*base).size = HEAP_SIZE;
            (*base).next = None;
            self.head = NonNull::new(base);
            self.initialized = true;
        }
    }

    unsafe fn alloc(&mut self, layout: Layout) -> *mut u8 {
        self.ensure_init();

        // Keep all allocations and free blocks aligned to at least 16 bytes
        let req_align = layout.align().max(16);
        let alloc_size = align_up(layout.size().max(16), 16);

        let mut prev: Option<NonNull<FreeNode>> = None;
        let mut curr = self.head;

        while let Some(curr_nn) = curr {
            let node_ptr = curr_nn.as_ptr();
            let region_start = node_ptr as usize;
            let region_size = (*node_ptr).size;
            let region_end = region_start + region_size;
            let next_node = (*node_ptr).next;

            let alloc_start = align_up(region_start, req_align);
            let Some(alloc_end) = alloc_start.checked_add(alloc_size) else {
                return ptr::null_mut();
            };

            if alloc_end <= region_end {
                let before_size = alloc_start - region_start;
                let after_size = region_end - alloc_end;

                // Keep any trailing free space >= 16 bytes
                let tail = if after_size >= 16 {
                    let after_ptr = alloc_end as *mut FreeNode;
                    (*after_ptr).size = after_size;
                    (*after_ptr).next = next_node;
                    NonNull::new(after_ptr)
                } else {
                    next_node
                };

                // Keep any leading alignment gap >= 16 bytes, or unlink this block
                if before_size >= 16 {
                    (*node_ptr).size = before_size;
                    (*node_ptr).next = tail;
                } else {
                    match prev {
                        Some(prev_nn) => (*prev_nn.as_ptr()).next = tail,
                        None => self.head = tail,
                    }
                }

                USED_BYTES.fetch_add(alloc_size, Ordering::Relaxed);
                return alloc_start as *mut u8;
            }

            prev = Some(curr_nn);
            curr = next_node;
        }

        ptr::null_mut()
    }

    unsafe fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {
        if ptr.is_null() {
            return;
        }

        let alloc_size = align_up(layout.size().max(16), 16);
        USED_BYTES.fetch_sub(alloc_size, Ordering::Relaxed);

        let freed_start = ptr as usize;
        let freed_end = freed_start + alloc_size;

        // Find sorted insertion position by memory address
        let mut prev: Option<NonNull<FreeNode>> = None;
        let mut curr = self.head;
        while let Some(curr_nn) = curr {
            if (curr_nn.as_ptr() as usize) > freed_start {
                break;
            }
            prev = Some(curr_nn);
            curr = (*curr_nn.as_ptr()).next;
        }

        let freed_ptr = freed_start as *mut FreeNode;
        (*freed_ptr).size = alloc_size;
        (*freed_ptr).next = curr;

        // Merge with the next free block if immediately adjacent
        if let Some(curr_nn) = curr {
            let curr_addr = curr_nn.as_ptr() as usize;
            if freed_end == curr_addr {
                (*freed_ptr).size += (*curr_nn.as_ptr()).size;
                (*freed_ptr).next = (*curr_nn.as_ptr()).next;
            }
        }

        // Link to previous free block and merge if immediately adjacent
        let freed_nn = NonNull::new_unchecked(freed_ptr);
        match prev {
            None => self.head = Some(freed_nn),
            Some(prev_nn) => {
                let prev_ptr = prev_nn.as_ptr();
                let prev_end = (prev_ptr as usize) + (*prev_ptr).size;
                if prev_end == freed_start {
                    (*prev_ptr).size += (*freed_ptr).size;
                    (*prev_ptr).next = (*freed_ptr).next;
                } else {
                    (*prev_ptr).next = Some(freed_nn);
                }
            }
        }
    }
}

pub struct LinkedListAllocator {
    lock: AtomicBool,
    state: UnsafeCell<AllocatorState>,
}

unsafe impl Sync for LinkedListAllocator {}

impl LinkedListAllocator {
    pub const fn new() -> Self {
        Self {
            lock: AtomicBool::new(false),
            state: UnsafeCell::new(AllocatorState::new()),
        }
    }

    fn with_lock<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut AllocatorState) -> R,
    {
        while self
            .lock
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        let result = unsafe { f(&mut *self.state.get()) };
        self.lock.store(false, Ordering::Release);
        result
    }
}

unsafe impl GlobalAlloc for LinkedListAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.with_lock(|state| state.alloc(layout))
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.with_lock(|state| state.dealloc(ptr, layout))
    }
}

pub fn heap_stats() -> (usize, usize) {
    (USED_BYTES.load(Ordering::Relaxed), HEAP_SIZE)
}