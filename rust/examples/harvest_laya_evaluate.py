"""Re-evaluate immutable training logits without inference or changing weights.

Uses empirical calibration cutpoints (v2); writes a new report, never overwrites
the original coarse-grid pilot report. Raw probabilities are rounded to four decimals
to match Laya's public inference API before threshold selection and evaluation.
"""
import argparse
import json
import math
from pathlib import Path
from harvest_laya_train import check_splits, metrics, select_thresholds


def scored(rows, predictions, temperature):
    by_id = {}
    for p in predictions:
        z = [v/temperature for v in p["logits"]]
        exp = [math.exp(v-max(z)) for v in z]
        dist = [round(v/sum(exp),4) for v in exp]
        by_id.setdefault(p["article_id"],{})[p["qid"]] = dict(zip(p["keys"],dist))
    return [dict(r,scores=by_id[r["article_id"]]) for r in rows if r["article_id"] in by_id]


def evaluate(rows, target):
    policy = select_thresholds([r for r in rows if r["split"]=="calibration"],target)
    result = {"policy":policy}
    for split in ("calibration","test"):
        selected = [r for r in rows if r["split"]==split]
        m = metrics(selected,policy["entity"],policy["reporting"])
        result[split] = {"tuned_policy":m,"fixed_policy":metrics(selected,0.55,0.5),
            "pass_everything":metrics(selected,0,0),
            "nontrivial_target_met":m["recall"] is not None and m["recall"]>=target and m["forwarded"]<m["articles"],
            "classification":{}}
        for qid in ("entity","content"):
            labelled = [r for r in selected if qid in r["labels"]]
            correct = sum(max(r["scores"][qid],key=r["scores"][qid].get)==r["labels"][qid] for r in labelled)
            result[split]["classification"][qid] = {"correct":correct,"total":len(labelled)}
    return result


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--checkpoint",type=Path,required=True)
    p.add_argument("--output",type=Path,required=True)
    p.add_argument("--calibration-recall",type=float,help="Optional operating-curve target; not a deployment guarantee")
    args = p.parse_args()
    root = args.checkpoint
    run = json.loads((root/"run.json").read_text())
    rows = [json.loads(line) for line in Path(run["arguments"]["dataset"]).read_text().splitlines()]
    import hashlib
    if hashlib.sha256(Path(run["arguments"]["dataset"]).read_bytes()).hexdigest()!=run["dataset_sha256"]:
        raise ValueError("dataset changed since training")
    check_splits(rows)
    report = json.loads((root/"report.json").read_text())
    cfg = json.loads((Path(run["arguments"]["model_dir"])/"rl_agent_config.json").read_text())
    base_t = cfg.get("temperature_by_options",{}).get("choice:3-5",cfg.get("temperature",[1,1,1])[0])
    base_t = min(5.0,max(0.5,float(base_t)))
    target = args.calibration_recall if args.calibration_recall is not None else run["arguments"]["target_recall"]
    if not 0 < target <= 1:
        p.error("calibration recall must be in (0,1]")
    result = {"method":"empirical-calibration-cutpoints-v2; API-rounded scores", "target_recall":target,
              "scope":"provisional opening-level labels; no independent human validation",
              "dataset_sha256":run["dataset_sha256"]}
    for name,filename,temperature in (("base","baseline-logits.json",base_t),("trained","trained-logits.json",report["temperature"])):
        result[name] = evaluate(scored(rows,json.loads((root/filename).read_text()),temperature),target)
    with args.output.open("x") as f:
        json.dump(result,f,indent=2)
    print(json.dumps(result,indent=2))


if __name__ == "__main__":
    main()
