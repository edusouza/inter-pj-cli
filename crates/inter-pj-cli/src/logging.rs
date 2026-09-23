//! Diagnostic logs on stderr, enabled with `-v`/`-vv`.

use tracing::level_filters::LevelFilter;
use tracing_subscriber::Layer;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

pub(crate) fn init(verbosity: u8) {
    let level = match verbosity {
        0 => LevelFilter::WARN,
        1 => LevelFilter::INFO,
        2 => LevelFilter::DEBUG,
        _ => LevelFilter::TRACE,
    };
    // Only this project's crates log. Dependencies (HTTP, TLS) never do, so
    // their internals cannot leak headers, tokens or bodies into the output.
    let targets = Targets::new()
        .with_target("inter_pj", level)
        .with_target("inter_pj_cli", level);
    let layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_target(false)
        .without_time()
        .with_filter(targets);
    let _ = tracing_subscriber::registry().with(layer).try_init();
}
