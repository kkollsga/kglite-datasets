#!/usr/bin/env python3
"""Measure release-installed SODIR membership generation from cached CSVs."""

import argparse
import json
import shutil
import subprocess
import tempfile
import time
from pathlib import Path


MEMBERS = [
    "sodir-showcase/fresh-workdir/sodir_index.json",
    "sodir-showcase/fresh-workdir/csv/discovery.csv",
    "sodir-showcase/fresh-workdir/csv/wellbore.csv",
    "sodir-showcase/fresh-workdir/csv/play.csv",
]
BLUEPRINT = json.dumps({"nodes": {stem: {"csv": f"csv/{stem}.csv"} for stem in ("discovery", "wellbore", "play")}})
CHILD = r'''
import json,sys,time
from pathlib import Path
from kglite_datasets import _sodir_internal
workdir=Path(sys.argv[1]); blueprint=sys.argv[2]
started=time.perf_counter()
report=_sodir_internal.refresh(str(workdir),blueprint,index_cooldown_days=10000,dataset_cooldown_days=10000,concurrency=1,enhance_discovery_play=True)
elapsed=(time.perf_counter()-started)*1000
out=workdir/'csv'/'_derived_discovery_play.csv'
with out.open() as handle: links=sum(1 for _ in handle)-1
print(json.dumps({'call_wall_ms':elapsed,'links':links,'csv_bytes':out.stat().st_size,'report':report['preprocess']}))
'''


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument("--archive",type=Path,required=True)
    parser.add_argument("--python",type=Path,required=True)
    parser.add_argument("--runs",type=int,default=5)
    parser.add_argument("--out",type=Path,required=True)
    args=parser.parse_args()
    captures=[]
    with tempfile.TemporaryDirectory(prefix="sodir-membership-bench-") as temp:
        base=Path(temp)/"base"
        subprocess.run(["tar","-xf",str(args.archive),"-C",temp,*MEMBERS],check=True)
        extracted=Path(temp)/"sodir-showcase"/"fresh-workdir"
        extracted.rename(base)
        for index in range(args.runs):
            workdir=Path(temp)/f"run-{index}"
            shutil.copytree(base,workdir)
            started=time.perf_counter()
            proc=subprocess.run([str(args.python),"-c",CHILD,str(workdir),BLUEPRINT],capture_output=True,text=True,check=True)
            outer_ms=(time.perf_counter()-started)*1000
            row=json.loads(proc.stdout.strip().splitlines()[-1]); row["fresh_process_wall_ms"]=outer_ms
            captures.append(row)
    call=[r["call_wall_ms"] for r in captures]; process=[r["fresh_process_wall_ms"] for r in captures]
    result={"archive":str(args.archive),"python":str(args.python),"runs":len(captures),"statistic":"mean of fresh-process first events","call_wall_ms_mean":sum(call)/len(call),"call_wall_ms_runs":call,"fresh_process_wall_ms_mean":sum(process)/len(process),"fresh_process_wall_ms_runs":process,"links":captures[0]["links"],"edge_growth":captures[0]["links"],"node_growth":0,"csv_bytes":captures[0]["csv_bytes"],"deterministic":all((r["links"],r["csv_bytes"])==(captures[0]["links"],captures[0]["csv_bytes"]) for r in captures),"stop_rule":{"max_seconds":2.0,"passed_absolute":max(call)<2000.0}}
    args.out.write_text(json.dumps(result,indent=2)+"\n")
    print(json.dumps(result,indent=2))


if __name__ == "__main__":
    main()
