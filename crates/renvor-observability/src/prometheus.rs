//! Prometheus text exposition (format version 0.0.4) over the kernel's metrics snapshot (FR-072).
//!
//! The port is `renvor_core::observe::metrics`; this is only a renderer over its
//! [`Snapshot`], so metrics are exportable without OpenTelemetry and without a scrape library in
//! the graph. Names and label names are the closed set the registry accepted; label values are
//! escaped as the format requires (`\\`, `"`, `\n`).
//!
//! The renderer is cross-checked against `prometheus-client`'s encoder in a dev-only differential
//! test: both outputs are parsed into `(name, labels, value)` samples and compared numerically,
//! because the two text formats differ in decoration (OpenMetrics adds `# EOF` and suffix rules)
//! and agree in content.

use core::fmt::Write as _;

use renvor_core::observe::metrics::{
    FamilySnapshot, InstrumentKind, Series, SeriesValue, Snapshot,
};

/// Renders `snapshot` as Prometheus text.
#[must_use]
pub fn render(snapshot: &Snapshot) -> String {
    let mut out = String::new();
    for family in &snapshot.families {
        render_family(&mut out, family);
    }
    out
}

fn kind_name(kind: InstrumentKind) -> &'static str {
    match kind {
        InstrumentKind::Counter => "counter",
        InstrumentKind::Gauge => "gauge",
        InstrumentKind::Histogram => "histogram",
    }
}

fn escape_help(text: &str) -> String {
    text.replace('\\', "\\\\").replace('\n', "\\n")
}

