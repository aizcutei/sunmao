//! Resident set size, read from the operating system.
//!
//! Leak detection needs a number the OS agrees with, not one the process keeps
//! about itself: a leak that matters is memory the process asked for and never
//! gave back, and an internal counter would miss anything the allocator or a
//! plugin's own runtime did behind our back.
//!
//! Every platform reports this differently and none of them report it
//! precisely. Reading it is the easy half; deciding what a reading *means* is
//! [`GrowthVerdict`], which is separated out precisely so it can be tested
//! without a leak to look at.

/// Resident set size in bytes, or `None` where the platform did not answer.
///
/// `None` is deliberately not `0`: "the OS would not tell us" and "this
/// process resides in no memory at all" must not be the same value, or a
/// platform where the query fails would silently report a perfect result.
pub fn resident_bytes() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        // `/proc/self/statm` field 2 is resident pages. `/proc/self/status`'s
        // VmRSS is the same number pre-formatted, but statm avoids parsing a
        // labelled table and is stable across kernels.
        let text = std::fs::read_to_string("/proc/self/statm").ok()?;
        let pages: u64 = text.split_whitespace().nth(1)?.parse().ok()?;
        let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        if page_size <= 0 {
            return None;
        }
        Some(pages.saturating_mul(page_size as u64))
    }

    #[cfg(target_os = "macos")]
    {
        // `task_info` with `MACH_TASK_BASIC_INFO`. The older `TASK_BASIC_INFO`
        // truncates resident size to 32 bits, which silently wraps for a
        // process over 4 GiB — a GUI host with a GPU backend can get there.
        const MACH_TASK_BASIC_INFO: libc::c_int = 20;

        #[repr(C)]
        #[derive(Default)]
        struct MachTaskBasicInfo {
            virtual_size: u64,
            resident_size: u64,
            resident_size_max: u64,
            user_time: [i32; 2],
            system_time: [i32; 2],
            policy: i32,
            suspend_count: i32,
        }

        unsafe extern "C" {
            fn mach_task_self() -> libc::c_uint;
            fn task_info(
                target_task: libc::c_uint,
                flavor: libc::c_int,
                task_info_out: *mut libc::c_void,
                task_info_count: *mut libc::c_uint,
            ) -> libc::c_int;
        }

        // `MACH_TASK_BASIC_INFO_COUNT` is upstream's
        // `sizeof(mach_task_basic_info_data_t) / sizeof(natural_t)`. Deriving
        // it from the struct rather than writing the number keeps the two from
        // drifting -- the first attempt here hardcoded 10, which is the count
        // for the *older* `TASK_BASIC_INFO`, and `task_info` rejected the call.
        const _: () = assert!(std::mem::size_of::<MachTaskBasicInfo>() == 48);
        let count_words =
            (std::mem::size_of::<MachTaskBasicInfo>() / std::mem::size_of::<u32>()) as libc::c_uint;

        let mut info = MachTaskBasicInfo::default();
        let mut count = count_words;
        let result = unsafe {
            task_info(
                mach_task_self(),
                MACH_TASK_BASIC_INFO,
                (&mut info as *mut MachTaskBasicInfo).cast(),
                &mut count,
            )
        };
        if result != 0 {
            return None;
        }
        Some(info.resident_size)
    }

    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::System::ProcessStatus::{
            GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
        };
        use windows_sys::Win32::System::Threading::GetCurrentProcess;

        let mut counters: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
        counters.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        let ok = unsafe {
            GetProcessMemoryInfo(
                GetCurrentProcess(),
                &mut counters,
                std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
            )
        };
        if ok == 0 {
            return None;
        }
        Some(counters.WorkingSetSize as u64)
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

/// What a pair of readings means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrowthVerdict {
    /// Growth per iteration is at or under the budget.
    Stable,
    /// Growth per iteration exceeds the budget.
    Leaking,
    /// The platform would not report memory, so nothing was measured.
    ///
    /// Deliberately distinct from `Stable`: a step that treats "could not
    /// measure" as "no leak" is a step that quietly stops testing the moment
    /// the query breaks.
    Unmeasured,
}

/// Decide whether a measured run leaked.
///
/// `baseline` is read **after** the warm-up iterations, not before them. The
/// first pass through any of this loads libraries, builds caches and lets the
/// allocator claim arenas, and counting that as a leak would make the check
/// fail for every plugin regardless of merit.
///
/// Shrinking counts as stable rather than as negative growth: a process
/// returning memory is not the thing being looked for, and allowing the
/// subtraction to go negative would let a late release mask a real leak
/// earlier in the same run.
pub fn verdict(
    baseline: Option<u64>,
    after: Option<u64>,
    iterations: u32,
    budget_bytes_per_iteration: u64,
) -> GrowthVerdict {
    let (Some(baseline), Some(after)) = (baseline, after) else {
        return GrowthVerdict::Unmeasured;
    };
    if iterations == 0 {
        return GrowthVerdict::Unmeasured;
    }
    let growth = after.saturating_sub(baseline);
    let allowed = budget_bytes_per_iteration.saturating_mul(u64::from(iterations));
    if growth <= allowed {
        GrowthVerdict::Stable
    } else {
        GrowthVerdict::Leaking
    }
}

