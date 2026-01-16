# TensorFlow ML Implementation Plan

## Overview

This document outlines the implementation plan for incorporating machine learning capabilities into Scoracle Data using TensorFlow. These features will provide users with predictive insights, sentiment analysis, and intelligent entity comparisons.

**Target Stack:**
- TensorFlow 2.x / Keras
- TensorFlow Hub (pre-trained models)
- NumPy / Pandas (feature engineering)
- FastAPI (API serving)
- PostgreSQL (feature store + predictions)

---

## Feature Priority Order

| Priority | Feature | Business Value | Complexity |
|----------|---------|----------------|------------|
| 1 | Transfer/Trade Predictor | Very High | High |
| 2 | Vibe Score (Sentiment) | High | Medium |
| 3 | Entity Similarity Engine | Medium-High | Medium |
| 4 | Performance Predictor | Medium | High |

---

## 1. Transfer/Trade Predictor (TOP PRIORITY)

### Overview
Predict transfer/trade likelihood percentages for players linked to teams based on news mentions, social media activity, and historical transfer patterns.

**Example Output:**
```
Chelsea Transfer Targets:
├── Victor Osimhen    → 67% likelihood (trending up ↑)
├── Joao Felix        → 34% likelihood (stable →)
├── Marcus Rashford   → 12% likelihood (trending down ↓)
```

### Data Sources
- **News API** (already integrated via `news_router.py`)
- **Twitter API v2** (already integrated via `intel_router.py`)
- **Reddit API** (already integrated)
- **Historical transfers** (new - requires seeding from API-Sports)

### Model Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                  TRANSFER PREDICTOR PIPELINE                │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐  │
│  │  News Feed   │    │ Twitter Feed │    │ Reddit Feed  │  │
│  └──────┬───────┘    └──────┬───────┘    └──────┬───────┘  │
│         │                   │                   │          │
│         ▼                   ▼                   ▼          │
│  ┌─────────────────────────────────────────────────────┐   │
│  │           TEXT PREPROCESSING LAYER                  │   │
│  │  • Entity extraction (player/team NER)              │   │
│  │  • Link detection (player ↔ team mentions)          │   │
│  │  • Rumor keyword identification                     │   │
│  └─────────────────────────────────────────────────────┘   │
│                          │                                  │
│                          ▼                                  │
│  ┌─────────────────────────────────────────────────────┐   │
│  │           FEATURE ENGINEERING                       │   │
│  │  • Mention frequency (24h, 7d, 30d windows)         │   │
│  │  • Source credibility weighting                     │   │
│  │  • Sentiment polarity of mentions                   │   │
│  │  • Co-occurrence strength (player + team)           │   │
│  │  • Temporal velocity (mention acceleration)         │   │
│  │  • Historical transfer pattern features             │   │
│  └─────────────────────────────────────────────────────┘   │
│                          │                                  │
│                          ▼                                  │
│  ┌─────────────────────────────────────────────────────┐   │
│  │           TENSORFLOW MODEL                          │   │
│  │  Multi-input neural network:                        │   │
│  │  • Text embedding branch (BERT/DistilBERT)          │   │
│  │  • Numerical features branch (Dense layers)         │   │
│  │  • Historical patterns branch (LSTM)                │   │
│  │  • Concatenated → Dense → Sigmoid output            │   │
│  └─────────────────────────────────────────────────────┘   │
│                          │                                  │
│                          ▼                                  │
│  ┌─────────────────────────────────────────────────────┐   │
│  │           OUTPUT                                    │   │
│  │  • Transfer probability (0-100%)                    │   │
│  │  • Confidence interval                              │   │
│  │  • Trend direction (↑ → ↓)                          │   │
│  │  • Key contributing factors                         │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### Feature Engineering Details

| Feature | Description | Weight |
|---------|-------------|--------|
| `mention_freq_24h` | Mentions in last 24 hours | High |
| `mention_freq_7d` | Mentions in last 7 days | Medium |
| `mention_velocity` | Rate of change in mentions | High |
| `source_tier` | Credibility of sources (Tier 1-4) | Very High |
| `sentiment_score` | Positive/negative sentiment | Medium |
| `cooccurrence_strength` | How often player+team mentioned together | High |
| `transfer_window_active` | Is transfer window open? | High |
| `contract_years_remaining` | Player's contract situation | High |
| `historical_club_spending` | Team's transfer budget patterns | Medium |
| `player_age_factor` | Age-based transfer likelihood | Low |

