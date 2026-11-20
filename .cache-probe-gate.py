import sys
sys.path.insert(0, "tests")
import test_ci_fork_runner_routing as t

E = t.GATE_TRUSTED_RUNNER_EXPRESSION
for name, value in (("fleet", E.replace(t.GATE_JIT_LABEL, "cmux-fleet-linux-x64")),
                    ("lane", E.replace(t.GATE_JIT_LABEL, "cmux-gcp-lane-x64")),
                    ("var", E.replace("'" + t.GATE_JIT_LABEL + "'", "vars.GATE_RUNNER")),
                    ("bare", t.GATE_JIT_LABEL)):
    print(name, t.runner_label_errors(value))
