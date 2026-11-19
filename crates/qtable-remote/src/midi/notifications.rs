//! CoreMIDI's endpoint cache needs notification delivery on the first client's
//! thread. Pump only on the discovery worker, before its once-a-second scan.
use std::ffi::c_void;

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFRunLoopDefaultMode: *const c_void;
    fn CFRunLoopRunInMode(mode: *const c_void, seconds: f64, return_after_source: u8) -> i32;
}

pub(super) fn pump() {
    // SAFETY: the framework constant is a valid CFStringRef. This processes the
    // current thread's pending sources without waiting or retaining any pointers.
    unsafe { CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.0, 0) };
}