### Source Credibility Tiers (Football Example)

```python
SOURCE_TIERS = {
    # Tier 1: Official + Top Journalists (weight: 1.0)
    "tier_1": ["official_club", "fabrizio_romano", "david_ornstein", "matt_law"],

    # Tier 2: Reliable National Media (weight: 0.7)
    "tier_2": ["bbc_sport", "sky_sports", "the_athletic", "espn"],

    # Tier 3: National Newspapers (weight: 0.4)
    "tier_3": ["guardian", "telegraph", "times", "mirror"],

    # Tier 4: Aggregators + Rumors (weight: 0.15)
    "tier_4": ["90min", "football_italia", "reddit_soccer"]
}
```

### Database Schema

```sql
-- Track transfer links/rumors
CREATE TABLE transfer_links (
    id SERIAL PRIMARY KEY,
    player_id INTEGER REFERENCES players(id),
    team_id INTEGER REFERENCES teams(id),
    sport VARCHAR(20) NOT NULL,
    first_linked_at TIMESTAMP NOT NULL,
    last_mention_at TIMESTAMP NOT NULL,
    total_mentions INTEGER DEFAULT 0,
    tier_1_mentions INTEGER DEFAULT 0,
    tier_2_mentions INTEGER DEFAULT 0,
    current_probability FLOAT,
    trend_direction VARCHAR(10), -- 'up', 'down', 'stable'
    is_active BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMP DEFAULT NOW(),
    updated_at TIMESTAMP DEFAULT NOW()
);

-- Store individual mention events
CREATE TABLE transfer_mentions (
    id SERIAL PRIMARY KEY,
    transfer_link_id INTEGER REFERENCES transfer_links(id),
    source_type VARCHAR(20) NOT NULL, -- 'news', 'twitter', 'reddit'
    source_name VARCHAR(100),
    source_tier INTEGER,
    headline TEXT,
    sentiment_score FLOAT,
    url TEXT,
    mentioned_at TIMESTAMP NOT NULL,
    created_at TIMESTAMP DEFAULT NOW()
);

-- Historical transfers for training
CREATE TABLE historical_transfers (
    id SERIAL PRIMARY KEY,
    player_id INTEGER,
    player_name VARCHAR(100),
    from_team_id INTEGER,
    to_team_id INTEGER,
    sport VARCHAR(20),
    transfer_date DATE,
    fee_millions FLOAT,
    rumor_duration_days INTEGER,
    peak_mentions INTEGER,
    created_at TIMESTAMP DEFAULT NOW()
);

-- Model predictions log
CREATE TABLE transfer_predictions (
    id SERIAL PRIMARY KEY,
    transfer_link_id INTEGER REFERENCES transfer_links(id),
    predicted_probability FLOAT NOT NULL,
    confidence_lower FLOAT,
    confidence_upper FLOAT,
    model_version VARCHAR(50),
    feature_importance JSONB,
    predicted_at TIMESTAMP DEFAULT NOW()
);
```

### API Endpoints

```python
# New endpoints for transfer predictions
@router.get("/transfers/predictions/{team_id}")
async def get_team_transfer_predictions(team_id: int) -> TransferPredictions:
    """
    Returns all players linked to a team with transfer probabilities.

    Response:
    {
        "team_id": 123,
        "team_name": "Chelsea",
        "transfer_window": "open",
        "predictions": [
            {
                "player_id": 456,
                "player_name": "Victor Osimhen",
                "current_team": "Napoli",
                "probability": 0.67,
                "confidence_interval": [0.58, 0.76],
                "trend": "up",
                "trend_change_7d": +0.12,
                "top_factors": ["tier_1_mentions", "contract_situation"],
                "recent_headlines": [...]
            }
        ],
        "last_updated": "2024-01-15T10:30:00Z"
    }
    """

@router.get("/transfers/predictions/player/{player_id}")
async def get_player_transfer_predictions(player_id: int) -> PlayerTransferOutlook:
    """
    Returns all teams linked to a player with probabilities.
    """

@router.get("/transfers/trending")
async def get_trending_transfers(sport: str, limit: int = 10) -> TrendingTransfers:
    """
    Returns hottest transfer rumors across the sport.
    """
```

### Implementation Phases

#### Phase 1A: Data Collection Pipeline
- [ ] Create `transfer_links` and `transfer_mentions` tables
- [ ] Build mention extractor for news feed (NER for player/team detection)
- [ ] Build mention extractor for Twitter feed
- [ ] Build mention extractor for Reddit feed
- [ ] Implement source tier classification
- [ ] Create scheduled job to scan feeds every 15 minutes

