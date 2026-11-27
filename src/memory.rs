use std::ffi::CStr;
use std::mem::MaybeUninit;

use raiden_core::probe::Memory;

unsafe extern "C" {
    fn responsibility_get_pid_responsible_for_pid(pid: libc::pid_t) -> libc::pid_t;
}

fn all_pids() -> Vec<libc::pid_t> {
    let count = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    let Ok(count) = usize::try_from(count) else {
        return Vec::new();
    };
    let mut pids: Vec<libc::pid_t> = vec![0; count + 64];
    let Ok(bytes) = libc::c_int::try_from(pids.len() * size_of::<libc::pid_t>()) else {
        return Vec::new();
    };
    let found = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), bytes) };
    pids.truncate(usize::try_from(found).unwrap_or(0));
    pids
}

fn name(pid: libc::pid_t) -> String {
    let mut buffer = [0_u8; 256];
    let length = unsafe {
        libc::proc_name(
            pid,
            buffer.as_mut_ptr().cast(),
            u32::try_from(buffer.len()).unwrap_or(0),
        )
    };
    if length <= 0 {
        return String::new();
    }
    CStr::from_bytes_until_nul(&buffer)
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn footprint(pid: libc::pid_t) -> Option<u64> {
    let mut info = MaybeUninit::<libc::rusage_info_v2>::zeroed();
    let result =
        unsafe { libc::proc_pid_rusage(pid, libc::RUSAGE_INFO_V2, info.as_mut_ptr().cast()) };
    (result == 0).then(|| unsafe { info.assume_init() }.ri_phys_footprint)
}

pub fn idle(app_name: &str) -> Option<Memory> {
    let pids = all_pids();
    let me = unsafe { libc::getpid() };
    let family_of = |raiden: libc::pid_t| -> Vec<libc::pid_t> {
        pids.iter()
            .copied()
            .filter(|pid| {
                *pid == raiden
                    || unsafe { responsibility_get_pid_responsible_for_pid(*pid) } == raiden
            })
            .collect()
    };
    let family = pids
        .iter()
        .copied()
        .filter(|pid| *pid != me && name(*pid) == app_name)
        .map(family_of)
        .max_by_key(Vec::len)?;
    let bytes = family.iter().filter_map(|pid| footprint(*pid)).sum();
    Some(Memory {
        bytes,
        processes: family.len(),
    })
}
