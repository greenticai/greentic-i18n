use std::sync::Arc;

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use greentic_i18n_lib::{DefaultResolver, I18n, I18nRequest, I18nResolver, I18nTag, normalize_tag};

fn build_request() -> I18nRequest {
    I18nRequest::new(
        Some(normalize_tag("fr-CA-u-ca-gregory-nu-latn-tz-usnyc").expect("valid tag")),
        Some("CAD".to_string()),
    )
}

fn bench_normalize_tag(c: &mut Criterion) {
    c.bench_function("normalize_tag.hot_path", |b| {
        b.iter(|| {
            black_box(normalize_tag(black_box(
                "fr-CA-u-ca-gregory-nu-latn-tz-usnyc",
            )))
            .expect("tag should normalize");
        })
    });
}

fn bench_resolve(c: &mut Criterion) {
    let resolver = DefaultResolver::new(
        I18nTag::new("en-US").expect("valid default tag"),
        Some("USD".to_string()),
    );
    let request = build_request();

    c.bench_function("resolver.resolve.lenient", |b| {
        b.iter(|| {
            black_box(resolver.resolve(black_box(request.clone()))).expect("resolve should work");
        })
    });
}

fn bench_hot_cache_get(c: &mut Criterion) {
    let resolver = Arc::new(DefaultResolver::default());
    let i18n = I18n::new(resolver);
    let resolution = i18n
        .resolve_and_cache(build_request())
        .expect("cached resolution should succeed");
    let id = resolution.id;

    c.bench_function("i18n.get.hot_cache", |b| {
        b.iter(|| {
            black_box(i18n.get(black_box(&id))).expect("profile should remain cached");
        })
    });
}

criterion_group!(
    benches,
    bench_normalize_tag,
    bench_resolve,
    bench_hot_cache_get
);
criterion_main!(benches);
