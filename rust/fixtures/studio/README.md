# Studio contract captures

`influencer-baseline.json` was captured from backend `0c8ebe5` before moving Influencer creation. It pins the v32 system prompt and 12 exact prompt/material-hash cases: three existing sourced-memory packages, live/empty packets, and absent/present prior score. Prompt hashes are SHA-256 of the exact UTF-8 prompt. The application acceptance test now also runs Studio creation for these inputs.

The original five `fixtures/vibe` model-quality cases retain their historical v31 prompts. They are not rewritten by this mechanical extraction. Live model-quality evaluation remains a separate acceptance gate.
