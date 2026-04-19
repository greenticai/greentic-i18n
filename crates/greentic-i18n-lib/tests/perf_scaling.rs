use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use greentic_i18n_lib::{DefaultResolver, I18n, I18nRequest, I18nTag, normalize_tag};

const OPS_PER_THREAD: usize = 25_000;
const SAMPLES: usize = 4;

fn shared_i18n() -> (Arc<I18n>, greentic_i18n_lib::I18nId) {
    let resolver = Arc::new(DefaultResolver::new(
        I18nTag::new("en-US").expect("valid default tag"),
        Some("USD".to_string()),
    ));
    let i18n = Arc::new(I18n::new(resolver));
    let resolution = i18n
        .resolve_and_cache(I18nRequest::new(
            Some(normalize_tag("fr-CA-u-ca-gregory-nu-latn-tz-usnyc").expect("valid tag")),
            Some("CAD".to_string()),
        ))
        .expect("resolve should work");
    (i18n, resolution.id)
}

fn run_workload(threads: usize) -> Duration {
    let (i18n, id) = shared_i18n();
    let start = Instant::now();
    let handles: Vec<_> = (0..threads)
        .map(|_| {
            let i18n = Arc::clone(&i18n);
            thread::spawn(move || {
                for _ in 0..OPS_PER_THREAD {
                    let profile = i18n.get(&id).expect("cached profile should be present");
                    assert_eq!(profile.tag.as_str(), "fr-CA-u-ca-gregory-nu-latn-tz-usnyc");
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("worker should not panic");
    }

    start.elapsed()
}

fn measured_workload(threads: usize) -> Duration {
    let _warmup = run_workload(threads);
    let mut best = Duration::MAX;
    for _ in 0..SAMPLES {
        best = best.min(run_workload(threads));
    }
    best
}

#[test]
fn cache_reads_scale_without_near_serialization() {
    let t1 = measured_workload(1);
    let t4 = measured_workload(4);
    let t8 = measured_workload(8);

    assert!(
        t4 <= t1.mul_f64(3.3),
        "4-thread cache reads regressed badly: t1={t1:?}, t4={t4:?}"
    );
    assert!(
        t8 <= t1.mul_f64(6.0),
        "8-thread cache reads regressed badly: t1={t1:?}, t8={t8:?}"
    );
}
