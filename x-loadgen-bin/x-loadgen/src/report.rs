//! Static HTML report: professional dashboard + `ECharts` (offline vendor).
//! UI strings support zh/en toggle in the browser (and default via report.lang).

use crate::config::MetricId;
use crate::metrics::ReportSummary;
use anyhow::{Context, Result};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

pub fn write_report(
    dir: &Path,
    summary: &ReportSummary,
    echarts_src: &Path,
    default_lang: &str,
) -> Result<(PathBuf, PathBuf)> {
    fs::create_dir_all(dir).with_context(|| format!("mkdir {}", dir.display()))?;
    let data_dir = dir.join("data");
    fs::create_dir_all(&data_dir)?;
    let assets_dir = dir.join("assets");
    fs::create_dir_all(&assets_dir)?;

    if echarts_src.is_file() {
        fs::copy(echarts_src, assets_dir.join("echarts.min.js"))
            .with_context(|| format!("copy echarts from {}", echarts_src.display()))?;
    } else {
        anyhow::bail!(
            "ECharts vendor missing: {} (expected under x-loadgen-bin/x-loadgen/assets/)",
            echarts_src.display()
        );
    }

    let summary_path = data_dir.join("summary.json");
    fs::write(
        &summary_path,
        serde_json::to_string_pretty(summary).context("serialize summary")?,
    )?;

    let lang = normalize_lang(default_lang);
    let html_path = dir.join("index.html");
    fs::write(&html_path, render_html(summary, lang))?;

    Ok((html_path, summary_path))
}

fn normalize_lang(lang: &str) -> &'static str {
    match lang.trim().to_ascii_lowercase().as_str() {
        "en" | "en-us" | "en_us" => "en",
        _ => "zh",
    }
}

fn has(s: &ReportSummary, id: MetricId) -> bool {
    s.metrics.iter().any(|m| m == id.as_str())
}

fn stats_rows(s: &ReportSummary) -> String {
    s
        .labels
        .iter()
        .fold(String::new(), |mut acc, l| {
            let _ = write!(
                acc,
                "<tr>\
<td>{label}</td><td class=\"mono\">{pid}</td>\
<td class=\"num\">{n}</td><td class=\"num\">{ok}</td><td class=\"num\">{fail}</td><td class=\"num\">{exp}</td>\
<td class=\"num\">{err:.2}</td>\
<td class=\"num\">{avg:.1}</td><td class=\"num\">{p50}</td><td class=\"num\">{p90}</td>\
<td class=\"num\">{p95}</td><td class=\"num\">{p99}</td><td class=\"num\">{max}</td>\
<td class=\"num\">{rps:.1}</td>\
</tr>",
                label = esc(&l.label),
                pid = esc(&l.product_id),
                n = l.samples,
                ok = l.success,
                fail = l.failure,
                exp = l.expected,
                err = l.error_pct,
                avg = l.latency.mean,
                p50 = l.latency.p50,
                p90 = l.latency.p90,
                p95 = l.latency.p95,
                p99 = l.latency.p99,
                max = l.latency.max,
                rps = l.throughput_rps,
            );
            acc
        })
}

fn total_row(s: &ReportSummary) -> String {
    format!(
        "<tr class=\"total\">\
<td colspan=\"2\"><strong data-i18n=\"total\">Total</strong></td>\
<td class=\"num\">{n}</td><td class=\"num\">{ok}</td><td class=\"num\">{fail}</td><td class=\"num\">{exp}</td>\
<td class=\"num\">{err:.2}</td>\
<td class=\"num\">{avg:.1}</td><td class=\"num\">{p50}</td><td class=\"num\">{p90}</td>\
<td class=\"num\">{p95}</td><td class=\"num\">{p99}</td><td class=\"num\">{max}</td>\
<td class=\"num\">{rps:.1}</td>\
</tr>",
        n = s.overview.samples,
        ok = s.overview.success,
        fail = s.overview.failure,
        exp = s.overview.expected,
        err = s.overview.error_rate * 100.0,
        avg = s.latency.mean,
        p50 = s.latency.p50,
        p90 = s.latency.p90,
        p95 = s.latency.p95,
        p99 = s.latency.p99,
        max = s.latency.max,
        rps = s.overview.avg_rps,
    )
}