#### Phase 1B: Historical Data Seeding
- [ ] Seed historical transfers from API-Sports
- [ ] Calculate retrospective mention patterns (if historical news available)
- [ ] Build training dataset from past transfer windows

#### Phase 1C: Model Development
- [ ] Feature engineering pipeline
- [ ] Train baseline model (gradient boosting for initial version)
- [ ] Develop TensorFlow multi-input model
- [ ] Implement model evaluation (precision, recall, calibration)
- [ ] Set up model versioning

#### Phase 1D: API & Integration
- [ ] Implement prediction endpoints
- [ ] Add caching layer for predictions
- [ ] Create prediction refresh scheduler (hourly)
- [ ] Build trend calculation logic

### Model Training Approach

```python
# Simplified model architecture
import tensorflow as tf
from tensorflow import keras

def build_transfer_predictor():
    # Text input branch (headlines/tweets)
    text_input = keras.Input(shape=(512,), name='text_embedding')
    text_branch = keras.layers.Dense(128, activation='relu')(text_input)
    text_branch = keras.layers.Dropout(0.3)(text_branch)

    # Numerical features branch
    numerical_input = keras.Input(shape=(15,), name='numerical_features')
    num_branch = keras.layers.Dense(64, activation='relu')(numerical_input)
    num_branch = keras.layers.BatchNormalization()(num_branch)

    # Historical pattern branch (time series of mentions)
    history_input = keras.Input(shape=(30, 5), name='mention_history')  # 30 days, 5 features
    history_branch = keras.layers.LSTM(32, return_sequences=False)(history_input)

    # Combine branches
    combined = keras.layers.concatenate([text_branch, num_branch, history_branch])
    combined = keras.layers.Dense(64, activation='relu')(combined)
    combined = keras.layers.Dropout(0.3)(combined)
    combined = keras.layers.Dense(32, activation='relu')(combined)

    # Output
    output = keras.layers.Dense(1, activation='sigmoid', name='probability')(combined)

    model = keras.Model(
        inputs=[text_input, numerical_input, history_input],
        outputs=output
    )

    model.compile(
        optimizer='adam',
        loss='binary_crossentropy',
        metrics=['accuracy', keras.metrics.AUC(name='auc')]
    )

    return model
```

---

## 2. Vibe Score (Sentiment Analysis)

### Overview
Real-time sentiment scoring for players and teams based on social media and news sentiment. Provides users with a "temperature check" on public perception.

**Example Output:**
```
LeBron James - Vibe Score: 78/100 (Positive)
├── Twitter Sentiment:  82/100 (Very Positive)
├── News Sentiment:     71/100 (Positive)
├── Reddit Sentiment:   74/100 (Positive)
└── Trend: ↑ +5 from last week
```

### Scoring System

```
VIBE SCORE SCALE:
├── 90-100: Elite (universally praised)
├── 75-89:  Positive (mostly favorable)
├── 60-74:  Neutral-Positive (mixed, leaning good)
├── 40-59:  Neutral (balanced opinions)
├── 25-39:  Neutral-Negative (mixed, leaning bad)
├── 10-24:  Negative (mostly unfavorable)
└── 0-9:    Crisis (universally criticized)
```

### Model Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    VIBE SCORE PIPELINE                      │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌──────────────────────────────────────────────────────┐  │
│  │              TEXT COLLECTION                         │  │
│  │  • Twitter: Last 100 mentions (24h-7d window)        │  │
│  │  • News: Last 20 articles (7d window)                │  │
│  │  • Reddit: Last 50 comments (7d window)              │  │
│  └──────────────────────────────────────────────────────┘  │
│                          │                                  │
│                          ▼                                  │
│  ┌──────────────────────────────────────────────────────┐  │
│  │         PRE-TRAINED SENTIMENT MODEL                  │  │
│  │  TensorFlow Hub: DistilBERT fine-tuned on sports     │  │
│  │  OR                                                  │  │
│  │  cardiffnlp/twitter-roberta-base-sentiment           │  │
│  └──────────────────────────────────────────────────────┘  │
│                          │                                  │
│                          ▼                                  │
│  ┌──────────────────────────────────────────────────────┐  │
│  │         SENTIMENT AGGREGATION                        │  │
│  │  • Per-source sentiment (Twitter, News, Reddit)      │  │
│  │  • Recency weighting (newer = higher weight)         │  │
│  │  • Engagement weighting (likes/RTs = credibility)    │  │
│  │  • Outlier filtering (remove extreme spam)           │  │
│  └──────────────────────────────────────────────────────┘  │
│                          │                                  │
│                          ▼                                  │
│  ┌──────────────────────────────────────────────────────┐  │
│  │         VIBE SCORE OUTPUT                            │  │
│  │  • Overall score (0-100)                             │  │
│  │  • Per-source breakdown                              │  │
│  │  • Trend (vs 7d ago)                                 │  │
│  │  • Key positive/negative themes                      │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### Database Schema

