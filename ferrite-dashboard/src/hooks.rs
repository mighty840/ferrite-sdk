use dioxus::prelude::*;

/// Returns a signal that increments on a timer, triggering resource re-fetches.
/// The interval is read from localStorage (`ferrite_refresh_interval`, default 5s).
pub fn use_poll_tick() -> Signal<u64> {
    let mut tick = use_signal(|| 0u64);

    use_effect(move || {
        spawn(async move {
            loop {
                let interval_secs: u64 = web_sys::window()
                    .and_then(|w| w.local_storage().ok())
                    .flatten()
                    .and_then(|s| s.get_item("ferrite_refresh_interval").ok())
                    .flatten()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(5);

                gloo_timers::future::TimeoutFuture::new((interval_secs * 1000) as u32).await;
                tick.set(tick() + 1);
            }
        });
    });

    tick
}
