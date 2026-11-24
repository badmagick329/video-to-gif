//! How much physical memory is free right now: the frame-memory guard's budget.

/// Free physical memory in bytes, from `GlobalMemoryStatusEx`'s `ullAvailPhys`: what can be
/// committed without paging anything out. `None` if Windows does not answer.
#[cfg(windows)]
pub fn available_bytes() -> Option<u64> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    // SAFETY: MEMORYSTATUSEX is plain data, valid zeroed; the call requires `dwLength` set first.
    let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    // SAFETY: `status` is a valid, writable MEMORYSTATUSEX with its length set.
    let ok = unsafe { GlobalMemoryStatusEx(&mut status) };
    (ok != 0).then_some(status.ullAvailPhys)
}

/// Other platforms are untested ([ADR](../../../../docs/decisions/2026-11-24-windows-first.md));
/// without a reading the guard lets every run start.
#[cfg(not(windows))]
pub fn available_bytes() -> Option<u64> {
    None
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn windows_reports_some_free_memory() {
        let free = super::available_bytes().unwrap();
        // Any machine that runs the tests has more than 64 MiB free.
        assert!(free > 64 << 20, "{free}");
    }
}
