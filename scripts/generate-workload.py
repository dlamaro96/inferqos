#!/usr/bin/env python3
"""Deterministic prompt-free workload generator for tests, demos, and replay."""
import argparse, json, math, random
p=argparse.ArgumentParser();p.add_argument("--pattern",choices=["constant","bursty","diurnal","mixed","pathological","spike"],default="mixed");p.add_argument("--events",type=int,default=1000);p.add_argument("--seed",type=int,default=7);p.add_argument("--output",default="workload.jsonl");a=p.parse_args();r=random.Random(a.seed)
with open(a.output,"w",encoding="utf-8") as f:
  for i in range(a.events):
    base={"constant":1,"bursty":1+8*(i%100<10),"diurnal":1+math.sin(i/100*math.pi),"mixed":1+r.random()*3,"pathological":12 if i%7==0 else 1,"spike":20 if 450<i<520 else 1}[a.pattern]
    cls=r.choices(["interactive","workflow","batch"],[3,2,5])[0];tokens=int(base*r.choice([64,256,2048,32768]));event={"timestamp_ms":i*100,"provider":"fake","model":"fake","pool":"primary","application":"demo","tenant":f"tenant-{i%10}","service_class":cls,"input_tokens":tokens,"cached_tokens":tokens//8,"requested_max_output":max(32,tokens//4),"actual_output":max(16,tokens//8),"latency_ms":50+tokens//100,"status":429 if base>10 and i%3==0 else 200,"retry_after_ms":1000,"throttled":base>10 and i%3==0};f.write(json.dumps(event,separators=(",",":"))+"\n")

