use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct TestAllocator;

thread_local! {
    static ALLOCATOR_CALLS: Cell<isize> = const { Cell::new(-1) };
}

fn record_allocator_call() {
    let _ = ALLOCATOR_CALLS.try_with(|calls| {
        let current = calls.get();
        if current >= 0 {
            calls.set(current + 1);
        }
    });
}

unsafe impl GlobalAlloc for TestAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_allocator_call();
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        record_allocator_call();
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_allocator_call();
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record_allocator_call();
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static TEST_ALLOCATOR: TestAllocator = TestAllocator;

struct AllocationScope;

impl Drop for AllocationScope {
    fn drop(&mut self) {
        ALLOCATOR_CALLS.with(|calls| calls.set(-1));
    }
}

pub fn count_allocator_calls<R>(callback: impl FnOnce() -> R) -> (R, usize) {
    ALLOCATOR_CALLS.with(|calls| {
        assert_eq!(calls.get(), -1);
        calls.set(0);
    });
    let scope = AllocationScope;
    let result = callback();
    let calls = ALLOCATOR_CALLS.with(|calls| calls.get() as usize);
    drop(scope);
    (result, calls)
}
