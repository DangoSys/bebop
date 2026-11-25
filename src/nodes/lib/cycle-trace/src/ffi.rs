use crate::CycleTrace;
use std::{
    ffi::{c_char, CStr},
    path::Path,
};

#[no_mangle]
pub unsafe extern "C" fn buckyball_trace_create(directory: *const c_char) -> *mut CycleTrace {
    let directory = CStr::from_ptr(directory).to_str().expect("trace directory is UTF-8");
    Box::into_raw(Box::new(CycleTrace::new(Path::new(directory))))
}

#[no_mangle]
pub unsafe extern "C" fn buckyball_trace_write(trace: *mut CycleTrace, channel: u32, word: u32, value: u64) {
    (*trace)
        .write(channel, word as usize, value)
        .expect("record cycle trace event");
}

#[no_mangle]
pub unsafe extern "C" fn buckyball_trace_finish(trace: *mut CycleTrace) {
    Box::from_raw(trace).finish().expect("finish cycle trace");
}
