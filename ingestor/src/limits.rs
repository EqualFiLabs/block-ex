use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use std::num::NonZeroU32;

pub fn make_limiter(rps: u32, bootstrap: bool) -> DefaultDirectRateLimiter {
    let eff = if bootstrap {
        ((rps as f32) * 2.5).ceil() as u32
    } else {
        rps
    };
    RateLimiter::direct(Quota::per_second(
        NonZeroU32::new(eff.max(1)).expect("quota denominator must be non-zero"),
    ))
}

pub fn eff_concurrency(base: usize, bootstrap: bool) -> usize {
    if bootstrap {
        (base * 2).max(base + 4)
    } else {
        base
    }
}
