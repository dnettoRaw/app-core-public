// =============================================================================
//        #######
//     ###       ###     F: macos_memory.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 08:11:39 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-beta.2
// =============================================================================

//! Own-process RSS, read through the system Mach task_info ABI.
use crate::{UiError, UiResult};
#[repr(C, packed(4))]
#[derive(Default)]
struct BasicInfo {
    virtual_size: u64,
    resident_size: u64,
    resident_size_max: u64,
    user_time: [i32; 2],
    system_time: [i32; 2],
    policy: i32,
    suspend_count: i32,
}
const _: [(); 48] = [(); std::mem::size_of::<BasicInfo>()];
unsafe extern "C" {
    static mach_task_self_: u32;
    fn task_info(task: u32, flavor: i32, data: *mut i32, count: *mut u32) -> i32;
}
/// Current resident bytes of this process, not system RAM, peak RSS or measured VRAM.
/// A small synchronous OS query; callers should sample slowly, outside critical rendering work.
pub fn native_process_resident_bytes() -> UiResult<u64> {
    let mut info = BasicInfo::default();
    let mut count = 12;
    // SAFETY: Apple MACH_TASK_BASIC_INFO uses a 4-byte-packed 48-byte structure,
    // twelve natural_t words. The writable buffer/count live through the call;
    // mach_task_self_ is the system-owned current-process port.
    let result = unsafe {
        task_info(
            mach_task_self_,
            20,
            (&mut info as *mut BasicInfo).cast(),
            &mut count,
        )
    };
    if result != 0 || count != 12 {
        return Err(UiError::Unavailable);
    }
    Ok(info.resident_size)
}
#[cfg(test)]
mod tests {
    #[test]
    fn own_process_reports_resident_memory() {
        assert!(super::native_process_resident_bytes().unwrap() > 0);
    }
}
