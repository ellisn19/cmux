//! Opt-in, read-only daily pilot receipt audit. Never calls a provider or scheduler.
use std::collections::HashSet;
use std::io::{Read, Write};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_BYTES: u64 = 1024 * 1024;
const MAX_ROWS: usize = 4096;
const DAY: u64 = 86_400;
const ON_DEMAND_VCPU_HOUR: f64 = 1.48 / 32.0;
const SPOT_FACTOR: f64 = 0.4;
const HEADER: &str = "id\tstate\towner\tpurpose\tworkload\timage_digest\tcost_center\ttask_started_at\texpires_at\tidle_autostop\tvcpus\tspot\trunning_hours\tstorage_usd\tegress_usd\tpreemptions\tqueue_seconds\tcompleted_work";

#[derive(Debug, Default)]
struct Report {
    rows: usize,
    compute: f64,
    storage: f64,
    egress: f64,
    running_hours: f64,
    preemptions: u64,
    queue_seconds: f64,
    completed: u64,
    findings: Vec<String>,
}

fn number(value: &str, field: &str) -> Result<f64, String> {
    let n: f64 = value.parse().map_err(|_| format!("invalid {field}"))?;
    if !n.is_finite() || !(0.0..=1_000_000_000.0).contains(&n) {
        return Err(format!("out of range {field}"));
    }
    Ok(n)
}

fn integer(value: &str, field: &str) -> Result<u64, String> {
    value.parse().map_err(|_| format!("invalid {field}"))
}

fn boolean(value: &str, field: &str) -> Result<bool, String> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!("invalid {field}: expected true or false")),
    }
}

fn audit(input: &str, now: u64) -> Result<Report, String> {
    let mut lines = input.lines();
    let observed = lines
        .next()
        .and_then(|s| s.strip_prefix("observed_at\t"))
        .ok_or("missing observed_at Unix timestamp")?;
    let observed = integer(observed, "observed_at")?;
    if observed > now || now - observed > DAY {
        return Err("receipt is future-dated or more than 24 hours old".into());
    }
    if lines.next() != Some(HEADER) {
        return Err("inventory header does not match the documented v1 fields".into());
    }
    let mut report = Report::default();
    let mut ids = HashSet::new();
    for line in lines {
        if report.rows == MAX_ROWS {
            return Err("inventory exceeds 4096 resources".into());
        }
        let fields: Vec<_> = line.split('\t').collect();
        if fields.len() != 18 || fields.iter().any(|s| s.chars().any(char::is_control)) {
            return Err("invalid inventory row: expected 18 tab-separated fields".into());
        }
        let id = fields[0];
        if id.trim().is_empty() || !ids.insert(id) {
            return Err("missing or duplicate resource id".into());
        }
        let state = fields[1];
        if !matches!(state, "running" | "stopped" | "orphaned-disk") {
            return Err(format!("{id}: unknown state"));
        }
        let started = integer(fields[7], "task_started_at")?;
        let expires = integer(fields[8], "expires_at")?;
        if started > observed {
            return Err(format!("{id}: task starts after observation"));
        }
        let autostop = boolean(fields[9], "idle_autostop")?;
        let vcpus = integer(fields[10], "vcpus")?;
        if (state != "orphaned-disk" && !(1..=256).contains(&vcpus))
            || (state == "orphaned-disk" && vcpus != 0)
        {
            return Err(format!("{id}: invalid vcpus for state"));
        }
        let spot = boolean(fields[11], "spot")?;
        let hours = number(fields[12], "running_hours")?;
        if hours > 24.0 || (state == "orphaned-disk" && hours != 0.0) {
            return Err(format!("{id}: invalid daily running_hours"));
        }
        for (index, name) in [
            (2, "owner"),
            (3, "purpose"),
            (4, "workload"),
            (5, "image_digest"),
            (6, "cost_center"),
        ] {
            if fields[index].trim().is_empty() {
                report.findings.push(format!("{id}: missing {name}"));
            }
        }
        if state == "orphaned-disk" {
            report
                .findings
                .push(format!("{id}: orphaned disk requires owner cleanup"));
        } else {
            if expires <= now {
                report
                    .findings
                    .push(format!("{id}: task TTL expired; owner must drain"));
            }
            if expires.saturating_sub(started) > 12 * 3600 {
                report
                    .findings
                    .push(format!("{id}: task TTL exceeds 12 hours"));
            }
            if !autostop {
                report
                    .findings
                    .push(format!("{id}: idle autostop is disabled"));
            }
        }
        report.compute +=
            hours * vcpus as f64 * ON_DEMAND_VCPU_HOUR * if spot { SPOT_FACTOR } else { 1.0 };
        report.storage += number(fields[13], "storage_usd")?;
        report.egress += number(fields[14], "egress_usd")?;
        report.preemptions = report
            .preemptions
            .checked_add(integer(fields[15], "preemptions")?)
            .ok_or("preemption count overflow")?;
        report.queue_seconds += number(fields[16], "queue_seconds")?;
        report.completed = report
            .completed
            .checked_add(integer(fields[17], "completed_work")?)
            .ok_or("completed work count overflow")?;
        report.running_hours += hours;
        report.rows += 1;
    }
    if report.rows == 0 {
        return Err("empty inventory is not pilot evidence".into());
    }
    Ok(report)
}

