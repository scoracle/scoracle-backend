"""
Sentiment Analyzer for Vibe Scores

Uses pre-trained transformer models for sentiment analysis
of sports-related social media and news content.
"""

from dataclasses import dataclass
from typing import Any

import numpy as np

from ..config import ML_CONFIG, get_vibe_label


@dataclass
class SentimentResult:
    """Result of sentiment analysis."""

    text: str
    label: str  # 'positive', 'negative', 'neutral'
    score: float  # -1.0 to 1.0
    confidence: float  # 0.0 to 1.0


@dataclass
class VibeScore:
    """Aggregated vibe score for an entity."""

    entity_id: int
    entity_name: str
    entity_type: str  # 'player' or 'team'
    sport: str
    overall_score: float  # 0-100
    vibe_label: str
    breakdown: dict[str, dict[str, float]]  # source -> {score, sample_size}
    trend: dict[str, Any]  # direction, change_7d, change_30d
    positive_themes: list[str]
    negative_themes: list[str]


class SentimentAnalyzer:
    """
    Sentiment analyzer for vibe score calculation.

    Uses pre-trained transformers for sentiment classification
    and aggregates scores across multiple sources.
    """

    def __init__(self, model_name: str | None = None):
        """
        Initialize sentiment analyzer.

        Args:
            model_name: HuggingFace model name for sentiment analysis
        """
        self._model = None
        self._tokenizer = None
        self._model_name = model_name or "cardiffnlp/twitter-roberta-base-sentiment-latest"
        self._version = ML_CONFIG.models["sentiment_analyzer"].version
        self._initialized = False

    def _initialize_model(self) -> bool:
        """
        Initialize the sentiment model.

        Returns:
            True if successful, False otherwise
        """
        if self._initialized:
            return self._model is not None

        try:
            from transformers import AutoModelForSequenceClassification, AutoTokenizer

            self._tokenizer = AutoTokenizer.from_pretrained(self._model_name)
            self._model = AutoModelForSequenceClassification.from_pretrained(self._model_name)
            self._initialized = True
            return True
        except ImportError:
            self._initialized = True
            return False
        except Exception:
            self._initialized = True
            return False

    def analyze(self, text: str) -> SentimentResult:
        """
        Analyze sentiment of a single text.

        Args:
            text: Text to analyze

        Returns:
            SentimentResult
        """
        if not self._initialize_model():
            return self._analyze_heuristic(text)

        try:
            import torch

            # Tokenize
            inputs = self._tokenizer(
                text,
                return_tensors="pt",
                truncation=True,
                max_length=512,
                padding=True,
            )

            # Get predictions
            with torch.no_grad():
                outputs = self._model(**inputs)
                scores = torch.softmax(outputs.logits, dim=1).numpy()[0]

            # Model outputs: negative, neutral, positive
            label_map = {0: "negative", 1: "neutral", 2: "positive"}
            predicted_class = int(np.argmax(scores))
            label = label_map[predicted_class]

            # Convert to -1 to 1 scale
            # negative = -1, neutral = 0, positive = 1
            score = float(scores[2] - scores[0])  # positive - negative
            confidence = float(scores[predicted_class])

            return SentimentResult(
                text=text,
                label=label,
                score=score,
                confidence=confidence,
            )
        except Exception:
            return self._analyze_heuristic(text)

    def _analyze_heuristic(self, text: str) -> SentimentResult:
        """
        Simple heuristic sentiment analysis.

        Uses keyword matching as fallback when model unavailable.
        """
        text_lower = text.lower()

        # Positive keywords
        positive_words = {
            "great", "amazing", "excellent", "fantastic", "awesome",
            "best", "win", "winner", "victory", "champion", "love",
            "incredible", "outstanding", "brilliant", "superb",
            "perfect", "happy", "thrilled", "excited", "proud",
            "clutch", "dominant", "elite", "goat", "legend",
        }

        # Negative keywords
        negative_words = {
            "bad", "terrible", "awful", "worst", "poor", "lose",
            "loser", "lost", "failure", "disappointing", "hate",
            "horrible", "pathetic", "embarrassing", "shameful",
            "trash", "garbage", "overrated", "choke", "bust",
            "injury", "injured", "hurt", "out", "suspended",
        }

        # Count matches
        positive_count = sum(1 for word in positive_words if word in text_lower)
        negative_count = sum(1 for word in negative_words if word in text_lower)

        total = positive_count + negative_count

        if total == 0:
            return SentimentResult(
                text=text,
                label="neutral",
                score=0.0,
                confidence=0.5,
            )

        score = (positive_count - negative_count) / total

        if score > 0.2:
            label = "positive"
        elif score < -0.2:
            label = "negative"
        else:
            label = "neutral"

        return SentimentResult(
            text=text,
            label=label,
            score=score,
            confidence=0.6,  # Lower confidence for heuristic
        )

    def batch_analyze(self, texts: list[str]) -> list[SentimentResult]:
        """
        Analyze sentiment of multiple texts.

        Args:
            texts: List of texts to analyze

        Returns:
            List of SentimentResult
        """
        if not self._initialize_model():
            return [self._analyze_heuristic(t) for t in texts]

        try:
            import torch

            results = []

            # Process in batches
            batch_size = 32
            for i in range(0, len(texts), batch_size):
                batch = texts[i:i + batch_size]

                inputs = self._tokenizer(
                    batch,
                    return_tensors="pt",
                    truncation=True,
                    max_length=512,
                    padding=True,
                )

                with torch.no_grad():
                    outputs = self._model(**inputs)
                    scores = torch.softmax(outputs.logits, dim=1).numpy()

                for j, text in enumerate(batch):
                    text_scores = scores[j]
                    label_map = {0: "negative", 1: "neutral", 2: "positive"}
                    predicted_class = int(np.argmax(text_scores))

                    results.append(SentimentResult(
                        text=text,
                        label=label_map[predicted_class],
                        score=float(text_scores[2] - text_scores[0]),
                        confidence=float(text_scores[predicted_class]),
                    ))

            return results
        except Exception:
            return [self._analyze_heuristic(t) for t in texts]

    def calculate_vibe_score(
        self,
        entity_id: int,
        entity_name: str,
        entity_type: str,
        sport: str,
        twitter_texts: list[str] | None = None,
        news_texts: list[str] | None = None,
        reddit_texts: list[str] | None = None,
        twitter_engagements: list[int] | None = None,
        previous_score: float | None = None,
    ) -> VibeScore:
        """
        Calculate aggregated vibe score for an entity.

        Args:
            entity_id: Entity ID
            entity_name: Entity name
            entity_type: 'player' or 'team'
            sport: Sport name
            twitter_texts: List of tweets
            news_texts: List of news headlines/excerpts
            reddit_texts: List of reddit comments
            twitter_engagements: List of engagement scores for tweets
            previous_score: Previous vibe score for trend calculation

        Returns:
            VibeScore result
        """
        breakdown = {}
        all_sentiments = []
        positive_words = []
        negative_words = []

        # Analyze Twitter
        if twitter_texts:
            twitter_results = self.batch_analyze(twitter_texts)
            twitter_scores = [r.score for r in twitter_results]

            # Weight by engagement if available
            if twitter_engagements and len(twitter_engagements) == len(twitter_scores):
                weights = np.array(twitter_engagements, dtype=float)
                weights = weights / (weights.sum() + 1e-6)  # Normalize
                weighted_score = float(np.sum(np.array(twitter_scores) * weights))
            else:
                weighted_score = float(np.mean(twitter_scores))

            # Convert to 0-100 scale
            twitter_vibe = (weighted_score + 1) * 50

            breakdown["twitter"] = {
                "score": twitter_vibe,
                "sample_size": len(twitter_texts),
            }
            all_sentiments.extend(twitter_scores)

            # Extract themes
            for r in twitter_results:
                if r.label == "positive":
                    positive_words.extend(self._extract_keywords(r.text))
                elif r.label == "negative":
                    negative_words.extend(self._extract_keywords(r.text))

        # Analyze News
        if news_texts:
            news_results = self.batch_analyze(news_texts)
            news_scores = [r.score for r in news_results]
            news_vibe = (float(np.mean(news_scores)) + 1) * 50

            breakdown["news"] = {
                "score": news_vibe,
                "sample_size": len(news_texts),
            }
            all_sentiments.extend(news_scores)

            for r in news_results:
                if r.label == "positive":
                    positive_words.extend(self._extract_keywords(r.text))
                elif r.label == "negative":
                    negative_words.extend(self._extract_keywords(r.text))

        # Analyze Reddit
        if reddit_texts:
            reddit_results = self.batch_analyze(reddit_texts)
            reddit_scores = [r.score for r in reddit_results]
            reddit_vibe = (float(np.mean(reddit_scores)) + 1) * 50

            breakdown["reddit"] = {
                "score": reddit_vibe,
                "sample_size": len(reddit_texts),
            }
            all_sentiments.extend(reddit_scores)

            for r in reddit_results:
                if r.label == "positive":
                    positive_words.extend(self._extract_keywords(r.text))
                elif r.label == "negative":
                    negative_words.extend(self._extract_keywords(r.text))

        # Calculate overall score
        if all_sentiments:
            # Weight sources: News > Twitter > Reddit
            weights = {"twitter": 0.35, "news": 0.4, "reddit": 0.25}
            weighted_sum = 0.0
            weight_total = 0.0

            for source, data in breakdown.items():
                w = weights.get(source, 0.33)
                weighted_sum += data["score"] * w
                weight_total += w

            overall_score = weighted_sum / weight_total if weight_total > 0 else 50.0
        else:
            overall_score = 50.0  # Neutral if no data

        # Calculate trend
        trend = {"direction": "stable", "change_7d": 0.0, "change_30d": 0.0}
        if previous_score is not None:
            change = overall_score - previous_score
            trend["change_7d"] = change
            if change > 3:
                trend["direction"] = "up"
            elif change < -3:
                trend["direction"] = "down"

        # Get top themes
        positive_themes = self._get_top_themes(positive_words, 3)
        negative_themes = self._get_top_themes(negative_words, 3)

        vibe_label = get_vibe_label(overall_score)

        return VibeScore(
            entity_id=entity_id,
            entity_name=entity_name,
            entity_type=entity_type,
            sport=sport,
            overall_score=round(overall_score, 1),
            vibe_label=vibe_label,
            breakdown=breakdown,
            trend=trend,
            positive_themes=positive_themes,
            negative_themes=negative_themes,
        )

    def _extract_keywords(self, text: str, max_words: int = 5) -> list[str]:
        """Extract potential theme keywords from text."""
        # Simple keyword extraction
        stopwords = {
            "the", "a", "an", "is", "are", "was", "were", "be", "been",
            "have", "has", "had", "do", "does", "did", "will", "would",
            "could", "should", "may", "might", "must", "shall", "can",
            "to", "of", "in", "for", "on", "with", "at", "by", "from",
            "up", "about", "into", "over", "after", "he", "she", "it",
            "they", "we", "you", "i", "this", "that", "these", "those",
            "and", "but", "or", "nor", "so", "yet", "both", "either",
        }

        words = text.lower().split()
        keywords = [
            w.strip(".,!?;:\"'()[]{}") for w in words
            if w.lower() not in stopwords and len(w) > 3
        ]
        return keywords[:max_words]

    def _get_top_themes(self, words: list[str], top_k: int = 3) -> list[str]:
        """Get top occurring themes."""
        if not words:
            return []

        from collections import Counter

        counts = Counter(words)
        return [word for word, _ in counts.most_common(top_k)]
