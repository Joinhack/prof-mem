use std::{
    cell::Cell,
    ptr,
    sync::{Mutex, MutexGuard, Once},
};

thread_local! {
    static ALLOC_ENTRYS: Cell<usize> = Cell::new(0);
}

pub(crate) struct AllocEntry;

pub(crate) struct AllocRecord(usize, Option<MutexGuard<'static, ()>>);

impl AllocRecord {
    pub(crate) fn top_entry(&self) -> bool {
        self.0 == 1
    }
}

impl AllocEntry {
    pub(crate) fn record(&self) -> AllocRecord {
        static mut MUTEX: *mut Mutex<()> = ptr::null_mut();
        static ONCE: Once = Once::new();
        let current = ALLOC_ENTRYS.get();
        if current > 0 {
            ALLOC_ENTRYS.set(current + 1);
            return AllocRecord(current, None);
        }
        ONCE.call_once(|| unsafe {
            let mutex = Mutex::new(());
            MUTEX = Box::into_raw(Box::new(mutex));
        });
        unsafe {
            ALLOC_ENTRYS.set(current + 1);
            let guard = Some((*MUTEX).lock().unwrap());
            AllocRecord(current, guard)
        }
    }
}

impl Drop for AllocRecord {
    fn drop(&mut self) {
        if self.1.is_some() {
            ALLOC_ENTRYS.with(|s| s.set(s.get() - 1));
        }
    }
}
