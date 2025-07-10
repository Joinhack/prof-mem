use std::{
    alloc::Layout,
    cell::{Cell, UnsafeCell},
    collections::HashMap,
    ffi::c_void,
    io::{self, Write},
    mem::MaybeUninit,
    sync::{Mutex, Once},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{entry::AllocEntry, profile_proto::ProfileProtoWriter};

thread_local! {
    static LOCKED:Cell<bool> = Cell::new(false);
}

pub(crate) struct LockGuard<'a>(Option<std::sync::MutexGuard<'a, ()>>);

impl<'a> Drop for LockGuard<'a> {
    fn drop(&mut self) {
        // if locked, set the lock flag.
        if self.0.is_some() {
            LOCKED.with(|locked| {
                assert!(locked.get());
                locked.set(false);
            })
        }
    }
}

/// alloc frame
struct AllocFrames {
    size: usize,
    frames: Vec<*mut c_void>,
}

pub(crate) struct AllocSymbolFrames {
    pub(crate) ptr: *const u8,
    pub(crate) size: usize,
    pub(crate) frames: Vec<Symbol>,
}

pub(crate) struct Symbol {
    pub(crate) file_name: String,
    pub(crate) line_no: u32,
    #[allow(dead_code)]
    pub(crate) col_no: u32,
    pub(crate) name: String,
    pub(crate) addr: *mut c_void,
}

pub(crate) struct HeapProfiler {
    max_deep: Cell<usize>,
    init_once: Once,
    frames: UnsafeCell<MaybeUninit<HashMap<*const u8, AllocFrames>>>,
}

impl HeapProfiler {
    #[inline]
    fn init_once(&self, max_deep: Option<usize>) {
        self.init_once.call_once(|| {
            unsafe {
                self.max_deep.set(max_deep.unwrap());
                (&mut *self.frames.get()).write(HashMap::new());
            };
        });
    }

    /// Try to acquire the lock. If it is not available, wait until another thread releases it.
    /// Acquire a  global re-entrant lock over
    /// this lock can be acquired as many times as you want on a single thread without deadlocking, allowing one thread
    #[inline(always)]
    pub(crate) fn lock(&self) -> LockGuard<'_> {
        static MUTEX: Mutex<()> = Mutex::new(());
        // if the local thread already aquired the lock, just return.
        if LOCKED.get() {
            return LockGuard(None);
        }
        LOCKED.with(|locked| locked.set(true));
        LockGuard(Some(MUTEX.lock().unwrap()))
    }

    #[inline(always)]
    fn trace_frames(&self) -> Vec<*mut c_void> {
        let _guard = self.lock();
        let mut stack = Vec::new();
        unsafe {
            let mut skip = 0;

            backtrace::trace_unsynchronized(|f| {
                // skip the call in alloc.
                // backtrace::backtrace::libunwind::trace::h08cd42aca7d0c759
                //prof_mem::profiler::HeapProfiler::trace_frames::h8cb48184406ee182
                //__rustc[4794b31dd7191200]::__rust_alloc
                //alloc::alloc::alloc::h39a8c1f0979b4a77
                if skip < 4 {
                    skip += 1;
                    return true;
                }
                stack.push(f.ip());
                if stack.len() < self.max_deep.get() {
                    true
                } else {
                    false
                }
            });
        }
        #[cfg(feature = "msg")]
        msg!("the callback deep is {}\n", stack.len());
        stack
    }

    fn resolve_frames(&self, f: &[*mut c_void]) -> Vec<Symbol> {
        f.iter()
            .filter_map(|addr| {
                let mut symbol: Option<Symbol> = None;
                unsafe {
                    backtrace::resolve_unsynchronized(*addr, |frame| {
                        if symbol.is_none() {
                            symbol = Some(frame.into());
                        }
                    });
                }
                symbol
            })
            .collect()
    }

    pub fn write_symbol_frames<T: Write>(
        &self,
        writer: &mut ProfileProtoWriter<T>,
    ) -> io::Result<()> {
        let _guard = self.lock();
        let alloc_frames = unsafe { (*self.frames.get()).assume_init_ref() };
        for (ptr, alloc_frame) in alloc_frames.iter() {
            writer.write_symbol_frame(AllocSymbolFrames {
                frames: self.resolve_frames(&alloc_frame.frames),
                size: alloc_frame.size,
                ptr: *ptr,
            });
        }
        Ok(())
    }

    #[inline(always)]
    pub(crate) fn insert(&self, ptr: *const u8, lay: Layout) {
        let _guard = self.lock();
        let frames = self.trace_frames();
        unsafe {
            (&mut *self.frames.get()).assume_init_mut().insert(
                ptr,
                AllocFrames {
                    size: lay.size(),
                    frames: frames,
                },
            );
        }
    }

    #[inline(always)]
    pub(crate) fn remove(&self, ptr: *const u8) {
        let _guard = self.lock();
        unsafe {
            (&mut *self.frames.get()).assume_init_mut().remove(&ptr);
        }
    }
}

impl From<&backtrace::Symbol> for Symbol {
    #[inline(always)]
    fn from(value: &backtrace::Symbol) -> Self {
        Self {
            addr: value.addr().unwrap_or_default(),
            file_name: value
                .filename()
                .map(|p| p.to_str().unwrap().to_string())
                .unwrap_or_default(),
            line_no: value.lineno().unwrap_or_default(),
            col_no: value.colno().unwrap_or_default(),
            name: value.name().map(|p| p.to_string()).unwrap_or_default(),
        }
    }
}

unsafe impl Send for HeapProfiler {}
unsafe impl Sync for HeapProfiler {}

pub(crate) fn get_profiler(max_deep: Option<usize>) -> &'static HeapProfiler {
    static PROFILER: HeapProfiler = HeapProfiler {
        max_deep: Cell::new(128),
        frames: UnsafeCell::new(MaybeUninit::uninit()),
        init_once: Once::new(),
    };
    PROFILER.init_once(max_deep);
    return &PROFILER;
}

pub fn dump() -> io::Result<()> {
    let _alloc_entry = AllocEntry::new();
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(format!("mem.{}.pb", time.as_millis()))?;
    let mut writer = ProfileProtoWriter::new(&mut file);
    let profiler = get_profiler(None);
    profiler.write_symbol_frames(&mut writer)?;
    writer.flush()
}