fn error_rows(s: &ReportSummary) -> String {
    s
        .errors
        .rows
        .iter()
        .fold(String::new(), |mut acc, r| {
            let _ = write!(
                acc,
                "<tr><td>{}</td><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{:.2}%</td><td class=\"num\">{:.2}%</td></tr>",
                esc(&r.key),
                esc(&r.kind),
                r.count,
                r.pct_of_samples,
                r.pct_of_errors
            );
            acc
        })
}

fn error_example_rows(s: &ReportSummary) -> String {
    s
        .errors
        .examples
        .iter()
        .take(25)
        .fold(String::new(), |mut acc, e| {
            let _ = write!(
                acc,
                "<tr><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td>\
<td>{}</td><td class=\"mono\">{}</td><td class=\"msg\">{}</td></tr>",
                esc(&e.label),
                e.request_index,
                e.status,
                e.latency_ms,
                esc(e.reason.as_deref().unwrap_or("—")),
                esc(e.request_id.as_deref().unwrap_or("—")),
                esc(e.message.as_deref().unwrap_or("")),
            );
            acc
        })
}

fn errors_section(s: &ReportSummary) -> String {
    let err_rows = error_rows(s);
    let ex_rows = error_example_rows(s);
    format!(
        r#"<section class="panel">
  <h2 data-i18n="errors">Errors</h2>
  <div class="table-wrap">
  <table>
    <thead><tr>
      <th data-i18n="col_key">Key</th>
      <th data-i18n="col_type">Type</th>
      <th class="num" data-i18n="col_count">Count</th>
      <th class="num" data-i18n="col_pct_samples">% samples</th>
      <th class="num" data-i18n="col_pct_errors">% errors</th>
    </tr></thead>
    <tbody>{err_rows}</tbody>
  </table>
  </div>
  <h3 data-i18n="samples">Samples</h3>
  <div class="table-wrap">
  <table class="compact">
    <thead><tr>
      <th data-i18n="col_label">Label</th>
      <th class="num">#</th>
      <th data-i18n="col_status">Status</th>
      <th class="num">ms</th>
      <th data-i18n="col_reason">Reason</th>
      <th data-i18n="col_req_id">Request id</th>
      <th data-i18n="col_message">Message</th>
    </tr></thead>
    <tbody>{ex_rows}</tbody>
  </table>
  </div>
</section>"#,
        err_rows = if err_rows.is_empty() {
            "<tr><td colspan=\"5\" class=\"empty\" data-i18n=\"no_errors\">No errors recorded</td></tr>"
                    .into()
        } else {
            err_rows
        },
        ex_rows = if ex_rows.is_empty() {
            "<tr><td colspan=\"7\" class=\"empty\" data-i18n=\"no_error_samples\">No unexpected error samples</td></tr>"
                    .into()
        } else {
            ex_rows
        },
    )
}

fn lat_global_section(s: &ReportSummary) -> String {
    format!(
        r#"<section class="panel">
  <h2 data-i18n="latency_all">Latency (all samples)</h2>
  <div class="table-wrap">
  <table>
    <thead><tr>
      <th data-i18n="col_count">Count</th>
      <th data-i18n="col_min">Min</th>
      <th data-i18n="col_mean">Mean</th>
      <th>p50</th><th>p90</th><th>p95</th><th>p99</th>
      <th data-i18n="col_max">Max</th>
    </tr></thead>
    <tbody><tr class="numline">
      <td class="num">{c}</td><td class="num">{min}</td><td class="num">{mean:.1}</td>
      <td class="num">{p50}</td><td class="num">{p90}</td><td class="num">{p95}</td>
      <td class="num">{p99}</td><td class="num">{max}</td>
    </tr></tbody>
  </table>
  </div>
</section>"#,
        c = s.latency.count,
        min = s.latency.min,
        mean = s.latency.mean,
        p50 = s.latency.p50,
        p90 = s.latency.p90,
        p95 = s.latency.p95,
        p99 = s.latency.p99,
        max = s.latency.max,
    )
}

