"""Local Laya head fine-tuning, calibration, and held-out intake evaluation.

RLCD + cross-entropy follows the official Apache-2.0 Laya notebook's objective:
https://github.com/NandhaKishorM/laya/blob/23a17522aa4942da6cce53a995a275760320b691/notebooks/laya_finetune_typed_decisions_2xT4_kaggle.ipynb
This single-device pilot freezes most/all of the encoder; it is not the upstream full-model run.
Only entity/content are supervised. Character and emotion outputs remain unvalidated.
"""
import argparse
from collections import Counter
import hashlib
import json
import math
from pathlib import Path
import random
import time


def metrics(rows, entity_threshold, reporting_threshold):
    """Intake relevance AND reporting, independent of downstream character gates."""
    scored = [r for r in rows if {"entity", "content"} <= r["labels"].keys()]
    tp = fp = fn = tn = 0
    positive_stories, retained_stories = set(), set()
    for index, r in enumerate(scored):
        positive = r["labels"]["entity"] in ("subject", "opponent") and r["labels"]["content"] == "reporting"
        predicted = (r["scores"]["entity"]["subject"] + r["scores"]["entity"]["opponent"] >= entity_threshold
                     and r["scores"]["content"]["reporting"] >= reporting_threshold)
        tp += positive and predicted
        fp += not positive and predicted
        fn += positive and not predicted
        tn += not positive and not predicted
        group = r.get("story_group", f"article:{r.get('article_id', index)}")
        if positive:
            positive_stories.add(group)
            if predicted:
                retained_stories.add(group)
    positives = tp + fn
    recall = tp / positives if positives else None
    # Wilson interval conveys how little a small pilot can establish.
    interval = None
    if positives:
        z = 1.96
        center = (recall + z*z/(2*positives))/(1+z*z/positives)
        radius = z * math.sqrt(recall*(1-recall)/positives+z*z/(4*positives**2))/(1+z*z/positives)
        interval = [center-radius, center+radius]
    return {"articles": len(scored), "positives": positives, "tp": tp, "fp": fp, "fn": fn, "tn": tn,
            "recall": recall, "recall_wilson_95": interval,
            "precision": tp/(tp+fp) if tp+fp else None,
            "positive_story_groups": len(positive_stories), "retained_story_groups": len(retained_stories),
            "story_recall": len(retained_stories)/len(positive_stories) if positive_stories else None,
            "forwarded": tp+fp, "forward_rate": (tp+fp)/len(scored) if scored else None}


def select_thresholds(calibration, target_recall):
    choices = []
    # Empirical calibration cutpoints avoid a coarse grid skipping narrow calibrated
    # score ranges. No test labels or test scores participate in selection.
    complete = [r for r in calibration if {"entity", "content"} <= r["labels"].keys()]
    entity_cuts = {0.0} | {r["scores"]["entity"]["subject"] + r["scores"]["entity"]["opponent"] for r in complete}
    reporting_cuts = {0.0} | {r["scores"]["content"]["reporting"] for r in complete}
    for entity in sorted(entity_cuts):
        for reporting in sorted(reporting_cuts):
            m = metrics(calibration, entity, reporting)
            if m["recall"] is not None and m["recall"] >= target_recall:
                choices.append((m["precision"], m["recall"], entity + reporting, entity, reporting))
    if not choices:
        raise ValueError("calibration needs useful-news positives")
    best = max(choices)
    return {"entity": best[3], "reporting": best[4], "target_recall": target_recall}