```sql
-- Store vibe scores over time
CREATE TABLE vibe_scores (
    id SERIAL PRIMARY KEY,
    entity_type VARCHAR(10) NOT NULL, -- 'player' or 'team'
    entity_id INTEGER NOT NULL,
    sport VARCHAR(20) NOT NULL,
    overall_score FLOAT NOT NULL,
    twitter_score FLOAT,
    news_score FLOAT,
    reddit_score FLOAT,
    sample_size INTEGER,
    positive_themes TEXT[],
    negative_themes TEXT[],
    calculated_at TIMESTAMP DEFAULT NOW()
);

-- Index for time-series queries
CREATE INDEX idx_vibe_scores_entity_time
ON vibe_scores(entity_type, entity_id, calculated_at DESC);

-- Store individual sentiment analyses (for debugging/auditing)
CREATE TABLE sentiment_samples (
    id SERIAL PRIMARY KEY,
    vibe_score_id INTEGER REFERENCES vibe_scores(id),
    source_type VARCHAR(20) NOT NULL,
    text_content TEXT,
    sentiment_label VARCHAR(20), -- 'positive', 'negative', 'neutral'
    sentiment_confidence FLOAT,
    engagement_score INTEGER,
    analyzed_at TIMESTAMP DEFAULT NOW()
);
```

### API Endpoints

```python
@router.get("/vibe/{entity_type}/{entity_id}")
async def get_vibe_score(entity_type: str, entity_id: int) -> VibeScore:
    """
    Returns current vibe score for an entity.

    Response:
    {
        "entity_id": 123,
        "entity_name": "LeBron James",
        "entity_type": "player",
        "vibe_score": 78,
        "vibe_label": "Positive",
        "breakdown": {
            "twitter": {"score": 82, "sample_size": 156},
            "news": {"score": 71, "sample_size": 12},
            "reddit": {"score": 74, "sample_size": 45}
        },
        "trend": {
            "direction": "up",
            "change_7d": +5,
            "change_30d": +12
        },
        "themes": {
            "positive": ["clutch performance", "leadership"],
            "negative": ["rest games", "load management"]
        },
        "last_updated": "2024-01-15T10:30:00Z"
    }
    """

@router.get("/vibe/trending/{sport}")
async def get_trending_vibes(sport: str) -> TrendingVibes:
    """
    Returns entities with biggest vibe score changes.
    """

@router.get("/vibe/history/{entity_type}/{entity_id}")
async def get_vibe_history(entity_type: str, entity_id: int, days: int = 30) -> VibeHistory:
    """
    Returns vibe score history for charting.
    """
```

### Implementation Phases

#### Phase 2A: Sentiment Model Setup
- [ ] Evaluate pre-trained models (DistilBERT vs RoBERTa vs custom)
- [ ] Set up TensorFlow Hub integration
- [ ] Fine-tune on sports-specific corpus (optional enhancement)
- [ ] Implement inference pipeline

#### Phase 2B: Data Pipeline
- [ ] Create aggregation logic for Twitter mentions
- [ ] Create aggregation logic for news articles
- [ ] Create aggregation logic for Reddit comments
- [ ] Implement recency and engagement weighting

#### Phase 2C: Score Calculation
- [ ] Build score aggregation formula
- [ ] Implement trend calculation
- [ ] Add theme extraction (keyword clustering)
- [ ] Create scheduled calculation job (every 4 hours)

#### Phase 2D: API & Caching
- [ ] Implement vibe score endpoints
- [ ] Add caching (1-hour TTL)
- [ ] Build history endpoint for charting

### Sentiment Model Options

| Model | Pros | Cons | Recommendation |
|-------|------|------|----------------|
| `cardiffnlp/twitter-roberta-base-sentiment` | Trained on Twitter, fast | May miss sports context | Good starting point |
| `distilbert-base-uncased-finetuned-sst-2` | Fast, general purpose | Not sports-specific | Backup option |
| Custom fine-tuned BERT | Sports-specific, accurate | Requires training data | Future enhancement |
| VADER (rule-based) | No ML needed, fast | Less accurate | Not recommended |