fn chart_values(s: &ReportSummary) -> Vec<(&'static str, String)> {
    let show_ts = has(s, MetricId::Throughput) || has(s, MetricId::Latency);
    let show_dist = has(s, MetricId::Latency) && !s.distribution.is_empty();
    let show_codes = has(s, MetricId::HttpStatus) && !s.status_codes.is_empty();
    let show_reasons = (has(s, MetricId::ErrorReason) || has(s, MetricId::ReasonTimeseries))
        && !s.reasons.is_empty();
    let show_lat_table = has(s, MetricId::Latency);
    let chart_throughput = if show_ts {
        r#"<section class="panel"><h2 data-i18n="chart_throughput">Throughput</h2><div id="chart-rps" class="chart"></div><p class="axis-caption" data-i18n="axis_time">Time from start (s)</p></section>"#
    } else {
        ""
    };
    let chart_lat = if show_ts && show_lat_table {
        r#"<section class="panel"><h2 data-i18n="chart_latency">Response time percentiles</h2><div id="chart-lat" class="chart"></div><p class="axis-caption" data-i18n="axis_time">Time from start (s)</p></section>"#
    } else {
        ""
    };
    let chart_dist = if show_dist {
        r#"<section class="panel"><h2 data-i18n="chart_dist">Response time distribution</h2><div id="chart-dist" class="chart"></div></section>"#
    } else {
        ""
    };
    // Full-width panels only (no half-width row): screenshots /tmp/1.png called out partial-width charts.
    let chart_codes = if show_codes {
        r#"<section class="panel"><h2 data-i18n="chart_status">HTTP status</h2><div id="chart-codes" class="chart"></div></section>"#
    } else {
        ""
    };
    let chart_reasons = if show_reasons {
        r#"<section class="panel"><h2 data-i18n="chart_reasons">Reasons</h2><div id="chart-reasons" class="chart"></div></section>"#
    } else {
        ""
    };
    let chart_reason_ts = if has(s, MetricId::ReasonTimeseries) && show_reasons {
        r#"<section class="panel">
  <h2 data-i18n="chart_reason_ts">Reasons over time</h2>
  <p class="chart-hint" data-i18n="chart_zoom_hint">Drag the time range bar under the chart (or scroll on the plot) to zoom. Labels are seconds from run start.</p>
  <div id="chart-reason-ts" class="chart tall"></div>
  <p class="axis-caption" data-i18n="axis_time">Time from start (s)</p>
</section>"#
    } else {
        ""
    };

    vec![
        ("chart_throughput", chart_throughput.to_string()),
        ("chart_lat", chart_lat.to_string()),
        ("chart_dist", chart_dist.to_string()),
        ("chart_codes", chart_codes.to_string()),
        ("chart_reasons", chart_reasons.to_string()),
        ("chart_reason_ts", chart_reason_ts.to_string()),
    ]
}

