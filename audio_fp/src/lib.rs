//! Floating-point environment support shared by the format adapters.
//! No allocator, thread-local storage, locks, or OS calls are used.

#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
compile_error!("audio_fp requires x86_64 MXCSR or AArch64 FPCR/FPSR support");

// Clang xmmintrin.h _MM_FLUSH_ZERO_MASK and emmintrin.h
// _MM_DENORMALS_ZERO_MASK. SSE2 is mandatory on x86_64.
#[cfg(target_arch = "x86_64")]
const FLUSH_MASK: u64 = 0x8040;
// AArch64 FPCR.FZ, also documented by Apple's fenv.h
// __fpcr_flush_to_zero / FE_DFL_DISABLE_DENORMS_ENV.
#[cfg(target_arch = "aarch64")]
const FLUSH_MASK: u64 = 0x0100_0000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Environment {
    control: u64,
    status: u64,
}

impl Environment {
    fn read() -> Self {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            let mut csr = 0u32;
            std::arch::asm!("stmxcsr [{ptr}]", ptr = in(reg) &mut csr,
                options(nostack, preserves_flags));
            Self {
                control: csr.into(),
                status: 0,
            }
        }
        #[cfg(target_arch = "aarch64")]
        unsafe {
            let (control, status);
            std::arch::asm!("mrs {control}, fpcr", "mrs {status}, fpsr",
                control = out(reg) control, status = out(reg) status,
                options(nostack, preserves_flags));
            Self { control, status }
        }
    }

    // Only values read on this thread, with documented control bits changed,
    // reach this function. Keep asm's default compiler memory barrier: the
    // callback's buffer accesses must remain inside the changed environment.
    fn write(self) {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            let csr = self.control as u32;
            std::arch::asm!("ldmxcsr [{ptr}]", ptr = in(reg) &csr,
                options(nostack, preserves_flags));
        }
        #[cfg(target_arch = "aarch64")]
        unsafe {
            std::arch::asm!("msr fpcr, {control}", "msr fpsr, {status}",
                control = in(reg) self.control, status = in(reg) self.status,
                options(nostack, preserves_flags));
        }
    }
}

// Cannot leave the calling thread. The guard is private and never escapes
// the scope function, including when the callback unwinds.
struct Restore(Environment, std::marker::PhantomData<*mut ()>);

impl Drop for Restore {
    fn drop(&mut self) {
        self.0.write();
    }
}

/// Run an audio callback with subnormal inputs/results treated as zero.
///
/// Rounding and exception masks are retained. The calling thread's complete
/// MXCSR (x86_64) or FPCR/FPSR (AArch64) is restored on return or unwind.
/// This intentionally changes arithmetic below the smallest normal value;
/// it does not apply a larger DSP noise floor or sanitize non-finite input.
///
/// ```
/// let output = audio_fp::with_denormals_flushed(|| 0.5_f32 * 0.25);
/// assert_eq!(output, 0.125);
/// ```
#[inline]
pub fn with_denormals_flushed<R>(callback: impl FnOnce() -> R) -> R {
    let previous = Environment::read();
    let _restore = Restore(previous, std::marker::PhantomData);
    Environment {
        control: previous.control | FLUSH_MASK,
        ..previous
    }
    .write();
    callback()
}

/// Test-only access for verifying the adapters through their real ABI.
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub mod test_support {
    use super::*;

    pub fn in_ieee_mode<R>(callback: impl FnOnce() -> R) -> R {
        let previous = Environment::read();
        let _restore = Restore(previous, std::marker::PhantomData);
        Environment {
            control: previous.control & !FLUSH_MASK,
            ..previous
        }
        .write();
        callback()
    }

    pub fn snapshot() -> [u64; 2] {
        let state = Environment::read();
        [state.control, state.status]
    }

    // Runtime operands, with bitwise results: float equality under FTZ could
    // itself treat a nonzero subnormal as zero and falsely pass the test.
    #[inline(never)]
    pub fn arithmetic_probe() -> [u32; 2] {
        use std::hint::black_box as b;
        [
            b(b(f32::from_bits(0x0040_0000)) * b(2.0)).to_bits(),
            b(b(f32::MIN_POSITIVE) * b(0.5)).to_bits(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_support::*;

    #[test]
    fn flushes_denormal_inputs_and_results() {
        in_ieee_mode(|| {
            assert_eq!(arithmetic_probe(), [0x0080_0000, 0x0040_0000]);
            assert_eq!(with_denormals_flushed(arithmetic_probe), [0, 0]);
            assert_eq!(arithmetic_probe(), [0x0080_0000, 0x0040_0000]);
        });
    }

    #[test]
    fn nested_scopes_preserve_rounding_and_exception_status() {
        in_ieee_mode(|| {
            let previous = Environment::read();
            let _restore = Restore(previous, std::marker::PhantomData);
            let mut host = previous;
            #[cfg(target_arch = "x86_64")]
            {
                host.control = (host.control & !0x6000) | 0x2000 | 0x20;
            }
            #[cfg(target_arch = "aarch64")]
            {
                host.control = (host.control & !0x00c0_0000) | 0x0040_0000;
                host.status |= 0x10;
            }
            host.write();
            with_denormals_flushed(|| {
                let active = Environment::read();
                assert_eq!(active.control & !FLUSH_MASK, host.control);
                assert_eq!(with_denormals_flushed(arithmetic_probe), [0, 0]);
                assert_eq!(Environment::read(), active);
            });
            assert_eq!(Environment::read(), host);
        });
    }

    #[test]
    fn unwinding_restores_the_host_environment() {
        in_ieee_mode(|| {
            let before = snapshot();
            let result = std::panic::catch_unwind(|| {
                with_denormals_flushed(|| {
                    assert_eq!(arithmetic_probe(), [0, 0]);
                    panic!("intentional callback unwind");
                })
            });
            assert!(result.is_err());
            assert_eq!(snapshot(), before);
        });
    }
}
