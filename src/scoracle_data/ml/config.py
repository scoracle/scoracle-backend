"""
ML Configuration for Scoracle Data

Contains model configuration, source tiers, and feature definitions.
"""

from dataclasses import dataclass, field
from typing import Any
from pathlib import Path


# Source credibility tiers for transfer rumors
SOURCE_TIERS: dict[str, list[str]] = {
    # Tier 1: Official + Top Journalists (weight: 1.0)
    "tier_1": [
        "official_club",
        "fabrizio_romano",
        "david_ornstein",
        "matt_law",
        "gianluca_di_marzio",
        "paul_joyce",
        "james_pearce",
    ],
    # Tier 2: Reliable National Media (weight: 0.7)
    "tier_2": [
        "bbc_sport",
        "bbc.com",
        "sky_sports",
        "skysports.com",
        "the_athletic",
        "theathletic.com",
        "espn",
        "espn.com",
        "guardian",
        "theguardian.com",
    ],
    # Tier 3: National Newspapers (weight: 0.4)
    "tier_3": [
        "telegraph",
        "telegraph.co.uk",
        "times",
        "thetimes.co.uk",
        "mirror",
        "mirror.co.uk",
        "dailymail",
        "dailymail.co.uk",
        "sun",
        "thesun.co.uk",
    ],
    # Tier 4: Aggregators + Rumors (weight: 0.15)
    "tier_4": [
        "90min",
        "90min.com",
        "football_italia",
        "footballitalia.it",
        "reddit",
        "reddit.com",
        "twitter",
        "x.com",
        "transfermarkt",
        "transfermarkt.com",
    ],
}

# Tier weights for scoring
TIER_WEIGHTS: dict[str, float] = {
    "tier_1": 1.0,
    "tier_2": 0.7,
    "tier_3": 0.4,
    "tier_4": 0.15,
}


# Transfer rumor keywords for detection
TRANSFER_KEYWORDS: list[str] = [
    # Transfer-related
    "transfer",
    "signing",
    "signs",
    "signed",
    "deal",
    "contract",
    "fee",
    "bid",
    "offer",
    "target",
    "interest",
    "interested",
    "linked",
    "link",
    "move",
    "moving",
    "join",
    "joining",
    "loan",
    "permanent",
    "negotiate",
    "negotiating",
    "talks",
    "discussions",
    "close",
    "agreed",
    "agreement",
    "done deal",
    "here we go",
    "medical",
    "announce",
    "announced",
    "confirm",
    "confirmed",
    # Trade-related (US sports)
    "trade",
    "traded",
    "trading",
    "swap",
    "exchange",
    "package",
    "picks",
    "draft",
    "free agent",
    "free agency",
    "waived",
    "released",
    "buyout",
]


# Vibe score scale labels
VIBE_LABELS: dict[tuple[int, int], str] = {
    (90, 100): "Elite",
    (75, 89): "Positive",
    (60, 74): "Neutral-Positive",
    (40, 59): "Neutral",
    (25, 39): "Neutral-Negative",
    (10, 24): "Negative",
    (0, 9): "Crisis",
}


@dataclass
class ModelConfig:
    """Configuration for a specific ML model."""

    name: str
    version: str
    local_path: str
    input_shape: tuple[int, ...] | None = None
    batch_size: int = 32
    cache_ttl_seconds: int = 3600


@dataclass
class MLConfig:
    """Main ML configuration."""

    # Model storage paths
    model_storage_local: Path = field(
        default_factory=lambda: Path("./ml_models")
    )
    model_storage_remote: str = "s3://scoracle-ml-models"  # Future

    # Model configurations
    models: dict[str, ModelConfig] = field(default_factory=lambda: {
        "transfer_predictor": ModelConfig(
            name="transfer_predictor",
            version="v1.0.0",
            local_path="transfer_predictor",
            input_shape=(512 + 15 + 150,),  # text + numerical + history
            batch_size=32,
            cache_ttl_seconds=3600,
        ),
        "sentiment_analyzer": ModelConfig(
            name="sentiment_analyzer",
            version="v1.0.0",
            local_path="sentiment_analyzer",
            input_shape=(512,),  # text embedding
            batch_size=64,
            cache_ttl_seconds=1800,
        ),
        "similarity_engine": ModelConfig(
            name="similarity_engine",
            version="v1.0.0",
            local_path="similarity_engine",
            input_shape=None,  # Variable based on sport
            batch_size=128,
            cache_ttl_seconds=86400,  # Daily recomputation
        ),
        "performance_predictor": ModelConfig(
            name="performance_predictor",
            version="v1.0.0",
            local_path="performance_predictor",
            input_shape=None,  # Variable based on sport/position
            batch_size=32,
            cache_ttl_seconds=3600,
        ),
    })

    # Inference settings
    inference_batch_size: int = 32
    inference_cache_ttl: int = 3600

    # Feature engineering settings
    mention_windows: dict[str, int] = field(default_factory=lambda: {
        "short": 24,    # 24 hours
        "medium": 168,  # 7 days
        "long": 720,    # 30 days
    })

    # Sentiment thresholds
    sentiment_positive_threshold: float = 0.6
    sentiment_negative_threshold: float = 0.4

    # Similarity settings
    similarity_top_k: int = 3
    similarity_min_score: float = 0.5


