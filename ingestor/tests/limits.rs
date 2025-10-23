use governor::DefaultDirectRateLimiter;
use ingestor::limits;
use std::num::NonZeroU32;

#[test]
fn limiter_scales_under_bootstrap() {
    assert_eq!(limits::eff_concurrency(8, false), 8);
    assert!(limits::eff_concurrency(8, true) >= 16);
    // can't inspect internal quota directly; assert derived math
    assert_eq!(super_eff_rps(10, false), 10);
    assert_eq!(super_eff_rps(10, true), 25);

    assert!(allows_batch(&limits::make_limiter(10, false), 10));
    assert!(allows_batch(&limits::make_limiter(10, true), 25));
    assert!(!allows_batch(&limits::make_limiter(10, true), 26));
}

fn super_eff_rps(rps: u32, bootstrap: bool) -> u32 {
    if bootstrap {
        ((rps as f32) * 2.5).ceil() as u32
    } else {
        rps
    }
}

fn allows_batch(limiter: &DefaultDirectRateLimiter, n: u32) -> bool {
    NonZeroU32::new(n)
        .and_then(|count| limiter.check_n(count).ok())
        .map(|res| res.is_ok())
        .unwrap_or(false)
}