def check_splits(rows):
    groups, seen = {}, set()
    for row in rows:
        if row["article_id"] in seen:
            raise ValueError("duplicate article")
        seen.add(row["article_id"])
        if row["split"] not in ("train", "calibration", "test"):
            raise ValueError("invalid split")
        previous = groups.setdefault(row["story_group"], row["split"])
        if previous != row["split"]:
            raise ValueError("story leakage across splits")
    if {r["split"] for r in rows} != {"train", "calibration", "test"}:
        raise ValueError("all three splits required")


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--dataset", type=Path, required=True)
    p.add_argument("--model-dir", type=Path, required=True)
    p.add_argument("--revision", required=True)
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--device", choices=("cpu", "mps", "cuda"), default="cpu")
    p.add_argument("--epochs", type=int, default=4)
    p.add_argument("--batch-size", type=int, default=2)
    p.add_argument("--accumulate", type=int, default=4)
    p.add_argument("--lr", type=float, default=1e-4)
    p.add_argument("--encoder-layers", type=int, default=0, help="Unfreeze only this many final encoder layers")
    p.add_argument("--encoder-lr", type=float, default=1e-5)
    p.add_argument("--target-recall", type=float, default=0.75)
    p.add_argument("--seed", type=int, default=20260924)
    args = p.parse_args()
    if args.epochs < 1 or args.batch_size < 1 or args.accumulate < 1 or args.encoder_layers < 0 or not 0 < args.target_recall <= 1:
        p.error("positive training sizes and target recall in (0,1] required")
    if args.output.exists():
        p.error("output already exists; use a new run directory")
    rows = [json.loads(line) for line in args.dataset.read_text().splitlines()]
    check_splits(rows)
    import importlib.metadata
    import torch
    import laya
    from laya.common import build_sequence, serialize_state, collate_items, proper_reward
    from safetensors.torch import save_file

    torch.set_num_threads(2)
    torch.manual_seed(args.seed)
    random.seed(args.seed)
    agent = laya.load(str(args.model_dir), device=args.device)
    if agent.device.type != args.device:
        raise RuntimeError("requested training device unavailable; no silent fallback")
    model = agent.model.float()
    model.eval()
    args.output.mkdir(parents=True)
    with (args.model_dir / "model.safetensors").open("rb") as f:
        base_hash = hashlib.file_digest(f, "sha256").hexdigest()
    metadata = {"base_revision": args.revision, "base_weights_sha256": base_hash,
        "dataset_sha256": hashlib.sha256(args.dataset.read_bytes()).hexdigest(),
        "arguments": {k: str(v) if isinstance(v, Path) else v for k,v in vars(args).items()},
        "runtime": {k: importlib.metadata.version(k) for k in ("laya", "torch", "transformers")},
        "dtype": "float32", "objective": "RLCD plus cross-entropy; encoder frozen except declared final layers; no act-head training",
        "supervised_questions": ["entity", "content"], "split_articles": dict(Counter(r["split"] for r in rows)),
        "human_reviewed_labels": sum(r["human_reviewed"] for r in rows)}
    (args.output / "run.json").write_text(json.dumps(metadata, indent=2))

    items = []
    for row in rows:
        request = row["request"]
        state_ids = agent.tok(serialize_state(request["state"]).replace(agent.tok.mask_token, " "),
                              add_special_tokens=False)["input_ids"]
        for qid, label in row["labels"].items():
            if qid not in ("entity", "content"):
                raise ValueError("pilot only supervises entity and content")
            question = request["questions"][qid]
            agent._check_question(qid, question)
            q = agent._to_internal(question)
            empty, _ = build_sequence(agent.tok, "", q, agent.cfg["max_len"], agent.cfg["head_max_len"], state_ids=[])
            if len(state_ids) > agent.cfg["max_len"] - len(empty):
                raise ValueError(f"{row['article_id']}/{qid}: source would truncate")
            encoded = agent._encode_state(request["state"], [qid], {qid:q})[0]
            keys = list(question["criteria"])
            if label not in keys:
                raise ValueError("unknown label")
            encoded.update(target=[float(k == label) for k in keys], keys=keys,
                           article_id=row["article_id"], qid=qid, split=row["split"])
            items.append(encoded)

    def forward(chunk):
        b = collate_items([[it] for it in chunk], agent.tok.pad_token_id)
        logits, _ = model(**{k:b[k].to(agent.device) for k in
                            ("input_ids", "attention_mask", "marker_pos", "marker_mask", "qtype")})
        return logits, b

    def predict_logits(selected):
        model.eval()
        predictions = []
        with torch.no_grad():
            for start in range(0, len(selected), args.batch_size):
                chunk = selected[start:start+args.batch_size]
                logits, _ = forward(chunk)
                for it,z in zip(chunk, logits.cpu().tolist()):
                    predictions.append({"article_id":it["article_id"], "qid":it["qid"],
                                        "keys":it["keys"], "logits":z[:len(it["keys"])], "target":it["target"]})
        return predictions

    def probabilities(predictions, temperature):
        by_article = {}
        for pred in predictions:
            dist = torch.softmax(torch.tensor(pred["logits"]) / temperature, -1).tolist()
            by_article.setdefault(pred["article_id"], {})[pred["qid"]] = dict(zip(pred["keys"], map(lambda v:round(v,4),dist)))
        return [dict(r, scores=by_article[r["article_id"]]) for r in rows if r["article_id"] in by_article]

    # Base and trained predictions use identical FP32 execution, not different backends.
    held = [it for it in items if it["split"] != "train"]
    started = time.perf_counter()
    print(f"Baseline: {len(held)} held-out decisions", flush=True)
    baseline = predict_logits(held)
    (args.output / "baseline-logits.json").write_text(json.dumps(baseline))
    for name, param in model.named_parameters():
        param.requires_grad_(not name.startswith(("encoder.", "act_head.")))
    if args.encoder_layers:
        layers = getattr(model.encoder, "layers", None)
        if layers is None or args.encoder_layers > len(layers):
            raise ValueError("unsupported encoder layer selection")
        for layer in layers[-args.encoder_layers:]:
            layer.requires_grad_(True)
    trainable = [v for v in model.parameters() if v.requires_grad]
    metadata["trainable_parameters"] = sum(p.numel() for p in trainable)
    initial_scorer = model.scorer[-1].weight.detach().cpu().clone()
    optimizer = torch.optim.AdamW([
        {"params":[v for n,v in model.named_parameters() if v.requires_grad and n.startswith("encoder.")], "lr":args.encoder_lr},
        {"params":[v for n,v in model.named_parameters() if v.requires_grad and not n.startswith("encoder.")], "lr":args.lr},
    ], weight_decay=0.01)
    training = [it for it in items if it["split"] == "train"]
    print(f"Training: {len(training)} decisions; {metadata['trainable_parameters']} parameters", flush=True)
    history = []
    for epoch in range(args.epochs):
        random.Random(args.seed + epoch).shuffle(training)
        model.train()
        model.encoder.eval()  # Frozen encoder must not add training-only dropout.
        optimizer.zero_grad(set_to_none=True)
        sigma = 0.4 - 0.3 * epoch / max(1, args.epochs-1)
        losses = []
        batches = [training[i:i+args.batch_size] for i in range(0,len(training),args.batch_size)]
        for index, chunk in enumerate(batches):
            logits, b = forward(chunk)
            mask = b["marker_mask"].to(agent.device)
            target = b["target"].to(agent.device)
            noise = torch.randn((4,) + logits.shape, device=agent.device) * sigma * mask
            noise = (noise - noise.sum(-1,keepdim=True)/mask.sum(-1,keepdim=True)) * mask
            explored = logits.detach().unsqueeze(0) + noise
            q = torch.softmax(explored.masked_fill(~mask,-1e4), -1)
            with torch.no_grad():
                reward = proper_reward(q, target.unsqueeze(0), b["qtype"].to(agent.device), mask, w_sph=0.75)
                advantage = reward - reward.mean(0,keepdim=True)
                advantage /= advantage.std() + 1e-6
            logp = -(((explored-logits.unsqueeze(0))**2)*mask).sum(-1)/(2*sigma*sigma)
            loss = -(advantage*logp).mean() - (target*torch.log_softmax(logits,-1)).sum(-1).mean()
            if not torch.isfinite(loss):
                raise RuntimeError("non-finite training loss")
            # Correct scaling for the final, possibly shorter accumulation group.
            group_size = min(args.accumulate, len(batches)-(index//args.accumulate)*args.accumulate)
            (loss/group_size).backward()
            losses.append(loss.item())
            if (index+1)%args.accumulate == 0 or index+1 == len(batches):
                torch.nn.utils.clip_grad_norm_(trainable, 1.0)
                optimizer.step()
                optimizer.zero_grad(set_to_none=True)
            if (index+1)%20 == 0:
                print(f"epoch {epoch+1}: batch {index+1}/{len(batches)}", flush=True)
        history.append({"epoch":epoch+1,"mean_objective":sum(losses)/len(losses)})
        print(json.dumps(history[-1]), flush=True)
    metadata["scorer_weight_changed"] = not torch.equal(initial_scorer, model.scorer[-1].weight.detach().cpu())
    if not metadata["scorer_weight_changed"]:
        raise RuntimeError("weights did not update")
    del optimizer
    trained = predict_logits(held)
    calibration_ids = {r["article_id"] for r in rows if r["split"] == "calibration"}
    calibration_logits = [p for p in trained if p["article_id"] in calibration_ids]
    def nll(temperature):
        return sum(-(torch.tensor(p["target"])*torch.log_softmax(torch.tensor(p["logits"])/temperature,-1)).sum().item()
                   for p in calibration_logits)/len(calibration_logits)
    temperature = min((0.5,0.75,1.0,1.25,1.5,2.0,3.0,4.0,5.0), key=nll)
    base_t = agent.temperature_by_options.get("choice:3-5", agent.temperature[0])
    base_rows = probabilities(baseline, base_t)
    trained_rows = probabilities(trained, temperature)
    proposed = select_thresholds([r for r in trained_rows if r["split"]=="calibration"], args.target_recall)
    base_proposed = select_thresholds([r for r in base_rows if r["split"]=="calibration"], args.target_recall)
    report = {"temperature":temperature, "calibration_nll":nll(temperature),
              "calibration_method":"empirical-calibration-cutpoints-v2; API-rounded scores",
              "proposed_intake_policy":proposed, "base_calibrated_policy":base_proposed,
              "label_status":"provisional Codex annotations; no independent human verification",
              "scope":"opening-level entity+reporting intake only; not story coverage or full character dispatch"}
    for split in ("calibration", "test"):
        b = [r for r in base_rows if r["split"]==split]
        t = [r for r in trained_rows if r["split"]==split]
        report[split] = {"base_fixed":metrics(b,0.55,0.5),
            "base_tuned_thresholds":metrics(b,base_proposed["entity"],base_proposed["reporting"]),
            "trained_fixed":metrics(t,0.55,0.5),
            "trained_tuned_thresholds":metrics(t,proposed["entity"],proposed["reporting"]),
            "pass_everything":metrics(t,0.0,0.0)}
    cfg = dict(agent.cfg)
    cfg["fine_tuned"] = True
    cfg["model_name"] = "scoracle-harvester-laya-pilot"
    cfg["temperature_by_options"] = dict(cfg.get("temperature_by_options",{}), **{"choice:3-5":temperature})
    cfg["harvester_training"] = {"questions":["entity","content"], "base_revision":args.revision,
                                 "dataset_sha256":metadata["dataset_sha256"], "experimental":True}
    save_file({k:v.detach().cpu().contiguous() for k,v in model.state_dict().items()}, str(args.output/"model.safetensors"))
    model.encoder.config.save_pretrained(args.output/"encoder")
    agent.tok.save_pretrained(args.output/"tokenizer")
    (args.output/"rl_agent_config.json").write_text(json.dumps(cfg,indent=2))
    (args.output/"trained-logits.json").write_text(json.dumps(trained))
    (args.output/"held-out-predictions.jsonl").write_text(''.join(json.dumps(r,ensure_ascii=False)+'\n' for r in trained_rows))
    metadata.update(history=history,elapsed_seconds=time.perf_counter()-started)
    (args.output/"run.json").write_text(json.dumps(metadata,indent=2))
    (args.output/"report.json").write_text(json.dumps(report,indent=2))
    print(json.dumps(report,indent=2),flush=True)


if __name__ == "__main__":
    main()
