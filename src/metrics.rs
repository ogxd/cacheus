use prometheus::{Counter, Encoder, HistogramVec, HistogramOpts, Opts, Registry, TextEncoder};

pub struct Metrics
{
    pub request_duration: HistogramVec,
    pub cache_calls: Counter,
    // cache_hits: Counter,
    pub cache_misses: Counter,
    pub connection_reset: Counter,
    // cache_evictions: Counter,
    // cache_resident_size: Counter,
    // cache_probatory_size: Counter,
    registry: Registry,
}

impl Metrics
{
    pub fn new() -> Metrics
    {
        let metrics = Metrics {
            request_duration: HistogramVec::new(
                HistogramOpts::new("cacheus:request_duration", "Request duration (s)").buckets(vec![
                    0.00001, /* 10μs */
                    0.00002, 0.00005, 0.0001, /* 100μs */
                    0.0002, 0.0005, 0.001, /* 1ms */
                    0.002, 0.005, 0.01, /* 10ms */
                    0.02, 0.05, 0.1, /* 100ms */
                    0.2, 0.5, 1., /* 1s */
                    2., 5., 10., /* 10s */
                ]), &["cached"]
            )
            .unwrap(),
            cache_calls: Counter::with_opts(Opts::new("cacheus:calls_total", "Number of cache calls")).unwrap(),
            cache_misses: Counter::with_opts(Opts::new("cacheus:misses_total", "Number of cache misses")).unwrap(),
            connection_reset: Counter::with_opts(Opts::new("cacheus:connection_reset_total", "Number of connection reset (RST)"))
                .unwrap(),
            registry: Registry::new(),
        };
        metrics
            .registry
            .register(Box::new(metrics.request_duration.clone()))
            .unwrap();
        metrics
            .registry
            .register(Box::new(metrics.cache_calls.clone()))
            .unwrap();
        metrics
            .registry
            .register(Box::new(metrics.cache_misses.clone()))
            .unwrap();
        metrics
            .registry
            .register(Box::new(metrics.connection_reset.clone()))
            .unwrap();
        metrics
    }

    pub fn encode(&self) -> Vec<u8>
    {
        let mut buffer = vec![];
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        encoder.encode(&metric_families, &mut buffer).unwrap();
        buffer
    }
}