fn escape_label(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn labels(series: &Series, extra: Option<(&str, &str)>) -> String {
    let mut pairs: Vec<String> = series
        .labels
        .iter()
        .map(|(name, value)| format!("{name}=\"{}\"", escape_label(value)))
        .collect();
    if let Some((name, value)) = extra {
        pairs.push(format!("{name}=\"{value}\""));
    }
    if pairs.is_empty() {
        String::new()
    } else {
        format!("{{{}}}", pairs.join(","))
    }
}

fn number(value: f64) -> String {
    if value.is_infinite() {
        if value > 0.0 {
            "+Inf".to_owned()
        } else {
            "-Inf".to_owned()
        }
    } else if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

fn render_family(out: &mut String, family: &FamilySnapshot) {
    let _ = writeln!(out, "# HELP {} {}", family.name, escape_help(family.help));
    let _ = writeln!(out, "# TYPE {} {}", family.name, kind_name(family.kind));
    for series in &family.series {
        match &series.value {
            SeriesValue::Scalar(value) => {
                let _ = writeln!(
                    out,
                    "{}{} {}",
                    family.name,
                    labels(series, None),
                    number(*value)
                );
            }
            SeriesValue::Histogram {
                bounds,
                cumulative_counts: counts,
                sum,
                count,
            } => {
                for (bound, cumulative) in bounds.iter().zip(counts.iter()) {
                    let _ = writeln!(
                        out,
                        "{}_bucket{} {}",
                        family.name,
                        labels(series, Some(("le", &number(*bound)))),
                        cumulative
                    );
                }
                if let Some(last) = counts.get(bounds.len()) {
                    let _ = writeln!(
                        out,
                        "{}_bucket{} {}",
                        family.name,
                        labels(series, Some(("le", "+Inf"))),
                        last
                    );
                }
                let _ = writeln!(
                    out,
                    "{}_sum{} {}",
                    family.name,
                    labels(series, None),
                    number(*sum)
                );
                let _ = writeln!(
                    out,
                    "{}_count{} {}",
                    family.name,
                    labels(series, None),
                    count
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use renvor_core::observe::metrics::Registry;

    use super::render;

    /// One sample: metric name, sorted labels, value.
    type Sample = (String, BTreeMap<String, String>, f64);

    /// Parses either text format into samples, ignoring comments and `# EOF`.
    fn samples(text: &str) -> Vec<Sample> {
        let mut out = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (head, value) = line.rsplit_once(' ').expect("a sample has a value");
            let value: f64 = match value {
                "+Inf" => f64::INFINITY,
                other => other.parse().expect("a number"),
            };
            let (name, labels) = match head.split_once('{') {
                Some((name, rest)) => {
                    let rest = rest.trim_end_matches('}');
                    let mut map = BTreeMap::new();
                    // Labels are `name="value"` with `\"` and `\\` escapes; split on commas
                    // outside quotes and unescape, so both encoders' escaping is compared as
                    // the value it stands for.
                    let mut pairs = Vec::new();
                    let mut current = String::new();
                    let mut quoted = false;
                    let mut chars = rest.chars();
                    while let Some(c) = chars.next() {
                        match c {
                            '"' => {
                                quoted = !quoted;
                                current.push(c);
                            }
                            '\\' if quoted => {
                                current.push(chars.next().unwrap_or('\\'));
                            }
                            ',' if !quoted => pairs.push(std::mem::take(&mut current)),
                            _ => current.push(c),
                        }
                    }
                    if !current.is_empty() {
                        pairs.push(current);
                    }
                    for pair in pairs {
                        let (key, value) = pair.split_once('=').expect("label=value");
                        let value = value
                            .strip_prefix('"')
                            .and_then(|v| v.strip_suffix('"'))
                            .unwrap_or(value);
                        // `le` is a number both encoders may spell differently (`1` and `1.0`);
                        // compare it as the number it is.
                        let value = if key == "le" {
                            match value {
                                "+Inf" => "+Inf".to_owned(),
                                other => other
                                    .parse::<f64>()
                                    .map_or(other.to_owned(), |n| format!("{n}")),
                            }
                        } else {
                            value.to_owned()
                        };
                        map.insert(key.to_owned(), value);
                    }
                    (name.to_owned(), map)
                }
                None => (head.to_owned(), BTreeMap::new()),
            };
            out.push((name, labels, value));
        }
        out.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        out
    }

    #[test]
    fn the_renderer_agrees_with_prometheus_client_sample_for_sample() {
        // The Renvor side.
        let registry = Registry::new();
        let requests = registry
            .counter("http_requests", "Requests handled.", &["route", "status"])
            .unwrap();
        requests.increment(&[("route", "/a"), ("status", "200")], 3);
        requests.increment(&[("route", "/b\"quoted\""), ("status", "500")], 1);
        let inflight = registry.gauge("inflight", "In flight.", &[]).unwrap();
        inflight.set(&[], 2.0);
        let latency = registry
            .histogram("latency_seconds", "Latency.", &["route"], &[0.1, 0.5, 1.0])
            .unwrap();
        for value in [0.05, 0.3, 0.7, 5.0] {
            latency.observe(&[("route", "/a")], value);
        }
        let ours = render(&registry.snapshot());

        // The reference encoder, fed the same facts.
        use prometheus_client::encoding::text::encode;
        use prometheus_client::metrics::counter::Counter;
        use prometheus_client::metrics::family::Family;
        use prometheus_client::metrics::gauge::Gauge;
        use prometheus_client::metrics::histogram::Histogram;
        let mut reference = prometheus_client::registry::Registry::default();
        let counter: Family<Vec<(String, String)>, Counter> = Family::default();
        counter
            .get_or_create(&vec![
                ("route".to_owned(), "/a".to_owned()),
                ("status".to_owned(), "200".to_owned()),
            ])
            .inc_by(3);
        counter
            .get_or_create(&vec![
                ("route".to_owned(), "/b\"quoted\"".to_owned()),
                ("status".to_owned(), "500".to_owned()),
            ])
            .inc();
        reference.register("http_requests", "Requests handled.", counter);
        let gauge: Gauge = Gauge::default();
        gauge.set(2);
        reference.register("inflight", "In flight.", gauge);
        let histogram: Family<Vec<(String, String)>, Histogram> =
            Family::new_with_constructor(|| Histogram::new([0.1, 0.5, 1.0]));
        for value in [0.05, 0.3, 0.7, 5.0] {
            histogram
                .get_or_create(&vec![("route".to_owned(), "/a".to_owned())])
                .observe(value);
        }
        reference.register("latency_seconds", "Latency.", histogram);
        let mut theirs = String::new();
        encode(&mut theirs, &reference).unwrap();

        // OpenMetrics names counters `<name>_total` in samples; Renvor's family name is the
        // sample name. Normalise that one known decoration, then compare every sample.
        let theirs = theirs.replace("http_requests_total", "http_requests");
        let ours_samples = samples(&ours);
        let theirs_samples = samples(&theirs);
        assert!(!ours_samples.is_empty());
        assert_eq!(
            ours_samples.len(),
            theirs_samples.len(),
            "sample count differs: {ours}\n---\n{theirs}"
        );
        for (index, (mine, reference)) in ours_samples.iter().zip(&theirs_samples).enumerate() {
            assert_eq!(mine.0, reference.0, "sample index {index} name differs");
            assert_eq!(mine.1, reference.1, "sample index {index} labels differ");
            assert!(
                (mine.2 - reference.2).abs() < 1e-9
                    || (mine.2.is_infinite() && reference.2.is_infinite()),
                "sample index {index} value differs"
            );
        }
        assert!(ours.contains("# TYPE latency_seconds histogram"));
        assert!(ours.contains("latency_seconds_bucket{route=\"/a\",le=\"+Inf\"} 4"));
    }

    #[test]
    fn a_negative_control_proves_the_comparison_sees_a_difference() {
        let registry = Registry::new();
        let counter = registry.counter("c", "C.", &["k"]).unwrap();
        counter.increment(&[("k", "v")], 2);
        let ours = samples(&render(&registry.snapshot()));
        let altered = samples("c{k=\"v\"} 3\n");
        assert_ne!(ours, altered);
    }
}