---

## 3. Entity Similarity Engine

### Overview
Find the top 3 most statistically similar entities (players or teams) within the same sport. Uses embedding-based similarity for nuanced comparisons.

**Example Output:**
```
Players similar to Jayson Tatum:
├── 1. Paul George    (92% similarity) - Scoring wing, similar efficiency
├── 2. Kawhi Leonard  (89% similarity) - Two-way forward, iso-heavy
└── 3. Jimmy Butler   (86% similarity) - Versatile scorer, playmaking
```

### Model Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                ENTITY SIMILARITY ENGINE                     │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌──────────────────────────────────────────────────────┐  │
│  │         FEATURE EXTRACTION                           │  │
│  │  Players: PPG, RPG, APG, FG%, 3P%, TS%, USG%, etc.  │  │
│  │  Teams: ORtg, DRtg, Pace, 3PAr, FTr, etc.           │  │
│  └──────────────────────────────────────────────────────┘  │
│                          │                                  │
│                          ▼                                  │
│  ┌──────────────────────────────────────────────────────┐  │
│  │         NORMALIZATION                                │  │
│  │  • Z-score normalization per stat                    │  │
│  │  • Position-adjusted (players only)                  │  │
│  │  • Era-adjusted (optional)                           │  │
│  └──────────────────────────────────────────────────────┘  │
│                          │                                  │
│                          ▼                                  │
│  ┌──────────────────────────────────────────────────────┐  │
│  │         EMBEDDING MODEL                              │  │
│  │  Autoencoder or Dense network                        │  │
│  │  Input: N stats → Latent: 32-64 dims → Output: N     │  │
│  │  Use latent space for similarity                     │  │
│  └──────────────────────────────────────────────────────┘  │
│                          │                                  │
│                          ▼                                  │
│  ┌──────────────────────────────────────────────────────┐  │
│  │         SIMILARITY CALCULATION                       │  │
│  │  • Cosine similarity in embedding space              │  │
│  │  • OR Euclidean distance (normalized)                │  │
│  │  • Return top-K most similar                         │  │
│  └──────────────────────────────────────────────────────┘  │
│                          │                                  │
│                          ▼                                  │
│  ┌──────────────────────────────────────────────────────┐  │
│  │         OUTPUT                                       │  │
│  │  • Top 3 similar entities                            │  │
│  │  • Similarity percentage                             │  │
│  │  • Key shared traits                                 │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### Feature Sets by Sport

#### NBA Players
```python
NBA_PLAYER_FEATURES = [
    # Scoring
    'ppg', 'fg_pct', 'fg3_pct', 'ft_pct', 'ts_pct',
    # Rebounding
    'rpg', 'orpg', 'drpg',
    # Playmaking
    'apg', 'ast_to_ratio', 'tov_pg',
    # Defense
    'spg', 'bpg',
    # Usage & Efficiency
    'usg_pct', 'per', 'mpg',
    # Shot profile (if available)
    'fg3a_rate', 'fta_rate'
]
```

#### NBA Teams
```python
NBA_TEAM_FEATURES = [
    'offensive_rating', 'defensive_rating', 'net_rating',
    'pace', 'ts_pct', 'efg_pct',
    'tov_pct', 'orb_pct', 'ft_rate',
    'fg3a_rate', 'ppg', 'opp_ppg'
]
```

#### NFL Players (QB Example)
```python
NFL_QB_FEATURES = [
    'pass_yds_pg', 'pass_td_pg', 'int_pg',
    'completion_pct', 'passer_rating', 'qbr',
    'rush_yds_pg', 'rush_td_pg',
    'sack_pct', 'ypa'
]
```

### Database Schema