fn render_html(s: &ReportSummary, default_lang: &str) -> String {
    let data_json = serde_json::to_string(s)
        .unwrap_or_else(|_| "{}".into())
        .replace("</", "<\\/");

    let show_codes = has(s, MetricId::HttpStatus) && !s.status_codes.is_empty();
    let show_reasons = (has(s, MetricId::ErrorReason) || has(s, MetricId::ReasonTimeseries))
        && !s.reasons.is_empty();
    let show_errors = has(s, MetricId::FailuresSample) || show_reasons || show_codes;
    let show_lat_table = has(s, MetricId::Latency);

    let verdict_class = match s.verdict.status.as_str() {
        "PASS" => "pass",
        "FAIL" => "fail",
        _ => "warn",
    };

    let notes: String = s.verdict.notes.iter().fold(String::new(), |mut acc, n| {
        let _ = write!(acc, "<li>{}</li>", esc(n));
        acc
    });

    let ctx = &s.run_context;
    let qps = ctx.qps.map_or_else(|| "—".into(), |q| q.to_string());

    let stats_rows = stats_rows(s);

    let total_row = total_row(s);

    let errors_section = if show_errors {
        errors_section(s)
    } else {
        String::new()
    };

    let lat_global = if show_lat_table {
        lat_global_section(s)
    } else {
        String::new()
    };

    let mut values = vec![
        (
            "html_lang",
            (if default_lang == "en" { "en" } else { "zh-CN" }).to_string(),
        ),
        ("title", esc(&s.title)),
        ("vclass", verdict_class.to_string()),
        ("vstatus", esc(&s.verdict.status)),
        ("run_id", esc(&s.run_id)),
        ("duration", format!("{:.2}", s.duration_s)),
        ("profile", esc(&s.profile)),
        ("notes", notes),
        ("samples", s.overview.samples.to_string()),
        ("failed", s.overview.failure.to_string()),
        ("err", format!("{:.2}", s.overview.error_rate * 100.0)),
        (
            "ok",
            format!(
                "{:.2}",
                if s.expected_reasons.is_empty() {
                    s.overview.business_ok_rate * 100.0
                } else {
                    s.overview.effective_ok_rate * 100.0
                }
            ),
        ),
        ("rps", format!("{:.1}", s.overview.avg_rps)),
        ("peak", format!("{:.0}", s.overview.peak_rps)),
        ("p95", s.latency.p95.to_string()),
        ("exp", s.overview.expected.to_string()),
        ("stats_rows", stats_rows),
        ("total_row", total_row),
        ("lat_global", lat_global),
        ("errors_section", errors_section),
        ("gateway", esc(&ctx.gateway)),
        ("enc", esc(&ctx.encryption)),
        ("conc", ctx.concurrency.to_string()),
        ("rpp", ctx.requests_per_product.to_string()),
        ("qps", esc(&qps)),
        ("nprod", ctx.products.to_string()),
        ("tool", esc(&ctx.tool)),
        ("data_json", data_json),
        ("default_lang", default_lang.to_string()),
    ];
    values.extend(chart_values(s));
    fill_template(REPORT_TEMPLATE, &values)
}

const REPORT_TEMPLATE: &str = include_str!("../assets/report.html");

