"""
ML Job Scheduler

Orchestrates ML background jobs with scheduling and logging.
"""

import asyncio
import logging
from dataclasses import dataclass, field
from datetime import datetime, timedelta
from enum import Enum
from typing import Any, Callable

logger = logging.getLogger(__name__)


class JobStatus(Enum):
    """Job execution status."""

    PENDING = "pending"
    RUNNING = "running"
    COMPLETED = "completed"
    FAILED = "failed"
    SKIPPED = "skipped"


@dataclass
class JobResult:
    """Result of a job execution."""

    job_name: str
    status: JobStatus
    started_at: datetime
    completed_at: datetime | None = None
    duration_seconds: float = 0.0
    items_processed: int = 0
    errors: list[str] = field(default_factory=list)
    metadata: dict[str, Any] = field(default_factory=dict)

    def to_dict(self) -> dict:
        """Convert to dictionary for storage."""
        return {
            "job_name": self.job_name,
            "status": self.status.value,
            "started_at": self.started_at.isoformat(),
            "completed_at": self.completed_at.isoformat() if self.completed_at else None,
            "duration_seconds": self.duration_seconds,
            "items_processed": self.items_processed,
            "errors": self.errors,
            "metadata": self.metadata,
        }


@dataclass
class JobConfig:
    """Configuration for a scheduled job."""

    name: str
    interval_minutes: int
    enabled: bool = True
    max_runtime_minutes: int = 30
    retry_on_failure: bool = True
    max_retries: int = 3