```sql
-- Store pre-computed embeddings
CREATE TABLE entity_embeddings (
    id SERIAL PRIMARY KEY,
    entity_type VARCHAR(10) NOT NULL,
    entity_id INTEGER NOT NULL,
    sport VARCHAR(20) NOT NULL,
    season VARCHAR(10),
    embedding FLOAT[] NOT NULL, -- 64-dim vector
    feature_vector FLOAT[], -- Original normalized features
    computed_at TIMESTAMP DEFAULT NOW(),
    UNIQUE(entity_type, entity_id, season)
);

-- Pre-computed similarity pairs (for fast lookup)
CREATE TABLE entity_similarities (
    id SERIAL PRIMARY KEY,
    entity_type VARCHAR(10) NOT NULL,
    entity_id INTEGER NOT NULL,
    similar_entity_id INTEGER NOT NULL,
    sport VARCHAR(20) NOT NULL,
    similarity_score FLOAT NOT NULL,
    shared_traits TEXT[],
    rank INTEGER NOT NULL, -- 1, 2, or 3
    computed_at TIMESTAMP DEFAULT NOW(),
    UNIQUE(entity_type, entity_id, similar_entity_id)
);

-- Index for fast lookups
CREATE INDEX idx_similarities_lookup
ON entity_similarities(entity_type, entity_id, rank);
```

### API Endpoints

```python
@router.get("/similar/{entity_type}/{entity_id}")
async def get_similar_entities(entity_type: str, entity_id: int, limit: int = 3) -> SimilarEntities:
    """
    Returns most similar entities.

    Response:
    {
        "entity_id": 123,
        "entity_name": "Jayson Tatum",
        "entity_type": "player",
        "sport": "nba",
        "similar_entities": [
            {
                "entity_id": 456,
                "entity_name": "Paul George",
                "similarity_score": 0.92,
                "similarity_label": "Very Similar",
                "shared_traits": ["scoring wing", "similar efficiency", "high usage"],
                "key_differences": ["more 3PA", "fewer assists"]
            },
            ...
        ]
    }
    """

@router.get("/similar/compare/{entity_type}/{entity_id_1}/{entity_id_2}")
async def compare_entities(entity_type: str, entity_id_1: int, entity_id_2: int) -> EntityComparison:
    """
    Direct comparison between two entities.
    """
```

### Implementation Phases

#### Phase 3A: Feature Engineering
- [ ] Define feature sets for each sport/position
- [ ] Build normalization pipeline
- [ ] Handle missing data (imputation strategy)

#### Phase 3B: Embedding Model
- [ ] Build autoencoder architecture
- [ ] Train on historical player/team data
- [ ] Evaluate embedding quality (clustering visualization)

#### Phase 3C: Similarity Computation
- [ ] Implement cosine similarity calculation
- [ ] Build batch computation job
- [ ] Store pre-computed similarities

#### Phase 3D: API & Integration
- [ ] Implement similarity endpoints
- [ ] Add to player/team profile responses
- [ ] Set up nightly recomputation job

### Model Architecture (Autoencoder)

```python
def build_similarity_autoencoder(input_dim: int, latent_dim: int = 64):
    # Encoder
    encoder_input = keras.Input(shape=(input_dim,))
    x = keras.layers.Dense(128, activation='relu')(encoder_input)
    x = keras.layers.BatchNormalization()(x)
    x = keras.layers.Dense(64, activation='relu')(x)
    latent = keras.layers.Dense(latent_dim, activation='linear', name='embedding')(x)

    encoder = keras.Model(encoder_input, latent, name='encoder')

    # Decoder
    decoder_input = keras.Input(shape=(latent_dim,))
    x = keras.layers.Dense(64, activation='relu')(decoder_input)
    x = keras.layers.Dense(128, activation='relu')(x)
    decoder_output = keras.layers.Dense(input_dim, activation='linear')(x)

    decoder = keras.Model(decoder_input, decoder_output, name='decoder')

    # Autoencoder
    autoencoder_input = keras.Input(shape=(input_dim,))
    encoded = encoder(autoencoder_input)
    decoded = decoder(encoded)

    autoencoder = keras.Model(autoencoder_input, decoded, name='autoencoder')
    autoencoder.compile(optimizer='adam', loss='mse')

    return autoencoder, encoder
```

---

## 4. Performance Predictor (Lowest Priority)

### Overview
Predict entity performance for upcoming games based on historical performance, opponent strength, and contextual factors.

**Example Output:**
```
LeBron James vs Warriors (Jan 20):
├── Projected Points:  27.5 (range: 22-33)
├── Projected Rebounds: 8.2 (range: 6-11)
├── Projected Assists:  7.8 (range: 5-10)
├── Confidence: Medium (72%)
└── Key Factors: Back-to-back game, strong opponent defense
```

