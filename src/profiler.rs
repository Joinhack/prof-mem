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

use crate::{native::AllocEntry, profile_proto::ProfileProtoWriter};

thread_local! {
    static LOCKED:Cell<bool> = Cell::new(false);
}

#[allow(dead_code)]
pub(crate) struct LockGuard<'a>(Option<std::sync::MutexGuard<'a, ()>>);

impl<'a> Drop for LockGuard<'a> {
    fn drop(&mut self) {
        if self.0.is_some() {
            LOCKED.with(|c| {
                assert!(c.get());
                c.set(false);
            })
        }
    }
}

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
    pub file_name: String,
    pub line_no: u32,
    pub col_no: u32,
    pub name: String,
    pub addr: *mut c_void,
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
    /// That is, this lock can be acquired as many times as you want on a single thread without deadlocking, allowing one thread
    #[inline(always)]
    pub(crate) fn lock(&self) -> LockGuard<'_> {
        static MUTEX: Mutex<()> = Mutex::new(());
        if LOCKED.get() {
            return LockGuard(None);
        }
        LOCKED.with(|locked| locked.set(true));
        LockGuard(Some(MUTEX.lock().unwrap()))
    }

    fn frames(&self) -> Vec<*mut c_void> {
        let _guard = self.lock();
        let mut stack = Vec::new();
        unsafe {
            backtrace::trace_unsynchronized(|f| {
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

    pub(crate) fn insert(&self, ptr: *const u8, lay: Layout) {
        let _guard = self.lock();
        let frames = self.frames();
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
