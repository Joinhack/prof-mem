macro_rules! msg {
    ($($arg:tt)*) => {
        let msg = format!($($arg)*);
        use std::str::FromStr;
        let cstr = std::ffi::CString::from_str(&msg).unwrap();
        unsafe {
            libc::printf(cstr.as_c_str().as_ptr() as _);
        }
    };
}
