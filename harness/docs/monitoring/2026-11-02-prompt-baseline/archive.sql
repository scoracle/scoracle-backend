BEGIN TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY;
SELECT jsonb_build_object('kind','generation','row',to_jsonb(l)) FROM cognition_ledger l
 WHERE generated_at>=:'since'::timestamptz AND stage IN ('narratives','vibe','transfers');
SELECT jsonb_build_object('kind','journalist','row',to_jsonb(p),'qualified_at_capture',coverage_character_visible(candidate_edition,'narratives',entity_type,entity_id,sport))
 FROM news_summaries p WHERE generated_at>=:'since'::timestamptz;
SELECT jsonb_build_object('kind','influencer','row',to_jsonb(p),'qualified_at_capture',coverage_character_visible(candidate_edition,'vibe',entity_type,entity_id,sport))
 FROM vibe_scores p WHERE generated_at>=:'since'::timestamptz;
SELECT jsonb_build_object('kind','insider','row',to_jsonb(p),'qualified_at_capture',coverage_character_visible(candidate_edition,'transfers',entity_type,entity_id,sport))
 FROM insider_scores p WHERE generated_at>=:'since'::timestamptz;
SELECT jsonb_build_object('kind','finding','row',to_jsonb(p)) FROM transfer_rumors p WHERE generated_at>=:'since'::timestamptz;
SELECT jsonb_build_object('kind','assignment','row',to_jsonb(a)) FROM coverage_assignments a
 WHERE finished_at>=:'since'::timestamptz AND stage IN ('narratives','vibe','transfers');
SELECT jsonb_build_object('kind','delivery','row',to_jsonb(d)-'product_ref') FROM classifier_spectrum_deliveries d
 WHERE updated_at>=:'since'::timestamptz AND character IN ('journalist','influencer','insider');
COMMIT;
