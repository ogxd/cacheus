use prometheus::{Counter, CounterVec, Encoder, Gauge, HistogramOpts, HistogramVec, Opts, Registry, TextEncoder};

pub struct Metrics
{
    pub request_duration: HistogramVec,
    pub requests: CounterVec,
    pub connection_reset: Counter,
    pub cache_entries: Gauge,
    registry: Registry,
}

impl Metrics
{
    pub fn new() -> Metrics
    {
        let metrics = Metrics {
            request_duration: HistogramVec::new(
                HistogramOpts::new("cacheus:request_duration_seconds", "Request duration (s)").buckets(vec![
                    0.00001, /* 10μs */
                    0.00002, 0.00005, 0.0001, /* 100μs */
                    0.0002, 0.0005, 0.001, /* 1ms */
                    0.002, 0.005, 0.01, /* 10ms */
                    0.02, 0.05, 0.1, /* 100ms */
                    0.2, 0.5, 1., /* 1s */
                    2., 5., 10., /* 10s */
                ]), &["status"]
            )
            .unwrap(),
            requests: CounterVec::new(Opts::new("cacheus:requests_total", "Number of cache calls"), &["status", "http_code"]).unwrap(),
            cache_entries: Gauge::with_opts(Opts::new("cacheus:cache_entries_count", "Number of entries in the resident cache")).unwrap(),
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
            .register(Box::new(metrics.requests.clone()))
            .unwrap();
        metrics
            .registry
            .register(Box::new(metrics.cache_entries.clone()))
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
