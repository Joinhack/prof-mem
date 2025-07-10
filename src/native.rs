use std::os::raw::c_int;

unsafe extern "C" {
    fn alloc_entry_increase() -> c_int;

    fn alloc_entry_decrease() -> c_int;
}

pub(crate) struct AllocEntry(pub(crate) usize);

impl AllocEntry {
    pub(crate) fn new() -> Self {
        Self(unsafe { alloc_entry_increase() as _ })
    }

    pub(crate) fn top_entry(&self) -> bool {
        self.0 == 1
    }
}

impl Drop for AllocEntry {
    fn drop(&mut self) {
        unsafe {
            alloc_entry_decrease();
        }
    }
}