# Global config instance
ML_CONFIG = MLConfig()


# Feature definitions by sport
NBA_PLAYER_FEATURES: list[str] = [
    # Scoring
    "ppg", "fg_pct", "fg3_pct", "ft_pct", "ts_pct",
    # Rebounding
    "rpg", "orpg", "drpg",
    # Playmaking
    "apg", "ast_to_ratio", "tov_pg",
    # Defense
    "spg", "bpg",
    # Usage & Efficiency
    "usg_pct", "per", "mpg",
    # Shot profile
    "fg3a_rate", "fta_rate",
]

NBA_TEAM_FEATURES: list[str] = [
    "offensive_rating", "defensive_rating", "net_rating",
    "pace", "ts_pct", "efg_pct",
    "tov_pct", "orb_pct", "ft_rate",
    "fg3a_rate", "ppg", "opp_ppg",
]

NFL_QB_FEATURES: list[str] = [
    "pass_yds_pg", "pass_td_pg", "int_pg",
    "completion_pct", "passer_rating", "qbr",
    "rush_yds_pg", "rush_td_pg",
    "sack_pct", "ypa",
]

NFL_RB_FEATURES: list[str] = [
    "rush_yds_pg", "rush_td_pg", "ypc",
    "rec_yds_pg", "rec_td_pg", "targets_pg",
    "fumbles_pg", "touches_pg",
]

NFL_WR_FEATURES: list[str] = [
    "rec_yds_pg", "rec_td_pg", "receptions_pg",
    "targets_pg", "catch_pct", "ypr",
    "yac_pg", "air_yds_pg",
]

FOOTBALL_PLAYER_FEATURES: list[str] = [
    # Attacking
    "goals", "assists", "shots_pg", "shots_on_target_pg",
    "xg", "xa", "npxg",
    # Passing
    "pass_completion_pct", "key_passes_pg", "progressive_passes_pg",
    # Dribbling
    "successful_dribbles_pg", "dribble_success_pct",
    # Defense
    "tackles_pg", "interceptions_pg", "clearances_pg",
    # Physical
    "duels_won_pct", "aerial_duels_won_pct",
]

FOOTBALL_TEAM_FEATURES: list[str] = [
    "goals_scored", "goals_conceded", "goal_difference",
    "xg", "xga", "npxg",
    "possession_pct", "pass_completion_pct",
    "shots_pg", "shots_against_pg",
    "clean_sheets", "ppg",
]

# Feature set lookup by sport and entity type
FEATURE_SETS: dict[str, dict[str, list[str]]] = {
    "nba": {
        "player": NBA_PLAYER_FEATURES,
        "team": NBA_TEAM_FEATURES,
    },
    "nfl": {
        "qb": NFL_QB_FEATURES,
        "rb": NFL_RB_FEATURES,
        "wr": NFL_WR_FEATURES,
    },
    "football": {
        "player": FOOTBALL_PLAYER_FEATURES,
        "team": FOOTBALL_TEAM_FEATURES,
    },
}


def get_tier_for_source(source: str) -> tuple[int, float]:
    """
    Get tier number and weight for a given source.

    Args:
        source: Source name or domain

    Returns:
        Tuple of (tier_number, weight). Returns (4, 0.15) if not found.
    """
    source_lower = source.lower()
    for tier_name, sources in SOURCE_TIERS.items():
        for s in sources:
            if s in source_lower or source_lower in s:
                tier_num = int(tier_name.split("_")[1])
                return tier_num, TIER_WEIGHTS[tier_name]
    return 4, TIER_WEIGHTS["tier_4"]


def get_vibe_label(score: float) -> str:
    """
    Get the vibe label for a given score.

    Args:
        score: Vibe score (0-100)

    Returns:
        Label string (e.g., "Positive", "Neutral", etc.)
    """
    score_int = int(score)
    for (low, high), label in VIBE_LABELS.items():
        if low <= score_int <= high:
            return label
    return "Unknown"


def get_features_for_entity(sport: str, entity_type: str, position: str | None = None) -> list[str]:
    """
    Get the feature list for a given sport and entity type.

    Args:
        sport: Sport name (e.g., "nba", "nfl", "football")
        entity_type: Entity type (e.g., "player", "team")
        position: Position for position-specific features (NFL)

    Returns:
        List of feature names
    """
    sport_lower = sport.lower()
    if sport_lower not in FEATURE_SETS:
        raise ValueError(f"Unknown sport: {sport}")

    sport_features = FEATURE_SETS[sport_lower]

    # NFL has position-specific features
    if sport_lower == "nfl" and position:
        pos_lower = position.lower()
        if pos_lower in sport_features:
            return sport_features[pos_lower]

    if entity_type not in sport_features:
        raise ValueError(f"Unknown entity type for {sport}: {entity_type}")

    return sport_features[entity_type]
