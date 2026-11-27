"""Compare the Rust progress tracker against a local Discord desktop release."""
import json
import math
import os
from pathlib import Path
import random
import struct
import subprocess
import sys

project = Path(__file__).resolve().parent
archive = Path(sys.argv[1]) if len(sys.argv) > 1 else max(
    (Path(os.environ["LOCALAPPDATA"]) / "Discord").glob("app-*/resources/app.asar"),
    key=lambda p: p.stat().st_mtime,
)
with archive.open("rb") as src:
    _, header, _, length = struct.unpack("<4I", src.read(16))
    entry = json.loads(src.read(length))["files"]["splashScreenPreload.js"]
    src.seek(8 + header + int(entry["offset"]))
    bundle = src.read(entry["size"]).decode()
# Only the isolated updater class and its two local dependencies are evaluated.
start = bundle.index("class TaskProgress")
stop = bundle.index("exports.SplashUpdateProgress=SplashUpdateProgress", start)
reference = bundle[start:stop] + """
const tracker = new SplashUpdateProgress();
for (const line of require('fs').readFileSync(0,'utf8').trim().split('\u005cn')) {
  const command = line.trim(); if (command === 'finish') tracker.finish();
  else if (command !== 'snapshot') tracker.record(JSON.parse(line));
  console.log(JSON.stringify(tracker.snapshot()));
}
"""
rng = random.Random(42)
lines = ["snapshot", "finish"]
for i in range(150):
    pkg = rng.randrange(12)
    task = {"version": str(pkg), "package_sha256": f"package-{pkg}"}
    if pkg != 11:
        task["package_size"] = 10 + pkg * 13
    kind = rng.choice(["ModuleDownload", "HostDownload", "ModuleInstall", "HostInstall"])
    event = {"task": {kind: task}, "state": rng.choice(["Pending", "Working", "Complete"]),
             "percent": rng.choice([None, -5, 0, 35, 85, 100, 140])}
    lines.append(json.dumps(event))
lines.append("finish")
data = "\n".join(lines) + "\n"

def run(cmd):
    return [json.loads(line) for line in subprocess.check_output(
        cmd, cwd=project, input=data, text=True, encoding="utf8").splitlines()]

actual = run(["cargo", "run", "--quiet", "--locked"])
expected = run(["node", "-e", reference])
assert len(expected) == len(actual) == len(lines)
for step, (left, right) in enumerate(zip(actual, expected)):
    for phase in ("downloads", "installs"):
        for key, value in right[phase].items():
            got = left[phase][key]
            assert (math.isclose(got, value) if key == "progress" else got == value), (step, phase, key, got, value)
print(f"{len(lines)} snapshots matched: {archive}")

# Compare the rendering data separately because IPC also supplies legacy states.
states = [{}, {"status":"launching"}, {"status":"update-failure","seconds":3}]
for status in ("installing-updates","downloading-updates"):
    for current in (None,0,1):
        state = {"status":status,"total":3}
        if current is not None: state["current"]=current
        states.append(state)
    for total in (0,3):
        for complete in (False,True):
            for install_progress in (10,60):
                states.append({"status":status,"downloads":{"current":1,"total":total,"complete":complete,"progress":40},
                               "installs":{"current":0,"total":total,"progress":install_progress}})
functions = bundle[bundle.index("exports.getStatusText=function"):bundle.index(";class TaskProgress")]
display_reference = "const source={};(function(exports){"+functions+"})(source);" + """
for (const state of JSON.parse(require('fs').readFileSync(0, 'utf8'))) {
  console.log(JSON.stringify({text:source.getStatusText(state),fills:source.getProgressFills(state)}));
}
"""
data = json.dumps(states)
expected = run(["node","-e",display_reference])
data = "\n".join(map(json.dumps,states))+"\n"
assert run(["cargo","run","--quiet","--locked","--","--display"]) == expected
print(f"{len(states)} display states matched")
