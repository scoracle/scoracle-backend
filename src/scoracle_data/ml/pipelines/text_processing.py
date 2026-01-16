"""
Text Processing Pipeline for Scoracle ML

Handles:
- Entity extraction (player/team NER)
- Link detection (player ↔ team mentions)
- Transfer rumor keyword identification
- Text preprocessing and normalization
"""

import re
from dataclasses import dataclass, field
from typing import Any

from ..config import TRANSFER_KEYWORDS, get_tier_for_source


@dataclass
class ExtractedEntity:
    """Represents an extracted entity from text."""

    name: str
    entity_type: str  # 'player' or 'team'
    confidence: float
    start_pos: int
    end_pos: int
    normalized_name: str | None = None
    entity_id: int | None = None


@dataclass
class TransferMention:
    """Represents a detected transfer mention."""

    player_name: str
    team_name: str
    headline: str
    source: str
    source_type: str  # 'news', 'twitter', 'reddit'
    source_tier: int
    source_weight: float
    sentiment_score: float | None = None
    keywords_found: list[str] = field(default_factory=list)
    confidence: float = 0.0
    url: str | None = None
    engagement_score: int = 0


class TextProcessor:
    """
    Text processing pipeline for ML features.

    Handles entity extraction, transfer mention detection,
    and text normalization for downstream ML models.
    """

    def __init__(
        self,
        player_names: list[str] | None = None,
        team_names: list[str] | None = None,
    ):
        """
        Initialize the text processor.

        Args:
            player_names: List of known player names for matching
            team_names: List of known team names for matching
        """
        self.player_names = set(player_names or [])
        self.team_names = set(team_names or [])

        # Build lookup dictionaries for fuzzy matching
        self._player_lookup: dict[str, str] = {}
        self._team_lookup: dict[str, str] = {}
        self._build_lookups()

        # Compile regex patterns
        self._transfer_pattern = re.compile(
            r"\b(" + "|".join(re.escape(kw) for kw in TRANSFER_KEYWORDS) + r")\b",
            re.IGNORECASE,
        )

    def _build_lookups(self) -> None:
        """Build lookup dictionaries for fuzzy name matching."""
        for name in self.player_names:
            normalized = self._normalize_name(name)
            self._player_lookup[normalized] = name
            # Also add last name only for common references
            parts = name.split()
            if len(parts) > 1:
                self._player_lookup[parts[-1].lower()] = name

        for name in self.team_names:
            normalized = self._normalize_name(name)
            self._team_lookup[normalized] = name
            # Add common variations
            self._add_team_variations(name)

    def _add_team_variations(self, team_name: str) -> None:
        """Add common team name variations to lookup."""
        variations = [
            team_name.lower(),
            team_name.replace(" FC", "").lower(),
            team_name.replace(" United", "").lower(),
            team_name.replace(" City", "").lower(),
        ]
        for var in variations:
            if var and var not in self._team_lookup:
                self._team_lookup[var] = team_name

    def _normalize_name(self, name: str) -> str:
        """Normalize a name for matching."""
        # Remove accents, lowercase, strip
        normalized = name.lower().strip()
        # Remove common suffixes
        for suffix in [" fc", " united", " city", " cf"]:
            normalized = normalized.replace(suffix, "")
        return normalized

    def update_entity_lists(
        self,
        player_names: list[str] | None = None,
        team_names: list[str] | None = None,
    ) -> None:
        """
        Update the entity lists for matching.

        Args:
            player_names: Updated list of player names
            team_names: Updated list of team names
        """
        if player_names:
            self.player_names = set(player_names)
        if team_names:
            self.team_names = set(team_names)
        self._build_lookups()

    def preprocess_text(self, text: str) -> str:
        """
        Preprocess text for analysis.

        Args:
            text: Raw text to preprocess

        Returns:
            Cleaned and normalized text
        """
        # Remove URLs
        text = re.sub(r"http\S+|www\.\S+", "", text)
        # Remove mentions and hashtags (keep the text)
        text = re.sub(r"[@#](\w+)", r"\1", text)
        # Normalize whitespace
        text = re.sub(r"\s+", " ", text).strip()
        return text

    def extract_entities(self, text: str) -> list[ExtractedEntity]:
        """
        Extract player and team entities from text.

        Args:
            text: Text to extract entities from

        Returns:
            List of extracted entities
        """
        entities = []
        text_lower = text.lower()

        # Extract players
        for normalized, original in self._player_lookup.items():
            if normalized in text_lower:
                # Find position
                pos = text_lower.find(normalized)
                entities.append(
                    ExtractedEntity(
                        name=original,
                        entity_type="player",
                        confidence=0.9 if len(normalized.split()) > 1 else 0.7,
                        start_pos=pos,
                        end_pos=pos + len(normalized),
                        normalized_name=normalized,
                    )
                )

        # Extract teams
        for normalized, original in self._team_lookup.items():
            if normalized in text_lower:
                pos = text_lower.find(normalized)
                entities.append(
                    ExtractedEntity(
                        name=original,
                        entity_type="team",
                        confidence=0.9,
                        start_pos=pos,
                        end_pos=pos + len(normalized),
                        normalized_name=normalized,
                    )
                )

        return entities

    def detect_transfer_keywords(self, text: str) -> list[str]:
        """
        Detect transfer-related keywords in text.

        Args:
            text: Text to analyze

        Returns:
            List of found keywords
        """
        matches = self._transfer_pattern.findall(text.lower())
        return list(set(matches))

    def extract_transfer_mentions(
        self,
        headline: str,
        content: str | None = None,
        source: str = "",
        source_type: str = "news",
        url: str | None = None,
        engagement_score: int = 0,
    ) -> list[TransferMention]:
        """
        Extract transfer mentions from news/social content.

        Args:
            headline: Article headline or tweet text
            content: Optional article body or additional text
            source: Source name (e.g., "Sky Sports")
            source_type: Type of source ('news', 'twitter', 'reddit')
            url: Source URL
            engagement_score: Engagement metric (likes, retweets, etc.)

        Returns:
            List of detected transfer mentions
        """
        mentions = []

        # Combine headline and content for analysis
        full_text = f"{headline} {content or ''}"
        processed_text = self.preprocess_text(full_text)

        # Extract entities
        entities = self.extract_entities(processed_text)
        players = [e for e in entities if e.entity_type == "player"]
        teams = [e for e in entities if e.entity_type == "team"]

        # Detect keywords
        keywords = self.detect_transfer_keywords(processed_text)

        # No transfer mentions if no keywords found
        if not keywords:
            return mentions

        # Get source tier
        tier, weight = get_tier_for_source(source)

        # Create mentions for each player-team pair
        for player in players:
            for team in teams:
                # Calculate confidence based on various factors
                confidence = self._calculate_mention_confidence(
                    player=player,
                    team=team,
                    keywords=keywords,
                    tier=tier,
                    text_length=len(processed_text),
                )

                mentions.append(
                    TransferMention(
                        player_name=player.name,
                        team_name=team.name,
                        headline=headline,
                        source=source,
                        source_type=source_type,
                        source_tier=tier,
                        source_weight=weight,
                        keywords_found=keywords,
                        confidence=confidence,
                        url=url,
                        engagement_score=engagement_score,
                    )
                )

        return mentions

    def _calculate_mention_confidence(
        self,
        player: ExtractedEntity,
        team: ExtractedEntity,
        keywords: list[str],
        tier: int,
        text_length: int,
    ) -> float:
        """
        Calculate confidence score for a transfer mention.

        Args:
            player: Extracted player entity
            team: Extracted team entity
            keywords: Found transfer keywords
            tier: Source tier (1-4)
            text_length: Length of the text

        Returns:
            Confidence score (0.0 - 1.0)
        """
        confidence = 0.0

        # Base confidence from entity extraction
        confidence += (player.confidence + team.confidence) / 4  # Max 0.45

        # Keyword weight
        high_confidence_keywords = {
            "done deal", "here we go", "confirmed", "signed",
            "official", "medical", "announced", "agreed",
        }
        medium_confidence_keywords = {
            "close", "imminent", "negotiate", "talks", "bid", "offer",
        }

        for kw in keywords:
            kw_lower = kw.lower()
            if kw_lower in high_confidence_keywords:
                confidence += 0.2
            elif kw_lower in medium_confidence_keywords:
                confidence += 0.1
            else:
                confidence += 0.05

        # Source tier weight
        tier_bonus = {1: 0.2, 2: 0.15, 3: 0.1, 4: 0.05}
        confidence += tier_bonus.get(tier, 0.05)

        # Proximity bonus: entities mentioned close together
        distance = abs(player.start_pos - team.start_pos)
        if distance < 50:
            confidence += 0.1
        elif distance < 100:
            confidence += 0.05

        return min(confidence, 1.0)

    def batch_process(
        self,
        items: list[dict[str, Any]],
    ) -> list[list[TransferMention]]:
        """
        Process multiple items in batch.

        Args:
            items: List of dicts with 'headline', 'content', 'source', etc.

        Returns:
            List of mention lists for each item
        """
        results = []
        for item in items:
            mentions = self.extract_transfer_mentions(
                headline=item.get("headline", item.get("title", "")),
                content=item.get("content", item.get("description", "")),
                source=item.get("source", ""),
                source_type=item.get("source_type", "news"),
                url=item.get("url"),
                engagement_score=item.get("engagement_score", 0),
            )
            results.append(mentions)
        return results


class SentimentTextProcessor:
    """
    Text processor specialized for sentiment analysis.

    Prepares text for sentiment model input.
    """

    def __init__(self, max_length: int = 512):
        """
        Initialize sentiment text processor.

        Args:
            max_length: Maximum text length for model input
        """
        self.max_length = max_length

    def preprocess_for_sentiment(self, text: str) -> str:
        """
        Preprocess text for sentiment analysis.

        Args:
            text: Raw text to preprocess

        Returns:
            Cleaned text ready for sentiment model
        """
        # Remove URLs
        text = re.sub(r"http\S+|www\.\S+", "", text)
        # Keep hashtags but remove the # symbol
        text = re.sub(r"#(\w+)", r"\1", text)
        # Remove @mentions
        text = re.sub(r"@\w+", "", text)
        # Remove extra whitespace
        text = re.sub(r"\s+", " ", text).strip()
        # Truncate if needed
        if len(text) > self.max_length:
            text = text[: self.max_length]
        return text

    def batch_preprocess(self, texts: list[str]) -> list[str]:
        """
        Preprocess multiple texts for sentiment analysis.

        Args:
            texts: List of raw texts

        Returns:
            List of preprocessed texts
        """
        return [self.preprocess_for_sentiment(t) for t in texts]