/// Human-readable size, for report lines a person has to read.
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_platform_reports_a_plausible_resident_size() {
        let bytes = resident_bytes().expect("one of the three supported platforms should answer");
        // A running test process with a loaded std is comfortably over 64 KiB
        // and comfortably under 64 GiB. The point is to catch a unit mix-up --
        // reporting pages as bytes would land far below this floor.
        assert!(
            bytes > 64 * 1024,
            "resident size {bytes} is implausibly small; are pages being reported as bytes?"
        );
        assert!(
            bytes < 64 * 1024 * 1024 * 1024,
            "resident size {bytes} is implausibly large"
        );
    }

    #[test]
    fn two_readings_in_a_row_are_close_to_each_other() {
        let first = resident_bytes().expect("supported platform");
        let second = resident_bytes().expect("supported platform");
        let difference = first.abs_diff(second);
        assert!(
            difference < 64 * 1024 * 1024,
            "two consecutive readings differed by {difference} bytes"
        );
    }

    #[test]
    fn growth_inside_the_budget_is_stable() {
        assert_eq!(
            verdict(Some(1_000_000), Some(1_000_000), 100, 1024),
            GrowthVerdict::Stable
        );
        assert_eq!(
            verdict(Some(1_000_000), Some(1_100_000), 100, 1024),
            GrowthVerdict::Stable,
            "100 KiB over 100 iterations is inside a 1 KiB/iteration budget"
        );
    }

    #[test]
    fn growth_past_the_budget_is_a_leak() {
        assert_eq!(
            verdict(Some(1_000_000), Some(1_200_000), 100, 1024),
            GrowthVerdict::Leaking
        );
    }

    /// The boundary is inclusive, and stated here rather than left to whoever
    /// reads the `<=` later.
    #[test]
    fn the_budget_boundary_itself_passes() {
        assert_eq!(
            verdict(Some(0), Some(102_400), 100, 1024),
            GrowthVerdict::Stable
        );
        assert_eq!(
            verdict(Some(0), Some(102_401), 100, 1024),
            GrowthVerdict::Leaking
        );
    }

    /// A process that gave memory back has not leaked, and must not be able to
    /// bank that credit against a later leak either.
    #[test]
    fn shrinking_is_stable_and_earns_no_credit() {
        assert_eq!(
            verdict(Some(2_000_000), Some(1_000_000), 10, 0),
            GrowthVerdict::Stable
        );
        // Saturating subtraction means the 1 MiB released above cannot offset
        // a later 1 MiB gain in a different run.
        assert_eq!(
            verdict(Some(1_000_000), Some(2_000_000), 10, 0),
            GrowthVerdict::Leaking
        );
    }

    /// "Could not measure" must never read as "no leak".
    #[test]
    fn an_unmeasured_run_is_not_a_passing_run() {
        assert_eq!(verdict(None, Some(1), 10, 0), GrowthVerdict::Unmeasured);
        assert_eq!(verdict(Some(1), None, 10, 0), GrowthVerdict::Unmeasured);
        assert_eq!(verdict(None, None, 10, 0), GrowthVerdict::Unmeasured);
        assert_ne!(verdict(None, None, 10, 0), GrowthVerdict::Stable);
    }

    /// Zero iterations means nothing was exercised, so there is nothing to
    /// conclude -- dividing a budget by it would be worse than saying so.
    #[test]
    fn zero_iterations_conclude_nothing() {
        assert_eq!(
            verdict(Some(0), Some(u64::MAX), 0, 0),
            GrowthVerdict::Unmeasured
        );
    }

    #[test]
    fn a_huge_budget_cannot_overflow_into_a_small_one() {
        assert_eq!(
            verdict(Some(0), Some(u64::MAX - 1), u32::MAX, u64::MAX),
            GrowthVerdict::Stable,
            "saturating multiplication must not wrap a vast budget into a tiny one"
        );
    }

    #[test]
    fn sizes_are_reported_in_units_a_person_reads() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(2048), "2.00 KiB");
        assert_eq!(human_bytes(5 * 1024 * 1024), "5.00 MiB");
        assert_eq!(human_bytes(3 * 1024 * 1024 * 1024), "3.00 GiB");
    }
}
