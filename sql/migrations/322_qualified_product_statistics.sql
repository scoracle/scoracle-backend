-- New nullable edition columns have no statistics until ANALYZE. Their initial
-- visibility estimates otherwise made the transfers board repeat identity scans.
ANALYZE public.news_summaries, public.vibe_scores, public.stat_summaries,
    public.insider_scores, public.transfer_rumors, public.momentum_summaries,
    public.sigil_synthesis, public.rating_history, public.momentum_scores;