### Model Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                PERFORMANCE PREDICTOR                        │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌──────────────────────────────────────────────────────┐  │
│  │         HISTORICAL PERFORMANCE (LSTM)                │  │
│  │  Last 10-20 games: stats, opponent, home/away        │  │
│  └──────────────────────────────────────────────────────┘  │
│                          │                                  │
│                          ▼                                  │
│  ┌──────────────────────────────────────────────────────┐  │
│  │         CONTEXTUAL FEATURES                          │  │
│  │  • Days rest                                         │  │
│  │  • Home/Away                                         │  │
│  │  • Opponent defensive rating                         │  │
│  │  • Back-to-back flag                                 │  │
│  │  • Season progress (early/mid/late)                  │  │
│  └──────────────────────────────────────────────────────┘  │
│                          │                                  │
│                          ▼                                  │
│  ┌──────────────────────────────────────────────────────┐  │
│  │         PREDICTION MODEL                             │  │
│  │  Multi-output regression (one per stat)              │  │
│  │  LSTM + Dense layers                                 │  │
│  └──────────────────────────────────────────────────────┘  │
│                          │                                  │
│                          ▼                                  │
│  ┌──────────────────────────────────────────────────────┐  │
│  │         OUTPUT                                       │  │
│  │  • Point estimates per stat                          │  │
│  │  • Confidence intervals                              │  │
│  │  • Feature importance                                │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### Database Schema

```sql
-- Store predictions
CREATE TABLE performance_predictions (
    id SERIAL PRIMARY KEY,
    entity_type VARCHAR(10) NOT NULL,
    entity_id INTEGER NOT NULL,
    opponent_id INTEGER,
    game_date DATE NOT NULL,
    sport VARCHAR(20) NOT NULL,
    predictions JSONB NOT NULL, -- {"points": 27.5, "rebounds": 8.2, ...}
    confidence_intervals JSONB, -- {"points": [22, 33], ...}
    confidence_score FLOAT,
    context_factors JSONB,
    model_version VARCHAR(50),
    predicted_at TIMESTAMP DEFAULT NOW(),
    -- Actuals (filled in after game)
    actuals JSONB,
    accuracy_score FLOAT
);

-- Track model accuracy over time
CREATE TABLE prediction_accuracy (
    id SERIAL PRIMARY KEY,
    model_version VARCHAR(50),
    sport VARCHAR(20),
    entity_type VARCHAR(10),
    stat_name VARCHAR(50),
    mae FLOAT, -- Mean Absolute Error
    rmse FLOAT, -- Root Mean Squared Error
    within_range_pct FLOAT, -- % of actuals within predicted range
    sample_size INTEGER,
    calculated_at TIMESTAMP DEFAULT NOW()
);
```

### API Endpoints

```python
@router.get("/predictions/{entity_type}/{entity_id}/next")
async def get_next_game_prediction(entity_type: str, entity_id: int) -> GamePrediction:
    """
    Returns prediction for entity's next scheduled game.
    """

@router.get("/predictions/{entity_type}/{entity_id}/game/{game_id}")
async def get_specific_game_prediction(entity_type: str, entity_id: int, game_id: int) -> GamePrediction:
    """
    Returns prediction for a specific upcoming game.
    """

@router.get("/predictions/accuracy/{model_version}")
async def get_model_accuracy(model_version: str) -> ModelAccuracy:
    """
    Returns accuracy metrics for a model version.
    """
```

### Implementation Phases

#### Phase 4A: Data Preparation
- [ ] Build game-by-game feature extraction
- [ ] Create opponent strength features
- [ ] Implement contextual feature engineering
- [ ] Build training dataset (2+ seasons)

#### Phase 4B: Model Development
- [ ] Build LSTM architecture
- [ ] Train multi-output regression model
- [ ] Implement confidence interval estimation
- [ ] Evaluate on holdout data

#### Phase 4C: Prediction Pipeline
- [ ] Create scheduled prediction job (daily)
- [ ] Implement actual vs predicted tracking
- [ ] Build accuracy monitoring

#### Phase 4D: API & Integration
- [ ] Implement prediction endpoints
- [ ] Add to game preview responses
- [ ] Create accuracy dashboard

---

## Infrastructure & Shared Components

### Project Structure

```
scoracle-data/
├── ml/
│   ├── __init__.py
│   ├── config.py                 # ML configuration
│   ├── models/
│   │   ├── __init__.py
│   │   ├── transfer_predictor.py
│   │   ├── sentiment_analyzer.py
│   │   ├── similarity_engine.py
│   │   └── performance_predictor.py
│   ├── pipelines/
│   │   ├── __init__.py
│   │   ├── feature_engineering.py
│   │   ├── text_processing.py
│   │   └── data_loaders.py
│   ├── training/
│   │   ├── __init__.py
│   │   ├── train_transfer.py
│   │   ├── train_similarity.py
│   │   └── train_performance.py
│   └── inference/
│       ├── __init__.py
│       ├── model_registry.py
│       └── prediction_service.py
├── api/
│   ├── routers/
│   │   ├── ml_router.py          # New ML endpoints
│   │   └── ... (existing routers)
├── jobs/
│   ├── ml_jobs.py                # Scheduled ML tasks
│   └── ... (existing jobs)
└── models/
    └── ml_models.py              # Pydantic models for ML responses
```

