# Credit workload receipt experiment

Default off. This standalone Rust experiment audits a daily inventory exported
by the fleet owner. It reads one file and prints a report; it never contacts a
provider, provisions or deletes resources, changes a setting, or authorizes
scale. It is not integrated into the app's Labs settings yet.

The first slice implements the internal workload plan's pilot receipt check.
The existing controller remains responsible for admission, creation, draining,
and cleanup. Billing and regional quota receipts remain separate requirements.
A clean audit means only that this supplied inventory passed these checks; it
does not prove that the inventory is complete or independently verified.

## Explicit opt-in

Build and test on the managed Linux pool through `cmux-ci`, not on an agent Mac:

```sh
cargo test --locked --manifest-path experiments/gcp-credit-workload/Cargo.toml
cargo run --locked --manifest-path experiments/gcp-credit-workload/Cargo.toml -- \
  --enable-experiment /path/to/daily-inventory.tsv
```

Without `--enable-experiment`, the command exits 2 before opening an inventory.
Exit 0 means this receipt has no findings; 1 means an owner needs to address
findings; 2 means disabled, invalid input, or an I/O error. Output is TSV.
`scale_authorized` is always false. No background job or scheduled invocation
is installed.

## Inventory v1

The first line is `observed_at<TAB>UNIX_SECONDS`. The second is this exact header
(replace each `<TAB>` with a tab):

```text
id<TAB>state<TAB>owner<TAB>purpose<TAB>workload<TAB>image_digest<TAB>cost_center<TAB>task_started_at<TAB>expires_at<TAB>idle_autostop<TAB>vcpus<TAB>spot<TAB>running_hours<TAB>storage_usd<TAB>egress_usd<TAB>preemptions<TAB>queue_seconds<TAB>completed_work
```

One row per VM or orphaned disk. IDs must be unique across the inventory.
`state` is `running`, `stopped`, or `orphaned-disk`; booleans are `true`/`false`.
Times are Unix seconds. Numeric fields cannot be blank, negative, or nonfinite.
Counters and vCPUs are integers. Text cannot contain tabs, newlines, or other
control characters. Input is bounded to 1 MiB and 4096 resources.

Counters, running hours, storage dollars, and egress dollars cover the same
24-hour period ending at `observed_at`. A stopped VM can have running hours
earlier in that period and still incurs storage cost. An orphaned disk has
zero vCPUs and running hours. Include it even if ownership labels are missing.
Do not replace unknown billing data with zero: collect the missing receipt
before auditing. Future-dated, stale (over 24 hours), and empty inventories
are rejected.

Every VM needs owner, purpose, workload, image digest, cost center, an unexpired
task TTL of at most 12 hours, and idle autostop. This first pilot requires
autostop; it does not adjudicate exceptions. Orphaned disks always produce a
finding. An owner must inspect findings and perform cleanup through the
existing controller; this tool never deletes anything.

## Interpreting costs

Compute is an **estimate**, using the plan's $1.48/32 = $0.04625 per vCPU-hour
and a 60% spot discount. Storage and egress are the supplied report amounts,
shown separately. These are not live prices or an independently read bill.

Four 4-vCPU spot lanes running 176 hours each estimate **$52.10/month in
compute**, within the plan's **$70/month envelope** including its small
storage/log allowance. This monthly scenario is separate from the observed
daily totals. The design supplies planning numbers, not a measured before/after
performance baseline; this experiment makes no speed or savings claim.

The report also reconciles running hours, preemptions, cumulative queue seconds,
and completed work. It cannot establish the plan's week of utilization and
recovery evidence, credit expiry/balance, budget alerts, regional quotas, or
owner approval for a 128/256-vCPU expansion.
