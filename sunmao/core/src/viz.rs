//! Lock-free audio→GUI visualization transport.
//!
//! The audio thread may not allocate, block, or take a lock, and the GUI thread
//! must never see a half-written frame. [`VizChannel`] is a triple buffer: the
//! producer always has a slot to write into, the consumer always has a complete
//! slot to read, and neither waits for the other.
//!
//! A display wants the *newest* frame, not every frame. That is what makes a
//! triple buffer the right shape rather than a queue: when the GUI paints at
//! 60 Hz and the audio thread publishes every block, dropping the frames in
//! between is correct behaviour, not data loss.

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// A frame of visualization data.
///
/// `Copy` and `Default` because the channel pre-allocates its slots at
/// construction: the audio thread must never allocate one.
pub trait VizFrame: Copy + Default + Send + 'static {}
impl<T: Copy + Default + Send + 'static> VizFrame for T {}

/// Bit set in the shared index when a newly published frame has not yet been
/// read. Keeping it in the same word as the index means publish and consume are
/// each a single atomic operation.
const FRESH: usize = 1 << 2;
const INDEX_MASK: usize = 0b11;

struct VizShared<T> {
    /// Three slots: one being written, one most recently published, one held by
    /// the reader.
    slots: [UnsafeCell<T>; 3],
    /// Index of the most recently published slot, plus the `FRESH` flag.
    published: AtomicUsize,
}

// SAFETY: every slot is only ever touched by one side at a time. The producer
// writes only its own `write_index`, which is never the published index and
// never the reader's index; the consumer only reads a slot after swapping it
// out of `published`, at which point the producer will not choose it again
// until the next swap.
unsafe impl<T: Send> Send for VizShared<T> {}
unsafe impl<T: Send> Sync for VizShared<T> {}

/// The audio-thread half. Publishing is one store and one swap: no allocation,
/// no lock, no unbounded loop.
pub struct VizPublisher<T: VizFrame> {
    shared: Arc<VizShared<T>>,
    write_index: usize,
}

/// The GUI-thread half.
pub struct VizConsumer<T: VizFrame> {
    shared: Arc<VizShared<T>>,
    read_index: usize,
    /// Last frame handed out, so [`VizConsumer::latest`] can keep returning it
    /// when the producer has published nothing new.
    last: T,
}

/// A connected publisher/consumer pair.
pub type VizChannel<T> = (VizPublisher<T>, VizConsumer<T>);

/// Create a connected publisher/consumer pair.
///
/// ```
/// # use sunmao_core::viz::viz_channel;
/// let (mut publisher, mut consumer) = viz_channel::<[f32; 4]>();
/// assert_eq!(consumer.take_fresh(), None);
/// publisher.publish([1.0, 2.0, 3.0, 4.0]);
/// assert_eq!(consumer.take_fresh(), Some([1.0, 2.0, 3.0, 4.0]));
/// // Nothing new since: the display keeps the frame it already has.
/// assert_eq!(consumer.take_fresh(), None);
/// assert_eq!(consumer.latest(), [1.0, 2.0, 3.0, 4.0]);
/// ```
pub fn viz_channel<T: VizFrame>() -> (VizPublisher<T>, VizConsumer<T>) {
    let shared = Arc::new(VizShared {
        slots: [
            UnsafeCell::new(T::default()),
            UnsafeCell::new(T::default()),
            UnsafeCell::new(T::default()),
        ],
        // Slot 1 published, not fresh; producer writes 0, consumer holds 2.
        published: AtomicUsize::new(1),
    });
    (
        VizPublisher {
            shared: Arc::clone(&shared),
            write_index: 0,
        },
        VizConsumer {
            shared,
            read_index: 2,
            last: T::default(),
        },
    )
}

impl<T: VizFrame> VizPublisher<T> {
    /// Publish a frame. Safe to call from the audio thread.
    ///
    /// Overwrites any frame the consumer has not collected — the newest frame
    /// is the one a display wants.
    pub fn publish(&mut self, frame: T) {
        // SAFETY: `write_index` is exclusively ours until the swap below.
        unsafe {
            *self.shared.slots[self.write_index].get() = frame;
        }
        let previous = self
            .shared
            .published
            .swap(self.write_index | FRESH, Ordering::AcqRel);
        // Take ownership of whatever slot the consumer is not using.
        self.write_index = previous & INDEX_MASK;
    }
}

impl<T: VizFrame> VizConsumer<T> {
    /// Take the newest frame if one has been published since the last call.
    ///
    /// `None` means nothing new, which a painter treats as "reuse what is
    /// already on screen" rather than as an error.
    pub fn take_fresh(&mut self) -> Option<T> {
        if self.shared.published.load(Ordering::Acquire) & FRESH == 0 {
            return None;
        }
        let previous = self
            .shared
            .published
            .swap(self.read_index, Ordering::AcqRel);
        self.read_index = previous & INDEX_MASK;
        // SAFETY: the swap gave us exclusive ownership of `read_index`; the
        // producer now owns the slot we handed back.
        let frame = unsafe { *self.shared.slots[self.read_index].get() };
        self.last = frame;
        Some(frame)
    }