### Dependencies to Add

```
# requirements.txt additions
tensorflow>=2.15.0
tensorflow-hub>=0.15.0
transformers>=4.36.0  # For pre-trained NLP models
numpy>=1.24.0
pandas>=2.0.0
scikit-learn>=1.3.0   # For preprocessing utilities
```

### Model Storage & Versioning

```python
# ml/config.py
ML_CONFIG = {
    "model_storage": {
        "local_path": "./ml_models",
        "remote_path": "s3://scoracle-ml-models",  # Future
    },
    "model_versions": {
        "transfer_predictor": "v1.0.0",
        "sentiment_analyzer": "v1.0.0",
        "similarity_engine": "v1.0.0",
        "performance_predictor": "v1.0.0",
    },
    "inference": {
        "batch_size": 32,
        "cache_ttl_seconds": 3600,
    }
}
```

### Scheduled Jobs

```python
# jobs/ml_jobs.py
ML_JOBS = [
    {
        "name": "transfer_mention_scan",
        "function": "scan_transfer_mentions",
        "schedule": "*/15 * * * *",  # Every 15 minutes
        "priority": 1
    },
    {
        "name": "transfer_prediction_refresh",
        "function": "refresh_transfer_predictions",
        "schedule": "0 * * * *",  # Every hour
        "priority": 1
    },
    {
        "name": "vibe_score_calculation",
        "function": "calculate_vibe_scores",
        "schedule": "0 */4 * * *",  # Every 4 hours
        "priority": 2
    },
    {
        "name": "similarity_recomputation",
        "function": "recompute_similarities",
        "schedule": "0 3 * * *",  # Daily at 3 AM
        "priority": 3
    },
    {
        "name": "performance_predictions",
        "function": "generate_performance_predictions",
        "schedule": "0 6 * * *",  # Daily at 6 AM
        "priority": 4
    }
]
```

---

## Implementation Timeline

### Phase 1: Transfer Predictor (TOP PRIORITY)
- **1A**: Data collection pipeline (mention extraction, source classification)
- **1B**: Historical data seeding
- **1C**: Model development & training
- **1D**: API integration & launch

### Phase 2: Vibe Score
- **2A**: Sentiment model setup
- **2B**: Data aggregation pipeline
- **2C**: Score calculation logic
- **2D**: API endpoints & caching

### Phase 3: Entity Similarity
- **3A**: Feature engineering
- **3B**: Embedding model training
- **3C**: Similarity computation
- **3D**: API integration

### Phase 4: Performance Predictor
- **4A**: Data preparation
- **4B**: Model development
- **4C**: Prediction pipeline
- **4D**: API integration

---

## Success Metrics

| Feature | Metric | Target |
|---------|--------|--------|
| Transfer Predictor | Precision@10 | >70% |
| Transfer Predictor | Calibration | <5% deviation |
| Vibe Score | Correlation with fan polls | >0.75 |
| Vibe Score | Latency | <200ms |
| Similarity Engine | User satisfaction (A/B) | >80% agree |
| Similarity Engine | Computation time | <50ms lookup |
| Performance Predictor | MAE (points) | <4.0 |
| Performance Predictor | Within-range % | >65% |

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Insufficient training data | Start with rule-based baseline, enhance with ML |
| API rate limits (Twitter/News) | Implement aggressive caching, batch requests |
| Model drift | Monitor accuracy, retrain monthly |
| Cold start (new players) | Fall back to position/team averages |
| Compute costs | Use TF Lite for inference, batch predictions |

---

## Next Steps

1. **Immediate**: Set up ML project structure and dependencies
2. **Week 1-2**: Build transfer mention extraction pipeline
3. **Week 3-4**: Develop transfer predictor model
4. **Week 5-6**: Implement vibe score system
5. **Week 7-8**: Build similarity engine
6. **Future**: Performance predictor (after schedule integration)

---

*Document Version: 1.0*
*Last Updated: January 2025*
*Status: Planning*
