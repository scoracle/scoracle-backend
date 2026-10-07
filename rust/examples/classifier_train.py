"""Train one multi-label head on a frozen candidate encoder; unknown labels are masked.

python classifier_train.py SOURCE.jsonl REVIEW.jsonl OUTPUT --model MODEL@REVISION
    --vector-family relevance --vector-family topic [--check]
Uses the installed torch/transformers runtime. Saves an uncalibrated checkpoint.
"""
import argparse
import hashlib
import json
import math
import random
from pathlib import Path

from classifier_replay import SCHEMA_PATH, digest, head_labels
from classifier_review import load_review, training_rows


def masked_loss(torch, logits, labels):
    targets = torch.tensor([[0 if value is None else value for value in row] for row in labels],
                           dtype=logits.dtype, device=logits.device)
    mask = torch.tensor([[value is not None for value in row] for row in labels], device=logits.device)
    if not mask.any():
        raise ValueError("batch has no known labels")
    return torch.nn.functional.binary_cross_entropy_with_logits(logits, targets, reduction="none")[mask].mean()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("review", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--model", required=True)
    parser.add_argument("--vector-family", action="append", required=True)
    parser.add_argument("--check", action="store_true", help="Validate data and report coverage without loading a model")
    parser.add_argument("--schema", type=Path, default=SCHEMA_PATH)
    parser.add_argument("--prior-manifest", type=Path, default=SCHEMA_PATH.with_name("corpus-manifest-2026-10-07.jsonl"))
    parser.add_argument("--epochs", type=int, default=3)
    parser.add_argument("--batch-size", type=int, default=8)
    parser.add_argument("--max-tokens", type=int, default=256)
    parser.add_argument("--threads", type=int, default=2)
    parser.add_argument("--learning-rate", type=float, default=0.001)
    parser.add_argument("--seed", type=int, default=42)
    args = parser.parse_args()
    if min(args.epochs, args.batch_size, args.max_tokens, args.threads) < 1 or not (
            math.isfinite(args.learning_rate) and args.learning_rate > 0):
        parser.error("training budgets and learning rate must be positive and finite")
    model_id, _, revision = args.model.partition("@")
    if len(revision) != 40 or any(character not in "0123456789abcdef" for character in revision):
        parser.error("pin the candidate to its 40-character checkpoint revision")
    schema = json.loads(args.schema.read_text())
    mapping = head_labels(schema, args.vector_family)
    labels = list(mapping)
    prior = {row["article_id"] for row in map(json.loads, args.prior_manifest.read_text().splitlines())}
    records = load_review(args.source, args.review, schema, prior)
    dataset = training_rows(records, schema, args.vector_family)
    coverage = {split: {label: {str(value): sum(row["labels"][i] == value for row in rows)
                              for value in (0, 1)} for i, label in enumerate(labels)}
                for split, rows in dataset.items()}
    print(json.dumps({"units": {split: len(rows) for split, rows in dataset.items()},
                      "known_label_coverage": coverage}), flush=True)
    if args.check:
        return
    if not dataset["train"] or any(not counts["0"] or not counts["1"] for counts in coverage["train"].values()):
        parser.error("every requested label needs reviewed positive and negative training examples")
    if args.output.exists():
        parser.error("output must be a new checkpoint directory")
    import torch
    from transformers import AutoConfig, AutoModelForSequenceClassification, AutoTokenizer

    torch.set_num_threads(args.threads)
    torch.manual_seed(args.seed)
    rng = random.Random(args.seed)
    tokenizer = AutoTokenizer.from_pretrained(model_id, revision=revision, trust_remote_code=False)
    config = AutoConfig.from_pretrained(model_id, revision=revision, trust_remote_code=False)
    config.num_labels = len(labels)
    config.id2label = dict(enumerate(labels))
    config.label2id = {label: i for i, label in enumerate(labels)}
    config.problem_type = "multi_label_classification"
    model = AutoModelForSequenceClassification.from_pretrained(
        model_id, revision=revision, config=config, ignore_mismatched_sizes=True,
        trust_remote_code=False, use_safetensors=True, attn_implementation="eager")
    for parameter in model.base_model.parameters():
        parameter.requires_grad_(False)
    # New tasks never inherit the old emotion label-to-weight association.
    for module in model.modules():
        if isinstance(module, torch.nn.Linear) and any(parameter.requires_grad for parameter in module.parameters(recurse=False)):
            module.reset_parameters()
    train = dataset["train"]
    inputs = [tokenizer(row["text"], truncation=False) for row in train]
    capacity = min(args.max_tokens, tokenizer.model_max_length, config.max_position_embeddings)
    if any(len(row["input_ids"]) > capacity for row in inputs):
        parser.error("training window exceeds token budget; source truncation is forbidden")
    optimizer = torch.optim.AdamW([parameter for parameter in model.parameters() if parameter.requires_grad],
                                 lr=args.learning_rate)
    losses = []
    for epoch in range(args.epochs):
        model.train()
        model.base_model.eval()
        order = list(range(len(train)))
        rng.shuffle(order)
        loss_sum, known = 0.0, 0
        for offset in range(0, len(order), args.batch_size):
            indices = order[offset:offset + args.batch_size]
            batch = tokenizer.pad([inputs[i] for i in indices], padding=True, return_tensors="pt")
            annotations = [train[i]["labels"] for i in indices]
            optimizer.zero_grad()
            loss = masked_loss(torch, model(**batch).logits, annotations)
            if not torch.isfinite(loss):
                raise ValueError("nonfinite training loss")
            loss.backward()
            optimizer.step()
            count = sum(value is not None for row in annotations for value in row)
            loss_sum += loss.item() * count
            known += count
        losses.append(loss_sum / known)
        print(f"epoch={epoch + 1} training_loss={losses[-1]:.6f}", flush=True)
    args.output.mkdir(parents=True)
    model.save_pretrained(args.output, safe_serialization=True)
    tokenizer.save_pretrained(args.output)
    with (args.output / "model.safetensors").open("rb") as weights:
        weights_hash = hashlib.file_digest(weights, "sha256").hexdigest()
    manifest = {"base_model": model_id, "base_revision": revision, "weights_sha256": weights_hash,
                "source_sha256": hashlib.sha256(args.source.read_bytes()).hexdigest(),
                "review_sha256": hashlib.sha256(args.review.read_bytes()).hexdigest(),
                "schema_sha256": digest(json.dumps(schema, sort_keys=True)),
                "families": args.vector_family, "frozen_encoder": True, "calibrated": False,
                "training_loss": losses, "coverage": coverage,
                "settings": {key: getattr(args, key) for key in ("epochs", "batch_size", "max_tokens", "threads", "learning_rate", "seed")}}
    (args.output / "training.json").write_text(json.dumps(manifest, indent=2) + "\n")


if __name__ == "__main__":
    main()
