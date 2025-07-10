use std::cell::Cell;

thread_local! {
    static ALLOC_ENTRY: Cell<usize> = Cell::new(0);
}

pub(crate) struct AllocEntry(pub(crate) usize);

/// the alloc entry counter.
impl AllocEntry {
    pub(crate) fn new() -> Self {
        let entry = ALLOC_ENTRY.get();
        ALLOC_ENTRY.with(|allc_entry| allc_entry.set(entry + 1));
        Self(entry)
    }

    // the top of entry.
    pub(crate) fn top_entry(&self) -> bool {
        self.0 == 0
    }
}

impl Drop for AllocEntry {
    fn drop(&mut self) {
        ALLOC_ENTRY.with(|allc_entry| allc_entry.set(allc_entry.get() - 1));
    }
}
