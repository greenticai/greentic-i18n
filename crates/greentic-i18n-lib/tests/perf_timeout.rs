use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use greentic_i18n_lib::{DefaultResolver, I18n, I18nRequest, I18nTag, normalize_tag};

#[test]
fn cache_hot_path_finishes_well_within_timeout() {
    let (done_tx, done_rx) = mpsc::channel();

    thread::spawn(move || {
        let resolver = Arc::new(DefaultResolver::new(
            I18nTag::new("en-US").expect("valid default tag"),
            Some("USD".to_string()),
        ));
        let i18n = I18n::new(resolver);
        let resolution = i18n
            .resolve_and_cache(I18nRequest::new(
                Some(normalize_tag("fr-CA-u-ca-gregory-nu-latn-tz-usnyc").expect("valid tag")),
                Some("CAD".to_string()),
            ))
            .expect("resolve should work");

        for _ in 0..200_000 {
            let profile = i18n
                .get(&resolution.id)
                .expect("cached profile should stay available");
            assert_eq!(profile.currency.as_deref(), Some("CAD"));
        }

        done_tx.send(()).expect("send should succeed");
    });

    done_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("hot cache path should not hang or stall");
}
