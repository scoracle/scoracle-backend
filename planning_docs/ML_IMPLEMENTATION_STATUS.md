# ML Implementation Status

> Last updated: January 2026

## Overview

This document tracks the implementation of machine learning capabilities for Scoracle Data, as outlined in `TENSORFLOW_ML_PLAN.md`.

---

## Completed

### 1. Core ML Module Structure
- `src/scoracle_data/ml/` - Main ML package
- `src/scoracle_data/ml/config.py` - Configuration (source tiers, keywords, feature definitions)

### 2. Models (with heuristic fallbacks)
| Model | File | Status |
|-------|------|--------|
| Transfer Predictor | `ml/models/transfer_predictor.py` | Built, needs training |
| Sentiment Analyzer | `ml/models/sentiment_analyzer.py` | Built, uses HuggingFace |
| Similarity Engine | `ml/models/similarity_engine.py` | Built, needs training |
| Performance Predictor | `ml/models/performance_predictor.py` | Built, needs training |

### 3. Pipelines
| Pipeline | File | Description |
|----------|------|-------------|
| Text Processing | `ml/pipelines/text_processing.py` | Entity extraction, transfer mention detection |
| Data Loaders | `ml/pipelines/data_loaders.py` | Database loading utilities |
| Feature Engineering | `ml/pipelines/feature_engineering.py` | Feature computation for all models |

### 4. Inference Layer
- `ml/inference/model_registry.py` - Model loading and versioning
- `ml/inference/prediction_service.py` - Unified prediction interface

### 5. API Router
- `src/scoracle_data/api/routers/ml.py` - Full ML API endpoints
- Endpoints: `/transfers/*`, `/vibe/*`, `/similar/*`, `/predictions/*`

### 6. Database Migration
- `src/scoracle_data/migrations/013_ml_tables.sql` - All ML tables

### 7. Jobs Module (NEW)
| Job | File | Interval | Description |
|-----|------|----------|-------------|
| Mention Scanner | `ml/jobs/mention_scanner.py` | 30 min | Scans news/Twitter/Reddit for transfer mentions |
| Prediction Refresh | `ml/jobs/prediction_refresh.py` | 60 min | Updates transfer predictions from mentions |
| Vibe Calculator | `ml/jobs/vibe_calculator.py` | 60 min | Calculates sentiment-based vibe scores |
| Scheduler | `ml/jobs/scheduler.py` | - | Job orchestration with history tracking |

### 8. CLI Commands (NEW)
```bash
# Individual jobs
python -m scoracle_data.cli ml scan --sport FOOTBALL
python -m scoracle_data.cli ml predict --sport NBA
python -m scoracle_data.cli ml vibe --entity-type player

# All jobs
python -m scoracle_data.cli ml run-all

# Status and history
python -m scoracle_data.cli ml status
python -m scoracle_data.cli ml history --job mention_scan

# Daemon mode
python -m scoracle_data.cli ml scheduler
```

---

## Next Steps

### Immediate (Before First Use)

1. **Run ML Migration**
   ```bash
   # Connect to Neon and run the migration
   psql $NEON_DATABASE_URL_V2 -f src/scoracle_data/migrations/013_ml_tables.sql
   ```

2. **Test CLI Commands**
   ```bash
   # Test mention scanning (uses free Google News RSS)
   python -m scoracle_data.cli ml scan --sport FOOTBALL

   # Check job status
   python -m scoracle_data.cli ml status
   ```

### Short-term

3. **Seed Historical Transfer Data**
   - Source: TransferMarkt, official club announcements
   - Target table: `historical_transfers`
   - Use for training transfer predictor

4. **Configure API Keys** (optional, for expanded scanning)
   ```bash
   # .env.local
   TWITTER_BEARER_TOKEN=your_token
   REDDIT_CLIENT_ID=your_id
   REDDIT_CLIENT_SECRET=your_secret
   ```

### Medium-term

5. **Train TensorFlow Models**
   - Requires: Historical data seeded
   - Scripts to create: `ml/training/train_transfer_predictor.py`
   - Output: Saved models in `ml_models/` directory

6. **Deploy Job Scheduler**
   - Option A: Run as daemon (`ml scheduler`)
   - Option B: Cron job calling individual jobs
   - Option C: Cloud scheduler (Railway, Render cron)

### Long-term

7. **Model Improvements**
   - Add more source tiers (podcasts, YouTube)
   - Fine-tune sentiment model on sports data
   - Implement similarity engine training

---

## Database Configuration

The Neon connection is configured in `.env.local`:
```
NEON_DATABASE_URL_V2=postgresql://...@ep-plain-bonus-a811ff4s-pooler.eastus2.azure.neon.tech/neondb?sslmode=require&channel_binding=require
```

The code looks for environment variables in this order:
1. `NEON_DATABASE_URL_V2`
2. `DATABASE_URL`
3. `NEON_DATABASE_URL`

---

## File Structure

```
src/scoracle_data/ml/
├── __init__.py
├── config.py                 # ML configuration
├── models/
│   ├── __init__.py
│   ├── transfer_predictor.py
│   ├── sentiment_analyzer.py
│   ├── similarity_engine.py
│   └── performance_predictor.py
├── pipelines/
│   ├── __init__.py
│   ├── text_processing.py
│   ├── data_loaders.py
│   └── feature_engineering.py
├── inference/
│   ├── __init__.py
│   ├── model_registry.py
│   └── prediction_service.py
├── training/
│   └── __init__.py           # Training scripts (TODO)
└── jobs/
    ├── __init__.py
    ├── mention_scanner.py
    ├── prediction_refresh.py
    ├── vibe_calculator.py
    └── scheduler.py
```

---

## Dependencies

ML dependencies are optional. Install with:
```bash
pip install -e ".[ml]"
```

This installs:
- tensorflow>=2.15.0
- transformers>=4.36.0
- numpy, pandas, scikit-learn

Without ML deps, heuristic fallbacks are used.
