//! Guards that concurrent cache reads are genuinely parallel rather than
//! funnelled through an exclusive lock.
//!
//! This deliberately does NOT assert a wall-clock scaling ratio (e.g. "4
//! threads must finish within 3.3x of 1 thread"). The read path is
//! `RwLock::read` -> `HashMap::get` -> `Arc::clone`, so every operation
//! performs two atomic read-modify-writes on cache lines shared by all
//! threads. That cache-line ping-pong makes aggregate throughput degrade
//! superlinearly with core count no matter how correct the locking is, so any
//! absolute ratio is really a measurement of the host machine: the old guard
//! passed on 4-vCPU CI runners and failed consistently on higher-core
//! developer machines.
//!
//! Instead the real cache is raced against a deliberately serialized baseline
//! on the same machine, in the same run, under the same contention. If reads
//! ever became exclusive the two would converge and this test fails. Being a
//! comparison, it is self-calibrating across machine speed and core count.
//!
//! The baseline holds the same map behind the same `RwLock` type and differs
//! only in taking `write()` instead of `read()`. Isolating exactly one
//! variable matters: an earlier attempt used a `Mutex` baseline and had no
//! teeth, because `Mutex` is itself ~1.4-2.4x slower than a contended
//! `RwLock::write` here, and that lock-implementation overhead alone cleared
//! the threshold even when reads had been made exclusive.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use greentic_i18n_lib::{
    DefaultResolver, I18n, I18nId, I18nProfile, I18nRequest, I18nTag, normalize_tag,
};

const OPS_PER_THREAD: usize = 100_000;
const SAMPLES: usize = 3;
const CACHED_TAG: &str = "fr-CA-u-ca-gregory-nu-latn-tz-usnyc";

/// The serialized baseline must be at least this much slower than the real
/// cache for concurrent reads to count as parallel.
///
/// Sits between the two states it has to tell apart. Measured on an 11-core
/// machine: 1.9x (2 threads) and 2.7x (4 threads) with reads shared, against
/// 1.0x once `get` is switched to an exclusive lock. The guard is set nearer
/// the broken end so a slow or noisy runner does not trip it.
const MIN_SPEEDUP_OVER_SERIALIZED: f64 = 1.35;

fn shared_i18n() -> (Arc<I18n>, I18nId) {
    let resolver = Arc::new(DefaultResolver::new(
        I18nTag::new("en-US").expect("valid default tag"),
        Some("USD".to_string()),
    ));
    let i18n = Arc::new(I18n::new(resolver));
    let resolution = i18n
        .resolve_and_cache(I18nRequest::new(
            Some(normalize_tag(CACHED_TAG).expect("valid tag")),
            Some("CAD".to_string()),
        ))
        .expect("resolve should work");
    (i18n, resolution.id)
}

/// Threads to contend with: enough to expose serialization, never more than
/// the host can actually run in parallel (oversubscription adds scheduler
/// noise without adding signal).
fn contending_threads() -> usize {
    let cores = thread::available_parallelism()
        .map(|cores| cores.get())
        .unwrap_or(2);
    cores.clamp(2, 4)
}

fn race<F>(threads: usize, read_once: F) -> Duration
where
    F: Fn() -> Arc<I18nProfile> + Send + Sync + 'static,
{
    let read_once = Arc::new(read_once);
    let start = Instant::now();
    let handles: Vec<_> = (0..threads)
        .map(|_| {
            let read_once = Arc::clone(&read_once);
            thread::spawn(move || {
                for _ in 0..OPS_PER_THREAD {
                    let profile = read_once();
                    assert_eq!(profile.tag.as_str(), CACHED_TAG);
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("worker should not panic");
    }

    start.elapsed()
}

/// Reads through the real cache, whose `get` takes a shared read lock.
fn concurrent_cache_reads(threads: usize) -> Duration {
    let (i18n, id) = shared_i18n();
    race(threads, move || {
        i18n.get(&id).expect("cached profile should be present")
    })
}

/// Reads the same profile from an equivalent map, taking the write lock so
/// that only one reader runs at a time. This is the shape the real cache must
/// beat: same lock type, same per-operation work (hash lookup plus
/// `Arc::clone`), differing only in that readers exclude each other.
fn serialized_cache_reads(threads: usize) -> Duration {
    let (i18n, id) = shared_i18n();
    let profile = i18n.get(&id).expect("cached profile should be present");
    let serialized: HashMap<I18nId, Arc<I18nProfile>> = HashMap::from([(id, profile)]);
    let serialized = Arc::new(RwLock::new(serialized));

    race(threads, move || {
        serialized
            .write()
            .expect("baseline lock should not be poisoned")
            .get(&id)
            .cloned()
            .expect("cached profile should be present")
    })
}

/// Best of several runs, discarding a warmup. Taking the minimum on both
/// sides keeps the comparison conservative: the baseline is credited with its
/// luckiest run too.
fn best_of(mut workload: impl FnMut(usize) -> Duration, threads: usize) -> Duration {
    let _warmup = workload(threads);
    (0..SAMPLES).fold(Duration::MAX, |best, _| best.min(workload(threads)))
}

#[test]
fn cache_reads_scale_without_near_serialization() {
    let threads = contending_threads();
    let concurrent = best_of(concurrent_cache_reads, threads);
    let serialized = best_of(serialized_cache_reads, threads);

    let speedup = serialized.as_secs_f64() / concurrent.as_secs_f64();
    assert!(
        speedup >= MIN_SPEEDUP_OVER_SERIALIZED,
        "cache reads look serialized: with {threads} threads the shared-lock cache took \
         {concurrent:?} against {serialized:?} for an exclusively locked equivalent \
         ({speedup:.2}x speedup, expected at least {MIN_SPEEDUP_OVER_SERIALIZED:.2}x). \
         Readers appear to be excluding each other."
    );
}