/// 将模板中的 `@@name@@` 占位符替换为对应值（单遍扫描，值内容不会被再次展开）。
fn fill_template(template: &str, values: &[(&str, String)]) -> String {
    let mut out = String::with_capacity(template.len() + 4096);
    let mut rest = template;
    while let Some(start) = rest.find("@@") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let resolved = after.find("@@").and_then(|end| {
            let key = &after[..end];
            values
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| (v, end))
        });
        if let Some((v, end)) = resolved {
            out.push_str(v);
            rest = &after[end + 2..];
        } else {
            out.push_str("@@");
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::*;
    use std::path::PathBuf;

    fn sample_label(
        name: &str,
        pid: &str,
        mean: f64,
        p50: u128,
        p95: u128,
        p99: u128,
        max: u128,
    ) -> LabelStats {
        LabelStats {
            label: name.into(),
            product_id: pid.into(),
            samples: 30000,
            success: 30000,
            failure: 0,
            expected: 0,
            error_pct: 0.0,
            effective_error_pct: 0.0,
            throughput_rps: 26.8,
            received_bytes: 1_000_000,
            latency: TrendStats {
                count: 30000,
                min: 20,
                max,
                mean,
                p50,
                p90: p95,
                p95,
                p99,
            },
        }
    }

    fn sample_timeseries() -> Vec<TimePoint> {
        let mut ts = Vec::new();
        for i in 0..40 {
            ts.push(TimePoint {
                t: 1_700_000_000 + i,
                t_rel: i * 5,
                rps: 70.0 + mazbri_core::num::u64_to_f64(i % 7) * 3.0,
                success: 70,
                failure: 0,
                expected: 0,
                bytes: 10000,
                p50: 80 + u128::from(i),
                p95: 200 + u128::from(i) * 2,
                p99: 400 + u128::from(i) * 3,
                status: vec![CountRow {
                    name: "200".into(),
                    count: 70,
                }],
                reasons: vec![],
            });
        }
        ts
    }

    fn sample_summary() -> ReportSummary {
        let lat = TrendStats {
            count: 90000,
            min: 23,
            max: 4150,
            mean: 186.1,
            p50: 181,
            p90: 237,
            p95: 390,
            p99: 730,
        };
        ReportSummary {
            schema_version: 1,
            run_id: "preview-theme".into(),
            title: "基线压测 83C4JT39 — 报告样式预览".into(),
            profile: "standard".into(),
            metrics: vec![
                "latency".into(),
                "throughput".into(),
                "http_status".into(),
                "error_reason".into(),
                "failures_sample".into(),
            ],
            started_unix_ms: 1_700_000_000_000,
            ended_unix_ms: 1_700_001_120_000,
            duration_s: 1120.43,
            overview: Overview {
                samples: 90000,
                success: 90000,
                failure: 0,
                expected: 0,
                business_ok_rate: 1.0,
                effective_ok_rate: 1.0,
                error_rate: 0.0,
                avg_rps: 80.3,
                peak_rps: 185.0,
                received_kb: 4096.0,
            },
            latency: lat,
            labels: vec![
                sample_label("Sample A", "8000000001", 86.6, 86, 123, 133, 319),
                sample_label("Sample B", "8000000002", 225.5, 226, 236, 241, 428),
                sample_label("Sample C", "8000000003", 246.1, 181, 598, 1076, 4150),
            ],
            timeseries: sample_timeseries(),
            distribution: [
                (50, 2000, "≤50"),
                (100, 19000, "≤100"),
                (200, 28000, "≤200"),
                (300, 35000, "≤300"),
                (500, 4000, "≤500"),
                (1000, 1500, "≤1000"),
                (u64::MAX, 500, ">10000"),
            ]
            .into_iter()
            .map(|(le_ms, count, label)| HistBin {
                le_ms,
                count,
                label: label.into(),
            })
            .collect(),
            errors: ErrorCatalog {
                rows: vec![],
                examples: vec![],
            },
            status_codes: vec![CountRow {
                name: "200".into(),
                count: 90000,
            }],
            reasons: vec![],
            verdict: Verdict {
                status: "PASS".into(),
                notes: vec!["No SLA thresholds configured".into()],
            },
            run_context: RunContext {
                tool: format!("x-loadgen/{}", env!("CARGO_PKG_VERSION")),
                gateway: "https://gateway.example.com".into(),
                encryption: "none".into(),
                concurrency: 15,
                requests_per_product: 30000,
                qps: None,
                products: 3,
                labels: vec!["Sample A".into(), "Sample B".into(), "Sample C".into()],
            },
            expected_reasons: vec![],
        }
    }

    #[test]
    fn preview_report_html() {
        let summary = sample_summary();
        let out = PathBuf::from("/tmp/loadgen-report-preview");
        let echarts = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/echarts.min.js");
        let (html, _) = write_report(&out, &summary, &echarts, "zh").expect("write preview");
        assert!(html.is_file());
        let body = std::fs::read_to_string(&html).unwrap();
        assert!(!body.contains("theme-simple"));
        assert!(!body.contains("theme-pro"));
        assert!(!body.contains("theme-switch"));
        assert!(body.contains("pro-strip"));
        assert!(body.contains("linear-gradient(135deg, #0b3d91"));
        assert!(body.contains("tip_avg_rps"));
        assert!(body.contains("id=\"chart-codes\""));
        assert!(!body.contains("panel half"));
        assert!(!body.contains("class=\"row\""));
        eprintln!("preview: {}", html.display());
    }
}