fn render(report: &Report, output: &mut impl Write) -> std::io::Result<()> {
    writeln!(output, "format\tcredit-workload-audit-v1")?;
    writeln!(output, "resources\t{}", report.rows)?;
    writeln!(output, "running_hours\t{:.3}", report.running_hours)?;
    writeln!(output, "estimated_compute_usd\t{:.4}", report.compute)?;
    writeln!(output, "reported_storage_usd\t{:.4}", report.storage)?;
    writeln!(output, "reported_egress_usd\t{:.4}", report.egress)?;
    writeln!(output, "spot_preemptions\t{}", report.preemptions)?;
    writeln!(output, "queue_seconds\t{:.3}", report.queue_seconds)?;
    writeln!(output, "completed_work\t{}", report.completed)?;
    // Separate the design's monthly envelope from the daily observed receipt.
    writeln!(
        output,
        "planned_monthly_lane_compute_usd\t{:.2}",
        4.0 * 4.0 * 176.0 * ON_DEMAND_VCPU_HOUR * SPOT_FACTOR
    )?;
    writeln!(output, "planned_monthly_lane_envelope_usd\t70.00")?;
    for finding in &report.findings {
        writeln!(output, "finding\t{finding}")?;
    }
    writeln!(
        output,
        "audit\t{}",
        if report.findings.is_empty() {
            "clean"
        } else {
            "needs-owner"
        }
    )?;
    writeln!(output, "scale_authorized\tfalse")
}

fn run(args: &[String], now: u64, output: &mut impl Write) -> Result<u8, String> {
    if args.first().map(String::as_str) != Some("--enable-experiment") {
        return Err(
            "experiment disabled; next: explicitly pass --enable-experiment INVENTORY.tsv".into(),
        );
    }
    if args.len() != 2 {
        return Err("next: pass --enable-experiment INVENTORY.tsv".into());
    }
    if !std::fs::metadata(&args[1])
        .map_err(|e| e.to_string())?
        .is_file()
    {
        return Err("receipt must be a regular file".into());
    }
    let file = std::fs::File::open(&args[1]).map_err(|e| format!("cannot open receipt: {e}"))?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("receipt must be a regular file".into());
    }
    let mut input = String::new();
    file.take(MAX_BYTES + 1)
        .read_to_string(&mut input)
        .map_err(|e| e.to_string())?;
    if input.len() as u64 > MAX_BYTES {
        return Err("receipt exceeds 1 MiB".into());
    }
    let report = audit(&input, now)?;
    render(&report, output).map_err(|e| e.to_string())?;
    Ok(if report.findings.is_empty() { 0 } else { 1 })
}

fn main() -> std::process::ExitCode {
    let now = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs(),
        Err(error) => {
            eprintln!("{error}");
            return 2.into();
        }
    };
    match run(
        &std::env::args().skip(1).collect::<Vec<_>>(),
        now,
        &mut std::io::stdout().lock(),
    ) {
        Ok(code) => code.into(),
        Err(error) => {
            eprintln!("{error}\nnext: correct the daily inventory using README.md, then explicitly opt in");
            2.into()
        }
    }
}

#[cfg(test)]
mod tests;
