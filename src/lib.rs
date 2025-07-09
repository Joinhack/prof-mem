#[cfg(feature = "msg")]
#[macro_use]
mod msg;

mod profiler;
mod record;
pub use crate::profiler::dump;
use crate::profiler::get_profiler;
use std::alloc::{GlobalAlloc, System};

use crate::record::AllocEntry;

pub struct ProfAlloc(pub usize);

unsafe impl GlobalAlloc for ProfAlloc {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        let alloc_record = AllocEntry.record();
        // if in the alloc to alloc the memory, we don't need anazly.
        if alloc_record.top_entry() {
            return ptr;
        }
        #[cfg(feature = "msg")]
        msg!("alloc_guard.0 {}", alloc_record.0);
        let profiler = get_profiler(self.0);
        profiler.insert(ptr, layout);
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        unsafe { System.dealloc(ptr, layout) };
        let profiler = get_profiler(self.0);
        profiler.remove(ptr);
    }
}
