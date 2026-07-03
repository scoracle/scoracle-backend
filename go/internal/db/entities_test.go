package db

import (
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
		"LEFT JOIN public.leagues l",
		"p.sport IN ('NBA', 'NFL', 'FOOTBALL')",
		"t.sport IN ('NBA', 'NFL', 'FOOTBALL')",
		"lower(p.sport) AS sport",
		"lower(t.sport) AS sport",
		"UNION ALL",
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