class MLJobScheduler:
    """
    Orchestrates ML background jobs.

    Jobs:
    - mention_scan: Scan news/social for transfer mentions (every 30 min)
    - prediction_refresh: Update transfer predictions (every hour)
    - vibe_calculate: Calculate vibe scores (every hour)
    - performance_predict: Pre-compute game predictions (every 6 hours)
    - similarity_update: Recompute similarities (daily)
    """

    # Default job configurations
    DEFAULT_JOBS = {
        "mention_scan": JobConfig(
            name="mention_scan",
            interval_minutes=30,
            max_runtime_minutes=15,
        ),
        "prediction_refresh": JobConfig(
            name="prediction_refresh",
            interval_minutes=60,
            max_runtime_minutes=30,
        ),
        "vibe_calculate": JobConfig(
            name="vibe_calculate",
            interval_minutes=60,
            max_runtime_minutes=20,
        ),
        "performance_predict": JobConfig(
            name="performance_predict",
            interval_minutes=360,  # 6 hours
            max_runtime_minutes=60,
        ),
        "similarity_update": JobConfig(
            name="similarity_update",
            interval_minutes=1440,  # Daily
            max_runtime_minutes=120,
        ),
    }

    def __init__(self, db: Any, config: dict | None = None):
        """
        Initialize job scheduler.

        Args:
            db: Database connection
            config: Optional configuration overrides
        """
        self.db = db
        self.config = config or {}
        self.jobs = dict(self.DEFAULT_JOBS)
        self._running = False
        self._tasks: dict[str, asyncio.Task] = {}

        # Apply config overrides
        for job_name, job_config in self.config.get("jobs", {}).items():
            if job_name in self.jobs:
                for key, value in job_config.items():
                    setattr(self.jobs[job_name], key, value)

    async def start(self) -> None:
        """Start the scheduler (runs all jobs on their intervals)."""
        if self._running:
            logger.warning("Scheduler already running")
            return

        self._running = True
        logger.info("Starting ML job scheduler")

        # Start each job's loop
        for job_name, job_config in self.jobs.items():
            if job_config.enabled:
                self._tasks[job_name] = asyncio.create_task(
                    self._job_loop(job_name, job_config)
                )

        logger.info(f"Started {len(self._tasks)} job loops")

    async def stop(self) -> None:
        """Stop the scheduler."""
        if not self._running:
            return

        self._running = False
        logger.info("Stopping ML job scheduler")

        # Cancel all tasks
        for task in self._tasks.values():
            task.cancel()

        # Wait for cancellation
        await asyncio.gather(*self._tasks.values(), return_exceptions=True)
        self._tasks.clear()

        logger.info("ML job scheduler stopped")

    async def run_job(self, job_name: str, **kwargs) -> JobResult:
        """
        Run a single job immediately.

        Args:
            job_name: Name of the job to run
            **kwargs: Additional arguments for the job

        Returns:
            Job result
        """
        if job_name not in self.jobs:
            return JobResult(
                job_name=job_name,
                status=JobStatus.FAILED,
                started_at=datetime.utcnow(),
                errors=[f"Unknown job: {job_name}"],
            )

        job_config = self.jobs[job_name]
        return await self._execute_job(job_name, job_config, **kwargs)

    async def run_all_jobs(self, **kwargs) -> dict[str, JobResult]:
        """
        Run all enabled jobs immediately.

        Args:
            **kwargs: Additional arguments passed to all jobs

        Returns:
            Dict mapping job name to result
        """
        results = {}

        for job_name, job_config in self.jobs.items():
            if job_config.enabled:
                results[job_name] = await self._execute_job(job_name, job_config, **kwargs)

        return results

    async def _job_loop(self, job_name: str, job_config: JobConfig) -> None:
        """Run a job on its configured interval."""
        while self._running:
            try:
                # Check if enough time has passed since last run
                if self._should_run(job_name, job_config):
                    await self._execute_job(job_name, job_config)

                # Sleep until next check
                await asyncio.sleep(60)  # Check every minute

            except asyncio.CancelledError:
                break
            except Exception as e:
                logger.error(f"Error in job loop for {job_name}: {e}")
                await asyncio.sleep(300)  # Wait 5 min on error

    def _should_run(self, job_name: str, job_config: JobConfig) -> bool:
        """Check if a job should run based on last execution."""
        last_run = self._get_last_run(job_name)

        if not last_run:
            return True

        elapsed = (datetime.utcnow() - last_run).total_seconds() / 60
        return elapsed >= job_config.interval_minutes

    def _get_last_run(self, job_name: str) -> datetime | None:
        """Get last successful run time for a job."""
        result = self.db.fetchone(
            """
            SELECT started_at FROM ml_job_runs
            WHERE job_name = %s AND status = 'completed'
            ORDER BY started_at DESC
            LIMIT 1
            """,
            (job_name,),
        )
        return result["started_at"] if result else None

    async def _execute_job(
        self,
        job_name: str,
        job_config: JobConfig,
        **kwargs,
    ) -> JobResult:
        """Execute a job and record the result."""
        started_at = datetime.utcnow()
        result = JobResult(
            job_name=job_name,
            status=JobStatus.RUNNING,
            started_at=started_at,
        )

        logger.info(f"Starting job: {job_name}")

        try:
            # Get the job executor
            executor = self._get_job_executor(job_name)

            # Run with timeout
            timeout = job_config.max_runtime_minutes * 60
            job_result = await asyncio.wait_for(
                executor(**kwargs),
                timeout=timeout,
            )

            result.status = JobStatus.COMPLETED
            result.items_processed = getattr(job_result, "items_processed", 0)
            result.metadata = getattr(job_result, "metadata", {})

            # Merge errors from job result if any
            if hasattr(job_result, "errors"):
                result.errors.extend(job_result.errors)

        except asyncio.TimeoutError:
            result.status = JobStatus.FAILED
            result.errors.append(f"Job timed out after {job_config.max_runtime_minutes} minutes")
            logger.error(f"Job {job_name} timed out")

        except Exception as e:
            result.status = JobStatus.FAILED
            result.errors.append(str(e))
            logger.error(f"Job {job_name} failed: {e}")

        result.completed_at = datetime.utcnow()
        result.duration_seconds = (result.completed_at - started_at).total_seconds()

        # Record the run
        self._record_run(result)

        logger.info(
            f"Job {job_name} {result.status.value}: "
            f"{result.items_processed} items in {result.duration_seconds:.1f}s"
        )

        return result

    def _get_job_executor(self, job_name: str) -> Callable:
        """Get the executor function for a job."""
        executors = {
            "mention_scan": self._run_mention_scan,
            "prediction_refresh": self._run_prediction_refresh,
            "vibe_calculate": self._run_vibe_calculate,
            "performance_predict": self._run_performance_predict,
            "similarity_update": self._run_similarity_update,
        }

        if job_name not in executors:
            raise ValueError(f"No executor for job: {job_name}")

        return executors[job_name]

    async def _run_mention_scan(self, sport_id: str | None = None, **kwargs) -> Any:
        """Run mention scanning job."""
        from .mention_scanner import MentionScanner

        scanner = MentionScanner(self.db)
        results = await scanner.scan_all_sources(sport_id)

        # Aggregate results
        total_found = sum(r.mentions_found for r in results.values())
        total_stored = sum(r.mentions_stored for r in results.values())
        all_errors = []
        for r in results.values():
            all_errors.extend(r.errors)

        return type("Result", (), {
            "items_processed": total_stored,
            "metadata": {"mentions_found": total_found, "sources": list(results.keys())},
            "errors": all_errors,
        })()

    async def _run_prediction_refresh(self, sport_id: str | None = None, **kwargs) -> Any:
        """Run prediction refresh job."""
        from .prediction_refresh import PredictionRefreshJob

        job = PredictionRefreshJob(self.db)
        result = job.run(sport_id)

        return type("Result", (), {
            "items_processed": result.predictions_updated + result.predictions_created,
            "metadata": {
                "updated": result.predictions_updated,
                "created": result.predictions_created,
                "links": result.links_updated,
            },
            "errors": result.errors,
        })()

    async def _run_vibe_calculate(
        self,
        entity_type: str | None = None,
        sport_id: str | None = None,
        **kwargs,
    ) -> Any:
        """Run vibe calculation job."""
        from .vibe_calculator import VibeCalculatorJob, SentimentSampler

        # First, sample sentiment from recent mentions
        sampler = SentimentSampler(self.db)
        samples_created = sampler.sample_from_mentions()

        # Then calculate vibe scores
        job = VibeCalculatorJob(self.db)
        result = job.run(entity_type, sport_id)

        return type("Result", (), {
            "items_processed": result.entities_updated + result.entities_created,
            "metadata": {
                "updated": result.entities_updated,
                "created": result.entities_created,
                "samples": result.samples_processed,
                "new_samples": samples_created,
            },
            "errors": result.errors,
        })()

    async def _run_performance_predict(self, sport_id: str | None = None, **kwargs) -> Any:
        """Run performance prediction job."""
        from .prediction_refresh import PerformancePredictionRefresh

        job = PerformancePredictionRefresh(self.db)
        result = job.refresh_upcoming(sport_id)

        return type("Result", (), {
            "items_processed": result.predictions_created,
            "metadata": {},
            "errors": result.errors,
        })()

    async def _run_similarity_update(self, sport_id: str | None = None, **kwargs) -> Any:
        """Run similarity recomputation job."""
        # This would use the SimilarityEngine to recompute embeddings
        # For now, return a placeholder
        logger.info("Similarity update not yet fully implemented")
        return type("Result", (), {
            "items_processed": 0,
            "metadata": {"status": "not_implemented"},
            "errors": [],
        })()

    def _record_run(self, result: JobResult) -> None:
        """Record job run in database."""
        try:
            self.db.execute(
                """
                INSERT INTO ml_job_runs (
                    job_name, status, started_at, completed_at,
                    duration_seconds, items_processed, errors, metadata
                ) VALUES (%s, %s, %s, %s, %s, %s, %s, %s)
                """,
                (
                    result.job_name,
                    result.status.value,
                    result.started_at,
                    result.completed_at,
                    result.duration_seconds,
                    result.items_processed,
                    result.errors,
                    result.metadata,
                ),
            )
        except Exception as e:
            logger.warning(f"Failed to record job run: {e}")

    def get_job_history(
        self,
        job_name: str | None = None,
        limit: int = 50,
    ) -> list[dict]:
        """Get recent job run history."""
        query = """
            SELECT job_name, status, started_at, completed_at,
                   duration_seconds, items_processed, errors, metadata
            FROM ml_job_runs
        """
        params = []

        if job_name:
            query += " WHERE job_name = %s"
            params.append(job_name)

        query += " ORDER BY started_at DESC LIMIT %s"
        params.append(limit)

        return self.db.fetchall(query, tuple(params))

    def get_job_stats(self) -> dict[str, dict]:
        """Get statistics for all jobs."""
        stats = {}

        for job_name in self.jobs:
            result = self.db.fetchone(
                """
                SELECT
                    COUNT(*) as total_runs,
                    COUNT(*) FILTER (WHERE status = 'completed') as successful,
                    COUNT(*) FILTER (WHERE status = 'failed') as failed,
                    AVG(duration_seconds) as avg_duration,
                    MAX(started_at) as last_run
                FROM ml_job_runs
                WHERE job_name = %s
                  AND started_at > NOW() - INTERVAL '7 days'
                """,
                (job_name,),
            )

            if result:
                stats[job_name] = {
                    "total_runs": result["total_runs"] or 0,
                    "successful": result["successful"] or 0,
                    "failed": result["failed"] or 0,
                    "avg_duration_seconds": float(result["avg_duration"] or 0),
                    "last_run": result["last_run"],
                    "enabled": self.jobs[job_name].enabled,
                    "interval_minutes": self.jobs[job_name].interval_minutes,
                }

        return stats
