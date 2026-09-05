package db

import (
	"os"
	"strings"
	"testing"
)

func TestUniversalEntitiesStatementContract(t *testing.T) {
	stmt := universalEntitiesStatement

	required := []string{
		"'page', 'entities'",
		"'total_entities'",
		"'entities'",
		"'id', id",
		"'type', type",
		"'sport', sport",
		"'name', name",
		"'aliases', to_jsonb(aliases)",
		"'search_tokens', to_jsonb(search_tokens)",
		"FROM public.players p",
		"FROM public.teams t",
		"FROM public.persons pe",
		"LEFT JOIN public.leagues l",
		"p.sport IN ('NBA', 'NFL', 'FOOTBALL')",
		"t.sport IN ('NBA', 'NFL', 'FOOTBALL')",
		"pe.sport IN ('NBA', 'NFL', 'FOOTBALL')",
		"lower(p.sport) AS sport",
		"lower(t.sport) AS sport",
		"lower(pe.sport) AS sport",
		"'person'::text AS type",
		"UNION ALL",
		// Fix B: the version stamp must be computed from the source tables, never NOW() —
		// a churning stamp defeats frontend revalidation.
		"FROM public.persons)))",
	}
	for _, needle := range required {
		if !strings.Contains(stmt, needle) {
			t.Fatalf("universal entities statement missing %q", needle)
		}
	}
}

func TestUniversalEntitiesStatementIsLightweight(t *testing.T) {
	forbiddenJSONKeys := []string{
		"'meta'",
		"'stat_definitions'",
		"'photo_url'",
		"'logo_url'",
		"'leagues'",
		"'venue_name'",
		"'venue_capacity'",
		"'raw_response'",
	}
	for _, needle := range forbiddenJSONKeys {
		if strings.Contains(universalEntitiesStatement, needle) {
			t.Fatalf("universal entities statement must not emit heavy field %q", needle)
		}
	}
}

func TestUniversalEntitiesStatementDoesNotReuseSportAutofillViews(t *testing.T) {
	for _, needle := range []string{
		"nba.autofill_entities",
		"nfl.autofill_entities",
		"football.autofill_entities",
	} {
		if strings.Contains(universalEntitiesStatement, needle) {
			t.Fatalf("universal entities statement must not reuse stat-filtered sport autofill view %q", needle)
		}
	}
}

func TestCurrentIdentityContractInDBStatements(t *testing.T) {
	srcBytes, err := os.ReadFile("db.go")
	if err != nil {
		t.Fatalf("read db.go: %v", err)
	}
	src := string(srcBytes)

	if !strings.Contains(universalEntitiesStatement, "public.player_current_identity") {
		t.Fatalf("universal entities must resolve team/position from player_current_identity")
	}
	if strings.Contains(src, "SELECT ps.team_id FROM public.player_stats ps") {
		t.Fatalf("db statements must not derive current team directly from latest player_stats")
	}
	for _, needle := range []string{
		"LEFT JOIN public.player_current_identity pci",
		"pci.team_id = l.team_id AND COALESCE(l.direction, '') = 'incoming'",
		"pci.team_id <> l.team_id AND COALESCE(l.direction, '') = 'outgoing'",
	} {
		if !strings.Contains(src, needle) {
			t.Fatalf("db transfer reads missing current-identity guard %q", needle)
		}
	}
}

func TestSportMetaUsesSportScopedAutofillVersions(t *testing.T) {
	srcBytes, err := os.ReadFile("db.go")
	if err != nil {
		t.Fatalf("read db.go: %v", err)
	}
	src := string(srcBytes)

	for _, needle := range []string{
		"public.sport_autofill_versions WHERE sport = 'NBA'",
		"public.sport_autofill_versions WHERE sport = 'NFL'",
		"public.sport_autofill_versions WHERE sport = 'FOOTBALL'",
		"'meta_version', (SELECT version::text FROM meta_info)",
		"'autofill_status', (SELECT status FROM meta_info)",
	} {
		if !strings.Contains(src, needle) {
			t.Fatalf("sport meta statements missing autofill version contract %q", needle)
		}
	}
	forbidden := []string{
		"MAX(updated_at) FROM public.players WHERE sport = 'NBA'",
		"MAX(updated_at) FROM public.players WHERE sport = 'NFL'",
		"MAX(updated_at) FROM public.players WHERE sport = 'FOOTBALL'",
	}
	for _, needle := range forbidden {
		if strings.Contains(src, needle) {
			t.Fatalf("sport meta version must not be derived from unrelated player timestamps: %q", needle)
		}
	}
}
