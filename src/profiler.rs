use std::{
    alloc::Layout,
    cell::{Cell, UnsafeCell},
    collections::HashMap,
    ffi::c_void,
    mem::MaybeUninit,
    ptr,
    sync::{Mutex, Once},
};

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

#[derive(Clone)]
struct FramesRecord {
    alloc: usize,
    frame: Vec<*mut c_void>,
}

pub(crate) struct FramesSymbolRecord {
    alloc: usize,
    frame: Vec<Symbol>,
}

#[derive(Debug)]
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
    frames: UnsafeCell<MaybeUninit<HashMap<*const u8, FramesRecord>>>,
}

impl HeapProfiler {
    #[inline]
    fn init_once(&self, max_deep: usize) {
        self.init_once.call_once(|| {
            unsafe {
                self.max_deep.set(max_deep);
                (&mut *self.frames.get()).write(HashMap::new());
            };
        });
    }

    /// Try to acquire the lock. If it is not available, wait until another thread releases it.
    /// Acquire a  global re-entrant lock over
    /// That is, this lock can be acquired as many times as you want on a single thread without deadlocking, allowing one thread
    #[inline(always)]
    pub(crate) fn lock(&self) -> LockGuard<'_> {
        static mut MUTEX: *mut Mutex<()> = ptr::null_mut();
        static ONCE: Once = Once::new();

        if LOCKED.get() {
            return LockGuard(None);
        }
        ONCE.call_once(|| unsafe {
            let mutex = Mutex::new(());
            MUTEX = Box::into_raw(Box::new(mutex));
        });
        unsafe {
            LOCKED.set(true);
            let guard = Some((*MUTEX).lock().unwrap());
            LockGuard(guard)
        }
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

    fn resolve_frame(&self, f: &FramesRecord) -> FramesSymbolRecord {
        let frames: Vec<Symbol> = f
            .frame
            .iter()
            .filter_map(|f| {
                let mut symbol: Option<Symbol> = None;
                unsafe {
                    backtrace::resolve_unsynchronized(*f, |frame| symbol = Some(frame.into()));
                }
                symbol
            })
            .collect();
        FramesSymbolRecord {
            alloc: f.alloc,
            frame: frames,
        }
    }

    pub fn clone_frames(&self) -> HashMap<*const u8, FramesRecord> {
        let mut rs = HashMap::new();
        unsafe {
            for (k, v) in (*self.frames.get()).assume_init_mut().iter() {
                rs.insert(*k, v.clone());
            }
        }
        rs
    }

    pub fn resolve_frames(&self, frames: HashMap<*const u8, FramesRecord>) {
        let mut rs = HashMap::new();
        for (k, v) in frames.iter() {
            rs.insert(*k, self.resolve_frame(v));
        }
    }

    pub(crate) fn insert(&self, ptr: *const u8, lay: Layout) {
        let _guard = self.lock();
        let frames = self.frames();
        unsafe {
            (&mut *self.frames.get()).assume_init_mut().insert(
                ptr,
                FramesRecord {
                    alloc: lay.size(),
                    frame: frames,
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

pub(crate) fn get_profiler(max_deep: usize) -> &'static HeapProfiler {
    static PROFILER: HeapProfiler = HeapProfiler {
        max_deep: Cell::new(128),
        frames: UnsafeCell::new(MaybeUninit::uninit()),
        init_once: Once::new(),
    };
    PROFILER.init_once(max_deep);
    return &PROFILER;
}

pub fn dump() {
    let profiler = get_profiler(128);
    let frames = profiler.clone_frames();
    profiler.resolve_frames(frames);
}
