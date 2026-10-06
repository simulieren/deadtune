//! Giving memory back to the OS while the window is out of sight.

/// Asks Windows to page out what this process is not using right now. Task Manager's
/// "Memory" column is the working set, so a minimized DeadTune stops looking heavy; pages
/// come back on first touch. A no-op elsewhere.
#[cfg(windows)]
pub fn trim_working_set() {
    use windows_sys::Win32::System::ProcessStatus::K32EmptyWorkingSet;
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    // SAFETY: GetCurrentProcess returns a pseudo handle that is always valid for this
    // process and needs no closing; K32EmptyWorkingSet only reads it.
    unsafe {
        K32EmptyWorkingSet(GetCurrentProcess());
    }
}

#[cfg(not(windows))]
pub fn trim_working_set() {}
