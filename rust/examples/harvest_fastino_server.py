"""Local GLiNER2.5-Decide adapter for the existing Harvester replay contract."""
import argparse
import hashlib
import importlib.metadata
from pathlib import Path
import time


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--model-dir", required=True)
    parser.add_argument("--revision", required=True)
    parser.add_argument("--port", type=int, default=8020)
    args = parser.parse_args()

    import torch
    from fastapi import FastAPI, HTTPException
    from gliner2.classification import Classifier, ClassificationSchema
    import uvicorn

    torch.set_num_threads(2)
    model = Classifier.from_pretrained(args.model_dir, device="cpu")
    limit = model.model.encoder.config.max_position_embeddings
    weights = Path(args.model_dir) / "model.safetensors"
    with weights.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    provenance = {
        "model": "fastino/GLiNER2.5-Decide", "revision": args.revision,
        "adapter": "harvest-fastino-v1", "device": str(model.device),
        "weights_sha256": digest,
        "runtime": {name: importlib.metadata.version(name) for name in ("gliner2", "torch")},
    }
    app = FastAPI()

    @app.get("/health")
    def health():
        return {"ok": True, "provenance": provenance}

    @app.post("/v1/systemone")
    def decide(payload: dict):
        state, questions = payload.get("state"), payload.get("questions")
        if not isinstance(state, str) or not isinstance(questions, dict) or not questions:
            raise HTTPException(422, "state string and nonempty questions required")
        try:
            schema = ClassificationSchema()
            for key, question in questions.items():
                if (not isinstance(key, str) or not isinstance(question, dict)
                        or question.get("type") != "noul"
                        or not isinstance(question.get("criteria"), dict)
                        or set(question["criteria"]) != {"false", "true"}
                        or not isinstance(question.get("instructions"), str)):
                    raise HTTPException(422, f"unsupported predicate: {key}")
                schema.single(key, question["criteria"], instruction=question["instructions"])
            compiled = model.compile_schema(schema)
            batch = model.model.processor.collate_fn_inference([(state, compiled.build())])
            tokens = batch.input_ids.shape[1]
            if tokens > limit:
                raise HTTPException(422, f"input would exceed encoder limit ({tokens}>{limit})")
            start = time.perf_counter()
            scores = model.score(state, compiled)
            answers = {key: {"noul": scores.probability(key, "true")} for key in questions}
        except HTTPException:
            raise
        except Exception as error:
            raise HTTPException(422, str(error)) from error
        return {
            "answers": answers,
            "provenance": dict(provenance,
                               coverage={key: {"tokens": tokens, "truncated": False}
                                         for key in questions},
                               inference_ms=(time.perf_counter() - start) * 1000),
        }

    uvicorn.run(app, host="127.0.0.1", port=args.port, log_level="warning")


if __name__ == "__main__":
    main()
