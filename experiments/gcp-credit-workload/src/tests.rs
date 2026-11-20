use super::*;

const NOW: u64 = 1_800_000_000;

fn receipt(rows: &[String]) -> String {
    format!("observed_at\t{NOW}\n{HEADER}\n{}\n", rows.join("\n"))
}

fn lane(id: &str) -> String {
    format!("{id}\trunning\towner\tagent-lane\twork-1\tsha256:abc\tpilot\t{NOW}\t{}\ttrue\t4\ttrue\t8\t0.25\t0.10\t1\t20\t3", NOW + 3600)
}

#[test]
fn default_off_never_opens_an_input() {
    let mut out = Vec::new();
    assert!(run(&["/does/not/exist".into()], NOW, &mut out)
        .unwrap_err()
        .contains("disabled"));
    assert!(out.is_empty());
}

#[test]
fn four_lane_daily_receipt_keeps_monthly_plan_and_storage_separate() {
    let report = audit(
        &receipt(&(0..4).map(|i| lane(&i.to_string())).collect::<Vec<_>>()),
        NOW,
    )
    .unwrap();
    assert!((report.compute - 2.368).abs() < 1e-9);
    assert_eq!(
        (report.storage, report.egress, report.completed),
        (1.0, 0.4, 12)
    );
    assert!(report.findings.is_empty());
    let mut out = Vec::new();
    render(&report, &mut out).unwrap();
    let out = String::from_utf8(out).unwrap();
    assert!(out.contains("planned_monthly_lane_compute_usd\t52.10\n"));
    assert!(out.contains("scale_authorized\tfalse\n"));
}

#[test]
fn stopped_resources_still_count_prior_running_hours_and_disk_costs() {
    let report = audit(
        &receipt(&[lane("stopped").replacen("\trunning\t", "\tstopped\t", 1)]),
        NOW,
    )
    .unwrap();
    assert!((report.compute - 0.592).abs() < 1e-9);
    assert_eq!(report.storage, 0.25);
}

#[test]
fn missing_owner_expired_ttl_and_disabled_autostop_fail_pilot() {
    let row = lane("bad")
        .replace("\towner\t", "\t\t")
        .replace(&(NOW + 3600).to_string(), &(NOW - 1).to_string())
        .replacen("\ttrue\t", "\tfalse\t", 1);
    let report = audit(&receipt(&[row]), NOW).unwrap();
    assert_eq!(report.findings.len(), 3);
}

#[test]
fn orphaned_disk_is_not_a_clean_pilot() {
    let row = format!("disk\torphaned-disk\towner\tcleanup\twork-1\tdigest\tpilot\t{NOW}\t{NOW}\tfalse\t0\tfalse\t0\t1.50\t0\t0\t0\t0");
    let report = audit(&receipt(&[row]), NOW).unwrap();
    assert_eq!(report.storage, 1.5);
    assert_eq!(report.compute, 0.0);
    assert_eq!(report.findings.len(), 1);
}

#[test]
fn ttl_is_measured_from_task_start_not_observation() {
    let row = lane("long-task").replace(
        &format!("\tpilot\t{NOW}\t"),
        &format!("\tpilot\t{}\t", NOW - 12 * 3600),
    );
    let report = audit(&receipt(&[row]), NOW).unwrap();
    assert_eq!(report.findings, ["long-task: task TTL exceeds 12 hours"]);
}

#[test]
fn malformed_boolean_and_counter_overflow_are_errors() {
    assert!(audit(
        &receipt(&[lane("bad").replacen("\ttrue\t", "\tyes\t", 1)]),
        NOW
    )
    .is_err());
    let huge = lane("huge").replace("\t1\t20\t", &format!("\t{}\t20\t", u64::MAX));
    assert!(audit(&receipt(&[huge, lane("second")]), NOW).is_err());
}

#[test]
fn stale_future_empty_duplicate_and_nonfinite_receipts_are_rejected() {
    let valid = receipt(&[lane("one")]);
    assert!(audit(&valid, NOW + DAY + 1).is_err());
    assert!(audit(&valid, NOW - 1).is_err());
    assert!(audit(&receipt(&[]), NOW).is_err());
    assert!(audit(&receipt(&[lane("one"), lane("one")]), NOW).is_err());
    for bad in ["NaN", "inf", "-1", "25"] {
        assert!(audit(&valid.replace("\t8\t", &format!("\t{bad}\t")), NOW).is_err());
    }
}

#[test]
fn opt_in_cli_reads_receipt_and_returns_findings_status() {
    let path =
        std::env::temp_dir().join(format!("credit-workload-audit-{}.tsv", std::process::id()));
    let args = ["--enable-experiment".into(), path.to_str().unwrap().into()];
    std::fs::write(&path, receipt(&[lane("one")])).unwrap();
    let mut out = Vec::new();
    assert_eq!(run(&args, NOW, &mut out).unwrap(), 0);
    std::fs::write(&path, receipt(&[lane("one").replace("\towner\t", "\t\t")])).unwrap();
    assert_eq!(run(&args, NOW, &mut Vec::new()).unwrap(), 1);
    std::fs::remove_file(path).unwrap();
}