    /// The newest frame seen so far, or `T::default()` before the first one.
    pub fn latest(&mut self) -> T {
        if let Some(frame) = self.take_fresh() {
            return frame;
        }
        self.last
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    struct TestAllocator;

    thread_local! {
        static ALLOCATOR_CALL_COUNT: Cell<isize> = const { Cell::new(-1) };
    }

    unsafe impl GlobalAlloc for TestAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            record();
            unsafe { System.alloc(layout) }
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            record();
            unsafe { System.dealloc(ptr, layout) }
        }
        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            record();
            unsafe { System.alloc_zeroed(layout) }
        }
        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            record();
            unsafe { System.realloc(ptr, layout, new_size) }
        }
    }

    fn record() {
        let _ = ALLOCATOR_CALL_COUNT.try_with(|count| {
            let current = count.get();
            if current >= 0 {
                count.set(current + 1);
            }
        });
    }

    #[global_allocator]
    static TEST_ALLOCATOR: TestAllocator = TestAllocator;

    fn count_allocations<R>(callback: impl FnOnce() -> R) -> (R, usize) {
        ALLOCATOR_CALL_COUNT.with(|count| count.set(0));
        let result = callback();
        let calls = ALLOCATOR_CALL_COUNT.with(|count| {
            let value = count.get();
            count.set(-1);
            value as usize
        });
        (result, calls)
    }

    #[test]
    fn publishing_never_allocates() {
        let (mut publisher, mut consumer) = viz_channel::<[f32; 32]>();
        let frame = [0.5f32; 32];
        let (_, calls) = count_allocations(|| {
            for _ in 0..1_000 {
                publisher.publish(frame);
            }
        });
        assert_eq!(calls, 0, "the audio path allocated {calls} times");
        assert_eq!(consumer.take_fresh(), Some(frame));
    }

    #[test]
    fn the_consumer_always_gets_the_newest_frame() {
        let (mut publisher, mut consumer) = viz_channel::<u32>();
        for value in 1..=10 {
            publisher.publish(value);
        }
        // A display wants the newest, not a backlog of ten.
        assert_eq!(consumer.take_fresh(), Some(10));
        assert_eq!(consumer.take_fresh(), None);
    }

    #[test]
    fn a_frame_is_never_read_while_it_is_being_written() {
        // The invariant that makes this safe is that the three indices are
        // always distinct. Drive the swap protocol hard and check it holds.
        let (mut publisher, mut consumer) = viz_channel::<u32>();
        for round in 0..1_000u32 {
            publisher.publish(round);
            let published = consumer.shared.published.load(Ordering::Acquire) & INDEX_MASK;
            assert_ne!(publisher.write_index, published, "round {round}");
            assert_ne!(publisher.write_index, consumer.read_index, "round {round}");
            if let Some(frame) = consumer.take_fresh() {
                assert_eq!(frame, round);
                assert_ne!(consumer.read_index, publisher.write_index, "round {round}");
            }
        }
    }

    #[test]
    fn latest_repeats_the_last_frame_rather_than_resetting_to_default() {
        let (mut publisher, mut consumer) = viz_channel::<u32>();
        assert_eq!(consumer.latest(), 0);
        publisher.publish(7);
        assert_eq!(consumer.latest(), 7);
        // Nothing new published: a painter must keep showing 7, not blink to 0.
        assert_eq!(consumer.latest(), 7);
        assert_eq!(consumer.latest(), 7);
    }

    #[test]
    fn frames_survive_crossing_a_real_thread_boundary() {
        use std::sync::atomic::AtomicBool;

        let (mut publisher, mut consumer) = viz_channel::<[f32; 8]>();
        let finished = Arc::new(AtomicBool::new(false));
        let producer_finished = Arc::clone(&finished);
        let producer = std::thread::spawn(move || {
            for round in 0..5_000u32 {
                publisher.publish([round as f32; 8]);
            }
            producer_finished.store(true, Ordering::Release);
            publisher
        });

        let mut seen = 0usize;
        let mut last = -1.0f32;
        let mut check = |frame: [f32; 8], last: &mut f32| {
            // Every element of a frame must come from the same publish: a torn
            // read would mix two rounds.
            assert!(
                frame.iter().all(|value| *value == frame[0]),
                "torn frame: {frame:?}"
            );
            // Frames may be skipped, but never go backwards.
            assert!(frame[0] >= *last, "{} went backwards from {last}", frame[0]);
            *last = frame[0];
        };

        // Poll until the producer says it is done rather than for a fixed
        // number of iterations. A fixed count races the scheduler: the consumer
        // loop is a single atomic load, so on a loaded machine it can burn
        // through every iteration before the spawned thread is ever scheduled.
        while !finished.load(Ordering::Acquire) {
            if let Some(frame) = consumer.take_fresh() {
                check(frame, &mut last);
                seen += 1;
            }
        }
        let _ = producer.join().unwrap();

        // Whatever the interleaving was, the last publish left a fresh frame
        // behind unless the consumer already collected one — so this makes
        // `seen > 0` a fact about the channel rather than about scheduling.
        if let Some(frame) = consumer.take_fresh() {
            check(frame, &mut last);
            seen += 1;
        }
        assert!(seen > 0, "the consumer never saw a frame");
        assert_eq!(last, 4_999.0, "the final frame was not the newest one");
    }
}
