use iotai_sdk::metrics::{MetricsBuffer, MetricValue};

pub fn counter_increment() -> Result<(), &'static str> {
    let mut buf: MetricsBuffer<8> = MetricsBuffer::new();
    buf.increment("clicks", 1, 0).map_err(|_| "first increment failed")?;
    buf.increment("clicks", 1, 0).map_err(|_| "second increment failed")?;
    let entry = buf.iter().find(|e| e.key == "clicks")
        .ok_or("entry not found")?;
    match entry.value {
        MetricValue::Counter(v) if v == 2 => Ok(()),
        MetricValue::Counter(_) => Err("counter value wrong"),
        _ => Err("wrong metric type"),
    }
}

pub fn gauge_overwrite() -> Result<(), &'static str> {
    let mut buf: MetricsBuffer<8> = MetricsBuffer::new();
    buf.gauge("temp", 23.5, 0).map_err(|_| "first gauge failed")?;
    buf.gauge("temp", 25.0, 10).map_err(|_| "second gauge failed")?;
    let entry = buf.iter().find(|e| e.key == "temp")
        .ok_or("entry not found")?;
    match entry.value {
        MetricValue::Gauge(v) if v == 25.0 => Ok(()),
        MetricValue::Gauge(_) => Err("gauge value wrong"),
        _ => Err("wrong metric type"),
    }
}

pub fn histogram_accumulate() -> Result<(), &'static str> {
    let mut buf: MetricsBuffer<8> = MetricsBuffer::new();
    buf.observe("latency", 10.0, 0).map_err(|_| "observe 1 failed")?;
    buf.observe("latency", 5.0, 0).map_err(|_| "observe 2 failed")?;
    buf.observe("latency", 20.0, 0).map_err(|_| "observe 3 failed")?;
    let entry = buf.iter().find(|e| e.key == "latency")
        .ok_or("entry not found")?;
    match entry.value {
        MetricValue::Histogram { min, max, sum, count }
            if min == 5.0 && max == 20.0 && sum == 35.0 && count == 3 => Ok(()),
        MetricValue::Histogram { .. } => Err("histogram values wrong"),
        _ => Err("wrong metric type"),
    }
}

pub fn buffer_full_evict() -> Result<(), &'static str> {
    let mut buf: MetricsBuffer<2> = MetricsBuffer::new();
    buf.increment("a", 1, 0).map_err(|_| "increment a failed")?;
    buf.increment("b", 2, 0).map_err(|_| "increment b failed")?;
    buf.increment("c", 3, 0).map_err(|_| "increment c failed")?;

    if buf.len() != 2 {
        return Err("expected 2 entries after eviction");
    }
    if buf.iter().any(|e| e.key == "a") {
        return Err("entry 'a' should have been evicted");
    }
    Ok(())
}
